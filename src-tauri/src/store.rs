use crate::models::{Task, TaskDto, ExecutionLog, Schedule, Action};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TaskStore {
    pool: SqlitePool,
}

impl TaskStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_tasks(&self) -> anyhow::Result<Vec<TaskDto>> {
        let rows: Vec<Task> = sqlx::query_as("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(|t| t.to_dto().map_err(|e| anyhow::anyhow!(e))).collect()
    }

    pub async fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskDto>> {
        let row: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|t| t.to_dto().map_err(|e| anyhow::anyhow!(e))).transpose()
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

        self.get_task(&id).await?.ok_or_else(|| anyhow::anyhow!("Task not found after insert"))
    }

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
        let existing = self.get_task(id).await?
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

        self.get_task(id).await?.ok_or_else(|| anyhow::anyhow!("Task not found after update"))
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
        Ok(())
    }

    pub async fn list_logs(
        &self,
        task_id: Option<&str>,
        limit: i64,
    ) -> anyhow::Result<Vec<ExecutionLog>> {
        let logs = if let Some(tid) = task_id {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs WHERE task_id = ? ORDER BY started_at DESC LIMIT ?"
            )
            .bind(tid)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs ORDER BY started_at DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Shell;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_store() -> TaskStore {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        TaskStore::new(pool)
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let store = test_store().await;
        let task = store.create_task(
            "Test Task".to_string(),
            None,
            Schedule::Cron { expression: "0 8 * * 1-5".to_string() },
            Action::RunCommand {
                command: "echo test".to_string(),
                args: vec![],
                shell: Shell::Sh,
            }
        ).await.unwrap();

        assert_eq!(task.name, "Test Task");
        assert!(task.enabled);

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_update_task() {
        let store = test_store().await;
        let task = store.create_task(
            "Original".to_string(),
            None,
            Schedule::DailyFirstUse,
            Action::Notify { title: "Hi".to_string(), body: "World".to_string(), sound: false },
        ).await.unwrap();

        let updated = store.update_task(
            &task.id,
            Some("Updated".to_string()),
            None,
            Some(false),
            None,
            None,
            None,
        ).await.unwrap();

        assert_eq!(updated.name, "Updated");
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let store = test_store().await;
        let task = store.create_task(
            "Delete Me".to_string(),
            None,
            Schedule::DailyFirstUse,
            Action::Notify { title: "Hi".to_string(), body: "World".to_string(), sound: false },
        ).await.unwrap();

        store.delete_task(&task.id).await.unwrap();
        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_log_execution() {
        let store = test_store().await;
        let task = store.create_task(
            "Log Test".to_string(),
            None,
            Schedule::DailyFirstUse,
            Action::Notify { title: "Hi".to_string(), body: "World".to_string(), sound: false },
        ).await.unwrap();

        store.log_execution(&task.id, "success", Some("output".to_string()), None, None)
            .await.unwrap();

        let logs = store.list_logs(Some(&task.id), 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "success");
    }
}
