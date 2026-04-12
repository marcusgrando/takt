//! Calendar trigger poller and dispatch routine.
//!
//! See docs/superpowers/specs/2026-04-10-calendar-trigger-design.md §5.

use crate::executor::ActionExecutor;
use crate::models::{CalendarEvent, Schedule, TaskDto};
use crate::platform::PlatformBridge;
use crate::store::TaskStore;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const CALENDAR_POLL_INTERVAL_SECS: u64 = 300;
pub const CALENDAR_LOOKAHEAD_MARGIN_SECS: u64 = 60;
pub const CALENDAR_LOOKBACK_MARGIN_SECS: u64 = 60;
pub const DISPATCH_TOLERANCE_SECS: i64 = 2;

/// Key format for tokens parking in `AppScheduler::cancel_tokens`:
/// `calendar:{task_id}:{event_id}:{event_start}`
pub fn dispatch_token_key(task_id: &str, event_id: &str, event_start: &str) -> String {
    format!("calendar:{}:{}:{}", task_id, event_id, event_start)
}

/// Returns true if `key` belongs to a pending calendar dispatch for `task_id`.
pub fn is_calendar_key_for(key: &str, task_id: &str) -> bool {
    if let Some(rest) = key.strip_prefix("calendar:") {
        rest.starts_with(&format!("{}:", task_id))
    } else {
        false
    }
}

// ── CalendarPoller ────────────────────────────────────────────────────

pub struct CalendarPoller {
    store: Arc<TaskStore>,
    executor: Arc<dyn ActionExecutor>,
    bridge: Arc<dyn PlatformBridge>,
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    stop: CancellationToken,
}

impl CalendarPoller {
    pub fn new(
        store: Arc<TaskStore>,
        executor: Arc<dyn ActionExecutor>,
        bridge: Arc<dyn PlatformBridge>,
        cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    ) -> Self {
        Self {
            store,
            executor,
            bridge,
            cancel_tokens,
            stop: CancellationToken::new(),
        }
    }

    pub fn stop(&self) {
        self.stop.cancel();
    }

