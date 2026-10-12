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
/// Longest gap since the last complete poll that is still caught up after
/// sleep. Bounds the EventKit query after long downtime. Dispatch markers are
/// retained for twice this long.
pub const CALENDAR_MAX_CATCH_UP_SECS: i64 = 24 * 60 * 60;

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
    /// Start time of the last poll that fetched every calendar. Events that
    /// started after it (minus the lookback margin) are new slots, so a poll
    /// after system sleep still catches up on the gap.
    last_complete_tick: Mutex<Option<DateTime<Utc>>>,
    /// Watermark used until the first complete poll, so failed startup polls
    /// do not move the cutoff forward.
    created_at: DateTime<Utc>,
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
            last_complete_tick: Mutex::new(None),
            created_at: Utc::now(),
        }
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
            tokio::time::sleep(std::time::Duration::from_secs(CALENDAR_POLL_INTERVAL_SECS)).await;
            if let Err(e) = self.tick().await {
                eprintln!("[takt] calendar poller tick error: {}", e);
            }
        }
    }

    async fn tick(&self) -> anyhow::Result<()> {
        let tick_started = Utc::now();
        let tasks = self.store.list_tasks().await?;
        let calendar_tasks: Vec<TaskDto> = tasks
            .into_iter()
            .filter(|t| t.enabled && matches!(t.schedule, Schedule::Calendar { .. }))
            .collect();
        if calendar_tasks.is_empty() {
            *self.last_complete_tick.lock().await = Some(tick_started);
            let _ = self.store.prune_old_calendar_dispatches().await;
            return Ok(());
        }

        // EventKit returns every event overlapping the window, including ones
        // that started long ago (in-progress, all-day, multi-day). Only events
        // starting after the last complete poll are new slots.
        let since = self
            .last_complete_tick
            .lock()
            .await
            .unwrap_or(self.created_at)
            .min(tick_started);
        let new_slot_cutoff = (since
            - Duration::seconds(
                (CALENDAR_POLL_INTERVAL_SECS + CALENDAR_LOOKBACK_MARGIN_SECS) as i64,
            ))
        .max(tick_started - Duration::seconds(CALENDAR_MAX_CATCH_UP_SECS));
        // Only a tick that fetched and reserved everything may advance the
        // watermark, otherwise a failed slot would never be retried.
        let mut tick_complete = true;

        // Group by calendar_id to minimize bridge calls.
        let mut by_calendar: HashMap<String, Vec<TaskDto>> = HashMap::new();
        for t in calendar_tasks {
            if let Schedule::Calendar { calendar_id, .. } = &t.schedule {
                by_calendar
                    .entry(calendar_id.clone())
                    .or_default()
                    .push(t.clone());
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
            let lookback_minutes = ((tick_started - new_slot_cutoff).num_seconds() + 59) / 60;

            let events = match self.bridge.fetch_events_in_window(
                calendar_id.clone(),
                lookback_minutes as u32,
                lookahead_minutes as u32,
            ) {
                Ok(list) => list,
                Err(e) => {
                    eprintln!("[takt] fetch_events_in_window({}): {}", calendar_id, e);
                    tick_complete = false;
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
                    if let Err(error) = self
                        .process_slot(task, event, minutes_before, new_slot_cutoff)
                        .await
                    {
                        tick_complete = false;
                        eprintln!(
                            "[takt] reserve calendar dispatch for {}: {}",
                            task.id, error
                        );
                    }
                }
            }
        }

        if tick_complete {
            *self.last_complete_tick.lock().await = Some(tick_started);
        }
        let _ = self.store.prune_old_calendar_dispatches().await;
        Ok(())
    }

    async fn process_slot(
        &self,
        task: &TaskDto,
        event: &CalendarEvent,
        minutes_before: u32,
        new_slot_cutoff: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let event_start = match DateTime::parse_from_rfc3339(&event.start) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return Ok(()),
        };
        let trigger_at = event_start - Duration::minutes(minutes_before as i64);
        let now = Utc::now();
        let d = (trigger_at - now).num_seconds();

        if event_start < new_slot_cutoff {
            return Ok(());
        }

        // Skip if already known.
        if self
            .store
            .dispatch_exists(&task.id, &event.id, &event.start)
            .await?
        {
            return Ok(());
        }

        // Skip if the trigger is past the next poll window — next tick will handle it.
        if d > CALENDAR_POLL_INTERVAL_SECS as i64 {
            return Ok(());
        }

        let trigger_at_iso = trigger_at.to_rfc3339();

        if d < -DISPATCH_TOLERANCE_SECS {
            // Missed slot.
            if task.run_if_missed {
                // Reserve as scheduled, then dispatch immediately.
                if !self
                    .store
                    .insert_calendar_dispatch(
                        &task.id,
                        &event.id,
                        &event.start,
                        "scheduled",
                        &trigger_at_iso,
                    )
                    .await?
                {
                    return Ok(());
                }
                self.spawn_dispatch(task.id.clone(), event.id.clone(), event.start.clone(), 0)
                    .await;
            } else {
                // Consume the slot without running.
                if !self
                    .store
                    .insert_calendar_dispatch(
                        &task.id,
                        &event.id,
                        &event.start,
                        "dispatched",
                        &trigger_at_iso,
                    )
                    .await?
                {
                    return Ok(());
                }
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
            return Ok(());
        }

        // Reserve and schedule a sleep until trigger_at (possibly zero seconds).
        if !self
            .store
            .insert_calendar_dispatch(
                &task.id,
                &event.id,
                &event.start,
                "scheduled",
                &trigger_at_iso,
            )
            .await?
        {
            return Ok(());
        }
        let delay_secs = d.max(0) as u64;
        self.spawn_dispatch(
            task.id.clone(),
            event.id.clone(),
            event.start.clone(),
            delay_secs,
        )
        .await;
        Ok(())
    }

    pub(crate) async fn spawn_dispatch(
        &self,
        task_id: String,
        event_id: String,
        event_start: String,
        delay_secs: u64,
    ) {
        let key = dispatch_token_key(&task_id, &event_id, &event_start);
        let token = CancellationToken::new();
        {
            let mut tokens = self.cancel_tokens.lock().await;
            if tokens.contains_key(&key) {
                return;
            }
            tokens.insert(key.clone(), token.clone());
        }

        let store = Arc::clone(&self.store);
        let executor = Arc::clone(&self.executor);
        let bridge = Arc::clone(&self.bridge);
        let cancel_tokens = Arc::clone(&self.cancel_tokens);

        tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = token.cancelled() => return,
                _ = tokio::time::sleep(std::time::Duration::from_secs(delay_secs)) => {}
            }
            run_dispatch(
                store,
                executor,
                bridge,
                task_id,
                event_id,
                event_start,
                token.clone(),
            )
            .await;
            let mut tokens = cancel_tokens.lock().await;
            // Removal cancels the old token before allowing a replacement.
            if !token.is_cancelled() {
                tokens.remove(&key);
            }
        });
    }
}

