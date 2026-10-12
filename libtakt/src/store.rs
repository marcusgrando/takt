use crate::models::{
    Action, CalendarAccessStatus, ExecutionLog, Schedule, Task, TaskDto, TaskHealth,
};
use crate::platform::PlatformBridge;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

/// Execution logs kept per task. Each row can hold up to 2 MiB of output, so
/// unbounded history would grow the database forever. The history view reads
/// at most 500 rows.
const MAX_LOGS_PER_TASK: i64 = 500;

/// A calendar dispatch row that has status = 'scheduled' and is waiting to fire.
#[derive(Debug, Clone)]
pub struct PendingDispatch {
    pub task_id: String,
    pub event_id: String,
    pub event_start: String,
    pub trigger_at: String,
}

pub struct TaskStore {
    pool: SqlitePool,
    bridge: Arc<dyn PlatformBridge>,
}

impl TaskStore {
    pub fn new(pool: SqlitePool, bridge: Arc<dyn PlatformBridge>) -> Self {
        Self { pool, bridge }
    }

    /// Populate `TaskDto.health` for every Calendar-scheduled task in the slice
    /// using at most two bridge calls. Non-Calendar tasks stay Healthy.
    fn populate_health(&self, tasks: &mut [TaskDto]) {
        let has_calendar_task = tasks
            .iter()
            .any(|t| matches!(t.schedule, Schedule::Calendar { .. }));
        if !has_calendar_task {
            return;
        }
        let status = match self.bridge.get_calendar_access_status() {
            Ok(s) => s,
            Err(_) => CalendarAccessStatus::Denied,
        };
        match status {
            CalendarAccessStatus::NotDetermined => {
                for t in tasks.iter_mut() {
                    if matches!(t.schedule, Schedule::Calendar { .. }) {
                        t.health = TaskHealth::CalendarAccessNotDetermined;
                    }
                }
            }
            CalendarAccessStatus::Denied => {
                for t in tasks.iter_mut() {
                    if matches!(t.schedule, Schedule::Calendar { .. }) {
                        t.health = TaskHealth::CalendarAccessDenied;
                    }
                }
            }
            CalendarAccessStatus::Authorized => {
                let known_ids: HashSet<String> = match self.bridge.list_calendars() {
                    Ok(list) => list.into_iter().map(|c| c.id).collect(),
                    Err(_) => HashSet::new(),
                };
                for t in tasks.iter_mut() {
                    if let Schedule::Calendar { calendar_id, .. } = &t.schedule {
                        if known_ids.contains(calendar_id) {
                            t.health = TaskHealth::Healthy;
                        } else {
                            t.health = TaskHealth::CalendarNotFound;
                        }
                    }
                }
            }
        }
    }

    pub async fn list_tasks(&self) -> anyhow::Result<Vec<TaskDto>> {
        let rows: Vec<Task> = sqlx::query_as("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        // A single unreadable row (e.g. written by a newer build) must not hide
        // or unschedule every other task.
        let mut dtos: Vec<TaskDto> = rows
            .iter()
            .filter_map(|t| match t.to_dto() {
                Ok(dto) => Some(dto),
                Err(error) => {
                    eprintln!("[takt] skipping unreadable task {}: {}", t.id, error);
                    None
                }
            })
            .collect();
        self.populate_health(&mut dtos);
        Ok(dtos)
    }

    pub async fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskDto>> {
        let row: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(t) => {
                let mut v = vec![t.to_dto().map_err(|e| anyhow::anyhow!(e))?];
                self.populate_health(&mut v);
                Ok(Some(v.into_iter().next().unwrap()))
            }
            None => Ok(None),
        }
    }

    pub async fn create_task(
        &self,
        name: String,
        description: Option<String>,
        run_if_missed: bool,
        notify_on_run: bool,
        schedule: Schedule,
        action: Action,
    ) -> anyhow::Result<TaskDto> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let schedule_json = serde_json::to_string(&schedule)?;
        let action_json = serde_json::to_string(&action)?;