    pub async fn run(self: Arc<Self>) {
        // Immediate first tick so a task loaded at startup can reserve its
        // upcoming events without waiting up to CALENDAR_POLL_INTERVAL_SECS.
        // Without this, a trigger that falls within the first poll window
        // after app launch would be marked `skipped` (when run_if_missed=false)
        // even though the app was already running.
        if let Err(e) = self.tick().await {
            eprintln!("[takt] calendar poller initial tick error: {}", e);
        }
        loop {
            tokio::select! {
                _ = self.stop.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(CALENDAR_POLL_INTERVAL_SECS)) => {
                    if let Err(e) = self.tick().await {
                        eprintln!("[takt] calendar poller tick error: {}", e);
                    }
                }
            }
        }
    }

    async fn tick(&self) -> anyhow::Result<()> {
        let tasks = self.store.list_tasks().await?;
        let calendar_tasks: Vec<TaskDto> = tasks
            .into_iter()
            .filter(|t| t.enabled && matches!(t.schedule, Schedule::Calendar { .. }))
            .collect();
        if calendar_tasks.is_empty() {
            let _ = self.store.prune_old_calendar_dispatches().await;
            return Ok(());
        }

        // Group by calendar_id to minimize bridge calls.
        let mut by_calendar: HashMap<String, Vec<TaskDto>> = HashMap::new();
        for t in calendar_tasks {
            if let Schedule::Calendar { calendar_id, .. } = &t.schedule {
                by_calendar.entry(calendar_id.clone()).or_default().push(t.clone());
            }
        }

        for (calendar_id, tasks) in by_calendar {
            let lookahead_minutes = tasks
                .iter()
                .filter_map(|t| match &t.schedule {
                    Schedule::Calendar { minutes_before, .. } => Some(*minutes_before as u64),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
                + (CALENDAR_POLL_INTERVAL_SECS + CALENDAR_LOOKAHEAD_MARGIN_SECS) / 60;
            let lookback_minutes =
                (CALENDAR_POLL_INTERVAL_SECS + CALENDAR_LOOKBACK_MARGIN_SECS) / 60;

            let events = match self.bridge.fetch_events_in_window(
                calendar_id.clone(),
                lookback_minutes as u32,
                lookahead_minutes as u32,
            ) {
                Ok(list) => list,
                Err(e) => {
                    eprintln!("[takt] fetch_events_in_window({}): {}", calendar_id, e);
                    continue;
                }
            };

            for task in &tasks {
                let (title_contains, minutes_before) = match &task.schedule {
                    Schedule::Calendar {
                        title_contains,
                        minutes_before,
                        ..
                    } => (title_contains.clone(), *minutes_before),
                    _ => continue,
                };
                for event in &events {
                    if let Some(needle) = &title_contains {
                        if !event.title.to_lowercase().contains(&needle.to_lowercase()) {
                            continue;
                        }
                    }
                    self.process_slot(task, event, minutes_before).await;
                }
            }
        }

        let _ = self.store.prune_old_calendar_dispatches().await;
        Ok(())
    }

    async fn process_slot(&self, task: &TaskDto, event: &CalendarEvent, minutes_before: u32) {
        let event_start = match DateTime::parse_from_rfc3339(&event.start) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return,
        };
        let trigger_at = event_start - Duration::minutes(minutes_before as i64);
        let now = Utc::now();
        let d = (trigger_at - now).num_seconds();

        // Skip if already known.
        if let Ok(true) = self
            .store
            .dispatch_exists(&task.id, &event.id, &event.start)
            .await
        {
            return;
        }

        // Skip if the trigger is past the next poll window — next tick will handle it.
        if d > CALENDAR_POLL_INTERVAL_SECS as i64 {
            return;
        }

        let trigger_at_iso = trigger_at.to_rfc3339();

        if d < -DISPATCH_TOLERANCE_SECS {
            // Missed slot.
            if task.run_if_missed {
                // Reserve as scheduled, then dispatch immediately.
                let _ = self
                    .store
                    .insert_calendar_dispatch(
                        &task.id,
                        &event.id,
                        &event.start,
                        "scheduled",
                        &trigger_at_iso,
                    )
                    .await;
                self.spawn_dispatch(task.id.clone(), event.id.clone(), event.start.clone(), 0)
                    .await;
            } else {
                // Consume the slot without running.
                let _ = self
                    .store
                    .insert_calendar_dispatch(
                        &task.id,
                        &event.id,
                        &event.start,
                        "dispatched",
                        &trigger_at_iso,
                    )
                    .await;
                let _ = self
                    .store
                    .log_execution(
                        &task.id,
                        "skipped",
                        None,
                        None,
                        Some("event start in the past; run_if_missed=false".to_string()),
                    )
                    .await;
            }
            return;
        }

        // Reserve and schedule a sleep until trigger_at (possibly zero seconds).
        let _ = self
            .store
            .insert_calendar_dispatch(
                &task.id,
                &event.id,
                &event.start,
                "scheduled",
                &trigger_at_iso,
            )
            .await;
        let delay_secs = d.max(0) as u64;
        self.spawn_dispatch(
            task.id.clone(),
            event.id.clone(),
            event.start.clone(),
            delay_secs,
        )
        .await;
    }

    async fn spawn_dispatch(
        &self,
        task_id: String,
        event_id: String,
        event_start: String,
        delay_secs: u64,
    ) {
        let key = dispatch_token_key(&task_id, &event_id, &event_start);
        let token = CancellationToken::new();
        let child = token.child_token();
        self.cancel_tokens.lock().await.insert(key.clone(), token);

        let store = Arc::clone(&self.store);
        let executor = Arc::clone(&self.executor);
        let bridge = Arc::clone(&self.bridge);
        let cancel_tokens = Arc::clone(&self.cancel_tokens);

        tokio::spawn(async move {
            tokio::select! {
                _ = child.cancelled() => {
                    cancel_tokens.lock().await.remove(&key);
                    return;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => {}
            }
            run_dispatch_pub(
                store,
                executor,
                bridge,
                task_id,
                event_id,
                event_start,
            )
            .await;
            cancel_tokens.lock().await.remove(&key);
        });
    }
}

// ── Fire-time dispatch ────────────────────────────────────────────────

pub async fn run_dispatch_pub(
    store: Arc<TaskStore>,
    executor: Arc<dyn ActionExecutor>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    event_id: String,
    event_start: String,
) {
    // 1. Re-read the task.
    let task = match store.get_task(&task_id).await {
        Ok(Some(t)) if t.enabled => t,
        Ok(Some(_)) => {
            // Disabled between reservation and fire time.
            let _ = store
                .delete_calendar_dispatch(&task_id, &event_id, &event_start)
                .await;
            let _ = store
                .log_execution(
                    &task_id,
                    "skipped",
                    None,
                    None,
                    Some("task disabled before dispatch".to_string()),
                )
                .await;
            return;
        }
        _ => {
            // Task was deleted.
            let _ = store
                .delete_calendar_dispatch(&task_id, &event_id, &event_start)
                .await;
            return;
        }
    };

    let (calendar_id, minutes_before) = match &task.schedule {
        Schedule::Calendar {
            calendar_id,
            minutes_before,
            ..
        } => (calendar_id.clone(), *minutes_before),
        _ => {
            // Schedule was edited to a non-calendar type between reservation and fire.
            let _ = store
                .delete_calendar_dispatch(&task_id, &event_id, &event_start)
                .await;
            return;
        }
    };

    // 2. Re-fetch the event instance (recurrence-safe via fetch_event_instance).
    let fresh = match bridge.fetch_event_instance(
        calendar_id.clone(),
        event_id.clone(),
        event_start.clone(),
    ) {
        Ok(Some(e)) if e.calendar_id == calendar_id => e,
        Ok(_) => {
            // Event cancelled, deleted, or moved to a different calendar.
            let _ = store
                .mark_calendar_dispatch_dispatched(&task_id, &event_id, &event_start)
                .await;
            let _ = store
                .log_execution(
                    &task.id,
                    "skipped",
                    None,
                    None,
                    Some("event no longer exists".to_string()),
                )
                .await;
            return;
        }
        Err(e) => {
            eprintln!("[takt] fetch_event_instance error: {}", e);
            let _ = store
                .delete_calendar_dispatch(&task_id, &event_id, &event_start)
                .await;
            return;
        }
    };

    // 3. Branch on the fresh delta.
    let fresh_start = match DateTime::parse_from_rfc3339(&fresh.start) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            let _ = store
                .mark_calendar_dispatch_dispatched(&task_id, &event_id, &event_start)
                .await;
            return;
        }
    };
    let new_trigger_at = fresh_start - Duration::minutes(minutes_before as i64);
    let now = Utc::now();
    let d = (new_trigger_at - now).num_seconds();

    if d > CALENDAR_POLL_INTERVAL_SECS as i64 {
        // Pushed beyond next poll; drop reservation and let the next tick re-reserve.
        let _ = store
            .delete_calendar_dispatch(&task_id, &event_id, &event_start)
            .await;
        return;
    }

    if d > DISPATCH_TOLERANCE_SECS {
        // Small shift forward — sleep the remainder then re-run dispatch with the latest event.
        tokio::time::sleep(std::time::Duration::from_secs(d as u64)).await;
        Box::pin(run_dispatch_pub(
            store, executor, bridge, task_id, event_id, event_start,
        ))
        .await;
        return;
    }

    if d < -DISPATCH_TOLERANCE_SECS && !task.run_if_missed {
        // New trigger is in the past and the task does not want catch-up.
        let _ = store
            .mark_calendar_dispatch_dispatched(&task_id, &event_id, &event_start)
            .await;
        let _ = store
            .log_execution(
                &task.id,
                "skipped",
                None,
                None,
                Some("event start moved into the past".to_string()),
            )
            .await;
        return;
    }

    // 4. Mark as dispatched BEFORE executing so concurrent poll ticks see us.
    let _ = store
        .mark_calendar_dispatch_dispatched(&task_id, &event_id, &event_start)
        .await;

    // 5. Delegate to the shared execution pipeline. This is the only path
    //    that calls the executor, writes execution_logs, updates last_run_at
    //    and next_run_at, and fires notify-on-run — matching Cron/OneShot/
    //    DailyFirstUse semantics exactly. The Option<&CalendarEvent> argument
    //    (added in Task 3) carries the fresh event so the executor can use it
    //    for OpenEventLinks and template variable substitution, and so the
    //    "Triggered by event: ..." traceability line gets prepended to stdout
    //    inside execute_and_log.
    let _ = crate::scheduler::execute_and_log(
        &*executor,
        &store,
        &*bridge,
        &task.id,
        &task.name,
        task.notify_on_run,
        &task.action,
        &task.schedule,
        Some(&fresh),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_token_key_format() {
        let key = dispatch_token_key("task-1", "event-abc", "2026-04-11T10:00:00Z");
        assert_eq!(key, "calendar:task-1:event-abc:2026-04-11T10:00:00Z");
    }

    #[test]
    fn is_calendar_key_for_matches() {
        let key = dispatch_token_key("task-1", "event-abc", "2026-04-11T10:00:00Z");
        assert!(is_calendar_key_for(&key, "task-1"));
        assert!(!is_calendar_key_for(&key, "task-2"));
    }

    #[test]
    fn is_calendar_key_for_rejects_non_calendar_key() {
        assert!(!is_calendar_key_for("task-1", "task-1"));
        assert!(!is_calendar_key_for("oneshot:task-1:something", "task-1"));
    }

    #[test]
    fn is_calendar_key_for_no_prefix_collision() {
        // "task-1" should not match "task-10"
        let key = dispatch_token_key("task-10", "event-x", "2026-04-11T10:00:00Z");
        assert!(!is_calendar_key_for(&key, "task-1"));
    }
}