// ── Fire-time dispatch ────────────────────────────────────────────────

async fn claim_dispatch(
    store: &TaskStore,
    task_id: &str,
    event_id: &str,
    event_start: &str,
) -> bool {
    match store
        .mark_calendar_dispatch_dispatched(task_id, event_id, event_start)
        .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            eprintln!("[takt] claim calendar dispatch for {}: {}", task_id, error);
            false
        }
    }
}

/// Claims a rescheduled event under both its reserved and its new start, so
/// the next poll does not dispatch the same occurrence again. On a storage
/// error nothing is claimed and the reservation stays retryable.
async fn claim_moved_dispatch(
    store: &TaskStore,
    task_id: &str,
    event_id: &str,
    reserved_start: &str,
    moved_start: &str,
    trigger_at: &DateTime<Utc>,
) -> bool {
    match store
        .claim_moved_calendar_dispatch(
            task_id,
            event_id,
            reserved_start,
            moved_start,
            &trigger_at.to_rfc3339(),
        )
        .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            eprintln!(
                "[takt] claim moved calendar dispatch for {}: {}",
                task_id, error
            );
            false
        }
    }
}

pub async fn run_dispatch_pub(
    store: Arc<TaskStore>,
    executor: Arc<dyn ActionExecutor>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    event_id: String,
    event_start: String,
) {
    run_dispatch(
        store,
        executor,
        bridge,
        task_id,
        event_id,
        event_start,
        CancellationToken::new(),
    )
    .await;
}

async fn run_dispatch(
    store: Arc<TaskStore>,
    executor: Arc<dyn ActionExecutor>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    event_id: String,
    event_start: String,
    cancellation: CancellationToken,
) {
    if cancellation.is_cancelled() {
        return;
    }
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

    let (calendar_id, minutes_before, title_contains) = match &task.schedule {
        Schedule::Calendar {
            calendar_id,
            minutes_before,
            title_contains,
        } => (calendar_id.clone(), *minutes_before, title_contains),
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
            if !claim_dispatch(&store, &task_id, &event_id, &event_start).await {
                return;
            }
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

    if title_contains
        .as_ref()
        .is_some_and(|needle| !fresh.title.to_lowercase().contains(&needle.to_lowercase()))
    {
        if claim_dispatch(&store, &task_id, &event_id, &event_start).await {
            let _ = store
                .log_execution(
                    &task_id,
                    "skipped",
                    None,
                    None,
                    Some("event title no longer matches".into()),
                )
                .await;
        }
        return;
    }

    // 3. Branch on the fresh delta.
    let fresh_start = match DateTime::parse_from_rfc3339(&fresh.start) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            claim_dispatch(&store, &task_id, &event_id, &event_start).await;
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
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(d as u64)) => {}
        }
        Box::pin(run_dispatch(
            store,
            executor,
            bridge,
            task_id,
            event_id,
            event_start,
            cancellation,
        ))
        .await;
        return;
    }

    if d < -DISPATCH_TOLERANCE_SECS && !task.run_if_missed {
        // New trigger is in the past and the task does not want catch-up.
        if !claim_dispatch(&store, &task_id, &event_id, &event_start).await {
            return;
        }
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
    if cancellation.is_cancelled() {
        return;
    }
    let claimed = if fresh.start == event_start {
        claim_dispatch(&store, &task_id, &event_id, &event_start).await
    } else {
        claim_moved_dispatch(
            &store,
            &task_id,
            &event_id,
            &event_start,
            &fresh.start,
            &new_trigger_at,
        )
        .await
    };
    if !claimed {
        return;
    }

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
#[path = "calendar_startup_tests.rs"]
mod startup_tests;

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