        sqlx::query(
            "INSERT INTO tasks (id, name, description, enabled, run_if_missed, notify_on_run, schedule_json, action_json, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&name)
        .bind(&description)
        .bind(run_if_missed as i64)
        .bind(notify_on_run as i64)
        .bind(&schedule_json)
        .bind(&action_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get_task(&id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task not found after insert"))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_task(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<Option<String>>,
        enabled: Option<bool>,
        run_if_missed: Option<bool>,
        notify_on_run: Option<bool>,
        schedule: Option<Schedule>,
        action: Option<Action>,
    ) -> anyhow::Result<TaskDto> {
        let existing = self
            .get_task(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;

        let name = name.unwrap_or(existing.name);
        let description = description.unwrap_or(existing.description);
        let enabled = enabled.unwrap_or(existing.enabled);
        let run_if_missed = run_if_missed.unwrap_or(existing.run_if_missed);
        let notify_on_run = notify_on_run.unwrap_or(existing.notify_on_run);
        let schedule = schedule.unwrap_or(existing.schedule);
        let action = action.unwrap_or(existing.action);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE tasks SET name=?, description=?, enabled=?, run_if_missed=?, notify_on_run=?, schedule_json=?, action_json=?, updated_at=? WHERE id=?"
        )
        .bind(&name)
        .bind(&description)
        .bind(enabled as i64)
        .bind(run_if_missed as i64)
        .bind(notify_on_run as i64)
        .bind(serde_json::to_string(&schedule)?)
        .bind(serde_json::to_string(&action)?)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        self.get_task(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task not found after update"))
    }

    pub async fn delete_task(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_last_run(&self, id: &str, next_run: Option<String>) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE tasks SET last_run_at=?, next_run_at=?, updated_at=? WHERE id=?")
            .bind(&now)
            .bind(next_run)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn log_execution(
        &self,
        task_id: &str,
        status: &str,
        stdout: Option<String>,
        stderr: Option<String>,
        error: Option<String>,
    ) -> anyhow::Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO execution_logs (id, task_id, started_at, finished_at, status, stdout, stderr, error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(task_id)
        .bind(&now)
        .bind(&now)
        .bind(status)
        .bind(stdout)
        .bind(stderr)
        .bind(error)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM execution_logs WHERE task_id = ? AND id NOT IN (
                 SELECT id FROM execution_logs WHERE task_id = ?
                 ORDER BY started_at DESC LIMIT ?)",
        )
        .bind(task_id)
        .bind(task_id)
        .bind(MAX_LOGS_PER_TASK)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_logs(
        &self,
        task_id: Option<&str>,
        limit: i64,
    ) -> anyhow::Result<Vec<ExecutionLog>> {
        let logs = if let Some(tid) = task_id {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs WHERE task_id = ? ORDER BY started_at DESC LIMIT ?",
            )
            .bind(tid)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs ORDER BY started_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(logs)
    }

    // ── calendar_dispatches helpers ───────────────────────────────────────────

    /// Reserve an occurrence, returning false when it already exists.
    pub async fn insert_calendar_dispatch(
        &self,
        task_id: &str,
        event_id: &str,
        event_start: &str,
        status: &str,     // "scheduled" | "dispatched"
        trigger_at: &str, // ISO 8601
    ) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let dispatched_at: Option<&str> = if status == "dispatched" {
            Some(now.as_str())
        } else {
            None
        };
        let result = sqlx::query(
            "INSERT OR IGNORE INTO calendar_dispatches
             (task_id, event_id, event_start, status, trigger_at, reserved_at, dispatched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task_id)
        .bind(event_id)
        .bind(event_start)
        .bind(status)
        .bind(trigger_at)
        .bind(&now)
        .bind(dispatched_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Atomically claim a scheduled occurrence, returning true only to its owner.
    pub async fn mark_calendar_dispatch_dispatched(
        &self,
        task_id: &str,
        event_id: &str,
        event_start: &str,
    ) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE calendar_dispatches
             SET status = 'dispatched', dispatched_at = ?
             WHERE task_id = ? AND event_id = ? AND event_start = ? AND status = 'scheduled'",
        )
        .bind(&now)
        .bind(task_id)
        .bind(event_id)
        .bind(event_start)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Delete a single dispatch row by its composite primary key.
    pub async fn delete_calendar_dispatch(
        &self,
        task_id: &str,
        event_id: &str,
        event_start: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM calendar_dispatches
             WHERE task_id = ? AND event_id = ? AND event_start = ?",
        )
        .bind(task_id)
        .bind(event_id)
        .bind(event_start)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns `true` when a row exists for the given composite key.
    pub async fn dispatch_exists(
        &self,
        task_id: &str,
        event_id: &str,
        event_start: &str,
    ) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM calendar_dispatches
             WHERE task_id = ? AND event_id = ? AND event_start = ?
             LIMIT 1",
        )
        .bind(task_id)
        .bind(event_id)
        .bind(event_start)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// Remove every 'scheduled' row that belongs to `task_id`.
    /// Used when re-syncing a task's upcoming events so stale rows are cleared.
    pub async fn delete_scheduled_dispatches_for_task(&self, task_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM calendar_dispatches
             WHERE task_id = ? AND status = 'scheduled'",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete all rows whose `event_start` is more than 24 hours in the past.
    pub async fn prune_old_calendar_dispatches(&self) -> anyhow::Result<()> {
        // Markers must outlive the poller's catch-up horizon or a consumed slot
        // becomes eligible again, and must stay while a pending reservation of
        // the same event can still be recovered (a moved event has two keys).
        // Pending rows are kept for restart recovery until they are a week old.
        let now = chrono::Utc::now();
        let marker_cutoff = (now
            - chrono::Duration::seconds(2 * crate::calendar::CALENDAR_MAX_CATCH_UP_SECS))
        .to_rfc3339();
        let pending_cutoff = (now - chrono::Duration::days(7)).to_rfc3339();
        sqlx::query(
            "DELETE FROM calendar_dispatches
             WHERE status = 'scheduled' AND event_start < ?",
        )
        .bind(&pending_cutoff)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM calendar_dispatches
             WHERE status = 'dispatched' AND event_start < ?
               AND NOT EXISTS (
                   SELECT 1 FROM calendar_dispatches AS pending
                   WHERE pending.task_id = calendar_dispatches.task_id
                     AND pending.event_id = calendar_dispatches.event_id
                     AND pending.status = 'scheduled')",
        )
        .bind(&marker_cutoff)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Atomically claims a reserved slot whose event moved to `moved_start`,
    /// and marks the moved slot dispatched too. Returns false when the caller
    /// must not execute: either the reserved slot was already claimed (no
    /// change), or the moved slot already ran (the reserved slot is consumed so
    /// restart recovery cannot replay it).
    pub async fn claim_moved_calendar_dispatch(
        &self,
        task_id: &str,
        event_id: &str,
        reserved_start: &str,
        moved_start: &str,
        moved_trigger_at: &str,
    ) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        let claim = "UPDATE calendar_dispatches
             SET status = 'dispatched', dispatched_at = ?
             WHERE task_id = ? AND event_id = ? AND event_start = ? AND status = 'scheduled'";
        let reserved = sqlx::query(claim)
            .bind(&now)
            .bind(task_id)
            .bind(event_id)
            .bind(reserved_start)
            .execute(&mut *tx)
            .await?;
        if reserved.rows_affected() != 1 {
            return Ok(false);
        }
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO calendar_dispatches
             (task_id, event_id, event_start, status, trigger_at, reserved_at, dispatched_at)
             VALUES (?, ?, ?, 'dispatched', ?, ?, ?)",
        )
        .bind(task_id)
        .bind(event_id)
        .bind(moved_start)
        .bind(moved_trigger_at)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() != 1 {
            let moved = sqlx::query(claim)
                .bind(&now)
                .bind(task_id)
                .bind(event_id)
                .bind(moved_start)
                .execute(&mut *tx)
                .await?;
            if moved.rows_affected() != 1 {
                tx.commit().await?;
                return Ok(false);
            }
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Return all rows whose status is 'scheduled', in trigger_at order.
    pub async fn list_pending_dispatches(&self) -> anyhow::Result<Vec<PendingDispatch>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT task_id, event_id, event_start, trigger_at
             FROM calendar_dispatches
             WHERE status = 'scheduled'
             ORDER BY trigger_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(task_id, event_id, event_start, trigger_at)| PendingDispatch {
                    task_id,
                    event_id,
                    event_start,
                    trigger_at,
                },
            )
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Shell;
    use crate::platform::PlatformBridge;
    use sqlx::sqlite::SqlitePoolOptions;

    struct NullBridge;
    impl PlatformBridge for NullBridge {
        fn send_notification(&self, _: String, _: String, _: bool) {}
        fn run_on_main_sync(&self, _: u64) {}
        fn get_user_activity_snapshot(&self) -> crate::models::UserActivitySnapshot {
            crate::models::UserActivitySnapshot {
                session_active: true,
                eligibility_generation: 0,
                input_event_count: 0,
                eligibility_input_event_count: 0,
                last_input_at_unix_millis: None,
            }
        }
        fn get_calendar_access_status(
            &self,
        ) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Ok(crate::models::CalendarAccessStatus::Authorized)
        }
        fn request_calendar_access(
            &self,
        ) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Ok(crate::models::CalendarAccessStatus::Authorized)
        }
        fn list_calendars(
            &self,
        ) -> Result<Vec<crate::models::CalendarInfo>, crate::error::TaktError> {
            Ok(vec![])
        }
        fn fetch_events_in_window(
            &self,
            _: String,
            _: u32,
            _: u32,
        ) -> Result<Vec<crate::models::CalendarEvent>, crate::error::TaktError> {
            Ok(vec![])
        }
        fn fetch_event_instance(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> Result<Option<crate::models::CalendarEvent>, crate::error::TaktError> {
            Ok(None)
        }
    }

    async fn test_store() -> TaskStore {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        TaskStore::new(pool, Arc::new(NullBridge) as Arc<dyn PlatformBridge>)
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let store = test_store().await;
        let task = store
            .create_task(
                "Test Task".to_string(),
                None,
                true,
                false,
                Schedule::Cron {
                    expression: "0 8 * * 1-5".to_string(),
                },
                Action::RunCommand {
                    command: "echo test".to_string(),
                    args: vec![],
                    shell: Shell::Sh,
                },
            )
            .await
            .unwrap();

        assert_eq!(task.name, "Test Task");
        assert!(task.enabled);

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_update_task() {
        let store = test_store().await;
        let task = store
            .create_task(
                "Original".to_string(),
                None,
                true,
                false,
                Schedule::DailyFirstUse { delay_minutes: 5 },
                Action::Notify {
                    title: "Hi".to_string(),
                    body: "World".to_string(),
                    sound: false,
                },
            )
            .await
            .unwrap();

        let updated = store
            .update_task(
                &task.id,
                Some("Updated".to_string()),
                None,
                Some(false),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(updated.name, "Updated");
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let store = test_store().await;
        let task = store
            .create_task(
                "Delete Me".to_string(),
                None,
                true,
                false,
                Schedule::DailyFirstUse { delay_minutes: 5 },
                Action::Notify {
                    title: "Hi".to_string(),
                    body: "World".to_string(),
                    sound: false,
                },
            )
            .await
            .unwrap();

        store.delete_task(&task.id).await.unwrap();
        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_log_execution() {
        let store = test_store().await;
        let task = store
            .create_task(
                "Log Test".to_string(),
                None,
                true,
                false,
                Schedule::DailyFirstUse { delay_minutes: 5 },
                Action::Notify {
                    title: "Hi".to_string(),
                    body: "World".to_string(),
                    sound: false,
                },
            )
            .await
            .unwrap();

        store
            .log_execution(&task.id, "success", Some("output".to_string()), None, None)
            .await
            .unwrap();

        let logs = store.list_logs(Some(&task.id), 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "success");
    }

    #[tokio::test]
    async fn log_execution_keeps_only_recent_rows_per_task() {
        let store = test_store().await;
        let mut task_ids = Vec::new();
        for name in ["Busy", "Quiet"] {
            let task = store
                .create_task(
                    name.to_string(),
                    None,
                    true,
                    false,
                    Schedule::DailyFirstUse { delay_minutes: 5 },
                    Action::Notify {
                        title: "Hi".to_string(),
                        body: "World".to_string(),
                        sound: false,
                    },
                )
                .await
                .unwrap();
            task_ids.push(task.id);
        }
        let (busy, quiet) = (&task_ids[0], &task_ids[1]);
        for _ in 0..MAX_LOGS_PER_TASK + 3 {
            store
                .log_execution(busy, "success", None, None, None)
                .await
                .unwrap();
        }
        store
            .log_execution(quiet, "success", None, None, None)
            .await
            .unwrap();

        let busy_logs = store.list_logs(Some(busy), 1_000).await.unwrap();
        assert_eq!(busy_logs.len(), MAX_LOGS_PER_TASK as usize);
        assert_eq!(store.list_logs(Some(quiet), 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_tasks_skips_unreadable_rows() {
        let store = test_store().await;
        store
            .create_task(
                "Readable".to_string(),
                None,
                true,
                false,
                Schedule::DailyFirstUse { delay_minutes: 5 },
                Action::Notify {
                    title: "Hi".to_string(),
                    body: "World".to_string(),
                    sound: false,
                },
            )
            .await
            .unwrap();
        let broken = store
            .create_task(
                "Broken".to_string(),
                None,
                true,
                false,
                Schedule::DailyFirstUse { delay_minutes: 5 },
                Action::Notify {
                    title: "Hi".to_string(),
                    body: "World".to_string(),
                    sound: false,
                },
            )
            .await
            .unwrap();
        sqlx::query("UPDATE tasks SET schedule_json = '{\"type\":\"FromTheFuture\"}' WHERE id = ?")
            .bind(&broken.id)
            .execute(&store.pool)
            .await
            .unwrap();

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "Readable");
    }

    // ── calendar_dispatches tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_insert_and_dispatch_exists() {
        let store = test_store().await;
        let exists = store
            .dispatch_exists("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap();
        assert!(!exists, "should not exist before insert");

        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();

        let exists = store
            .dispatch_exists("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap();
        assert!(exists, "should exist after insert");
    }

    #[tokio::test]
    async fn test_insert_or_ignore_is_idempotent() {
        let store = test_store().await;
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();
        // Second insert with same PK should be silently ignored.
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();

        let pending = store.list_pending_dispatches().await.unwrap();
        assert_eq!(
            pending.len(),
            1,
            "INSERT OR IGNORE must not duplicate the row"
        );
    }

    #[tokio::test]
    async fn test_mark_dispatched() {
        let store = test_store().await;
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();

        // Before marking: should appear in pending list
        let pending = store.list_pending_dispatches().await.unwrap();
        assert_eq!(pending.len(), 1);

        store
            .mark_calendar_dispatch_dispatched("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap();

        // After marking: no longer pending
        let pending = store.list_pending_dispatches().await.unwrap();
        assert_eq!(
            pending.len(),
            0,
            "dispatched row should not appear in pending list"
        );
    }

    #[tokio::test]
    async fn test_delete_calendar_dispatch() {
        let store = test_store().await;
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();
        assert!(store
            .dispatch_exists("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap());

        store
            .delete_calendar_dispatch("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap();
        assert!(!store
            .dispatch_exists("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_delete_scheduled_dispatches_for_task() {
        let store = test_store().await;
        // Insert two scheduled rows for task t1 and one dispatched row.
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();
        store
            .insert_calendar_dispatch(
                "t1",
                "e2",
                "2026-05-02T10:00:00Z",
                "scheduled",
                "2026-05-02T09:55:00Z",
            )
            .await
            .unwrap();
        store
            .insert_calendar_dispatch(
                "t1",
                "e3",
                "2026-05-03T10:00:00Z",
                "dispatched",
                "2026-05-03T09:55:00Z",
            )
            .await
            .unwrap();
        // Insert a scheduled row for a different task.
        store
            .insert_calendar_dispatch(
                "t2",
                "e4",
                "2026-05-01T11:00:00Z",
                "scheduled",
                "2026-05-01T10:55:00Z",
            )
            .await
            .unwrap();

        store
            .delete_scheduled_dispatches_for_task("t1")
            .await
            .unwrap();

        // t1 scheduled rows gone; t1 dispatched row and t2 row still there.
        assert!(!store
            .dispatch_exists("t1", "e1", "2026-05-01T10:00:00Z")
            .await
            .unwrap());
        assert!(!store
            .dispatch_exists("t1", "e2", "2026-05-02T10:00:00Z")
            .await
            .unwrap());
        assert!(store
            .dispatch_exists("t1", "e3", "2026-05-03T10:00:00Z")
            .await
            .unwrap());
        assert!(store
            .dispatch_exists("t2", "e4", "2026-05-01T11:00:00Z")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn prune_keeps_recent_pending_rows_and_dispatch_markers() {
        let store = test_store().await;
        let old = &(chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339();
        let recent = (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339();
        store
            .insert_calendar_dispatch("t1", "pending", old, "scheduled", old)
            .await
            .unwrap();
        store
            .insert_calendar_dispatch("t1", "recent", &recent, "dispatched", &recent)
            .await
            .unwrap();

        store.prune_old_calendar_dispatches().await.unwrap();

        assert!(store.dispatch_exists("t1", "pending", old).await.unwrap());
        assert!(store
            .dispatch_exists("t1", "recent", &recent)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn prune_keeps_markers_of_events_with_pending_reservations() {
        let store = test_store().await;
        let reserved = (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339();
        let moved = (chrono::Utc::now() - chrono::Duration::days(3) - chrono::Duration::hours(1))
            .to_rfc3339();
        let ancient = (chrono::Utc::now() - chrono::Duration::days(8)).to_rfc3339();
        store
            .insert_calendar_dispatch("t1", "e1", &reserved, "scheduled", &reserved)
            .await
            .unwrap();
        store
            .insert_calendar_dispatch("t1", "e1", &moved, "dispatched", &moved)
            .await
            .unwrap();
        store
            .insert_calendar_dispatch("t1", "e2", &ancient, "scheduled", &ancient)
            .await
            .unwrap();

        store.prune_old_calendar_dispatches().await.unwrap();

        assert!(store.dispatch_exists("t1", "e1", &moved).await.unwrap());
        assert!(!store.dispatch_exists("t1", "e2", &ancient).await.unwrap());
    }

    #[tokio::test]
    async fn test_prune_old_calendar_dispatches() {
        let store = test_store().await;
        // An event_start well in the past (should be pruned).
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2020-01-01T00:00:00Z",
                "dispatched",
                "2020-01-01T00:00:00Z",
            )
            .await
            .unwrap();
        // An event_start in the future (should survive).
        store
            .insert_calendar_dispatch(
                "t1",
                "e2",
                "2099-01-01T00:00:00Z",
                "scheduled",
                "2099-01-01T00:00:00Z",
            )
            .await
            .unwrap();

        store.prune_old_calendar_dispatches().await.unwrap();

        assert!(
            !store
                .dispatch_exists("t1", "e1", "2020-01-01T00:00:00Z")
                .await
                .unwrap(),
            "old row should be pruned"
        );
        assert!(
            store
                .dispatch_exists("t1", "e2", "2099-01-01T00:00:00Z")
                .await
                .unwrap(),
            "future row should survive"
        );
    }

    #[tokio::test]
    async fn test_list_pending_dispatches_ordering() {
        let store = test_store().await;
        store
            .insert_calendar_dispatch(
                "t1",
                "e2",
                "2026-05-02T10:00:00Z",
                "scheduled",
                "2026-05-02T09:55:00Z",
            )
            .await
            .unwrap();
        store
            .insert_calendar_dispatch(
                "t1",
                "e1",
                "2026-05-01T10:00:00Z",
                "scheduled",
                "2026-05-01T09:55:00Z",
            )
            .await
            .unwrap();
        // A dispatched row — should not appear in the list.
        store
            .insert_calendar_dispatch(
                "t1",
                "e3",
                "2026-05-03T10:00:00Z",
                "dispatched",
                "2026-05-03T09:55:00Z",
            )
            .await
            .unwrap();

        let pending = store.list_pending_dispatches().await.unwrap();
        assert_eq!(pending.len(), 2);
        // Results should be ordered by trigger_at ascending.
        assert_eq!(pending[0].event_id, "e1");
        assert_eq!(pending[1].event_id, "e2");
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;
    use crate::models::*;
    use crate::platform::PlatformBridge;
    use std::sync::{Arc, Mutex};

    struct MockBridge {
        status: CalendarAccessStatus,
        calendars: Vec<CalendarInfo>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl MockBridge {
        fn new(status: CalendarAccessStatus, calendars: Vec<CalendarInfo>) -> Self {
            Self {
                status,
                calendars,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl PlatformBridge for MockBridge {
        fn send_notification(&self, _: String, _: String, _: bool) {}
        fn run_on_main_sync(&self, _: u64) {}
        fn get_user_activity_snapshot(&self) -> crate::models::UserActivitySnapshot {
            crate::models::UserActivitySnapshot {
                session_active: true,
                eligibility_generation: 0,
                input_event_count: 0,
                eligibility_input_event_count: 0,
                last_input_at_unix_millis: None,
            }
        }
        fn get_calendar_access_status(
            &self,
        ) -> Result<CalendarAccessStatus, crate::error::TaktError> {
            self.calls.lock().unwrap().push("get_status");
            Ok(self.status.clone())
        }
        fn request_calendar_access(&self) -> Result<CalendarAccessStatus, crate::error::TaktError> {
            Ok(self.status.clone())
        }
        fn list_calendars(&self) -> Result<Vec<CalendarInfo>, crate::error::TaktError> {
            self.calls.lock().unwrap().push("list_calendars");
            Ok(self.calendars.clone())
        }
        fn fetch_events_in_window(
            &self,
            _: String,
            _: u32,
            _: u32,
        ) -> Result<Vec<CalendarEvent>, crate::error::TaktError> {
            Ok(vec![])
        }
        fn fetch_event_instance(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> Result<Option<CalendarEvent>, crate::error::TaktError> {
            Ok(None)
        }
    }

    fn make_dto(schedule: Schedule) -> TaskDto {
        TaskDto {
            id: "t1".into(),
            name: "test".into(),
            description: None,
            enabled: true,
            run_if_missed: false,
            notify_on_run: false,
            schedule,
            action: Action::Settings {
                pane_url: "x".into(),
            },
            created_at: "".into(),
            updated_at: "".into(),
            last_run_at: None,
            next_run_at: None,
            health: TaskHealth::Healthy,
        }
    }

    fn store_with(bridge: Arc<MockBridge>) -> (TaskStore, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let pool = rt.block_on(async {
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("in-memory pool")
        });
        let store = TaskStore::new(pool, bridge as Arc<dyn PlatformBridge>);
        (store, rt)
    }

    #[test]
    fn non_calendar_task_stays_healthy_and_no_bridge_calls() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::Authorized, vec![]));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Cron {
            expression: "0 * * * *".into(),
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert!(bridge.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn calendar_task_not_determined() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::NotDetermined, vec![]));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarAccessNotDetermined);
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status"]);
    }

    #[test]
    fn calendar_task_denied() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::Denied, vec![]));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarAccessDenied);
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status"]);
    }

    #[test]
    fn calendar_task_healthy_when_calendar_present() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo {
                id: "cal-1".into(),
                title: "Work".into(),
                source: "Google".into(),
                color_hex: None,
            }],
        ));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert_eq!(
            bridge.calls.lock().unwrap().as_slice(),
            &["get_status", "list_calendars"]
        );
    }

    #[test]
    fn calendar_task_not_found_when_calendar_missing() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo {
                id: "cal-other".into(),
                title: "Other".into(),
                source: "Local".into(),
                color_hex: None,
            }],
        ));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarNotFound);
    }

    #[test]
    fn multiple_calendar_tasks_share_two_bridge_calls() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo {
                id: "cal-1".into(),
                title: "Work".into(),
                source: "Google".into(),
                color_hex: None,
            }],
        ));
        let (store, _rt) = store_with(Arc::clone(&bridge));
        let mut tasks = vec![
            make_dto(Schedule::Calendar {
                calendar_id: "cal-1".into(),
                title_contains: None,
                minutes_before: 5,
            }),
            make_dto(Schedule::Calendar {
                calendar_id: "cal-1".into(),
                title_contains: None,
                minutes_before: 10,
            }),
            make_dto(Schedule::Calendar {
                calendar_id: "cal-missing".into(),
                title_contains: None,
                minutes_before: 0,
            }),
        ];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert_eq!(tasks[1].health, TaskHealth::Healthy);
        assert_eq!(tasks[2].health, TaskHealth::CalendarNotFound);
        assert_eq!(
            bridge.calls.lock().unwrap().as_slice(),
            &["get_status", "list_calendars"]
        );
    }
}
