//! Calendar trigger poller and dispatch routine.
//!
//! See docs/superpowers/specs/2026-04-10-calendar-trigger-design.md §5.

// Imports used by the poller tick logic added in Task 8.
#[allow(unused_imports)]
use crate::executor::ActionExecutor;
#[allow(unused_imports)]
use crate::models::{Action, CalendarEvent, Schedule, TaskDto};
#[allow(unused_imports)]
use crate::platform::PlatformBridge;
#[allow(unused_imports)]
use crate::store::TaskStore;
#[allow(unused_imports)]
use chrono::{DateTime, Utc};
#[allow(unused_imports)]
use std::collections::HashMap;
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tokio::sync::Mutex;
#[allow(unused_imports)]
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
