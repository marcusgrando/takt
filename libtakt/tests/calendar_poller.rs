//! Integration tests for the CalendarPoller dispatch routine.
//!
//! These tests construct a MockPlatformBridge that returns scripted
//! calendar data, an in-memory SQLite TaskStore, and drive the poller's
//! run_dispatch_pub directly (bypassing tokio timing) to exercise every
//! branch of the fire-time rule.

use libtakt::calendar;
use libtakt::executor::{ActionExecutor, ExecutionResult, ExecutorError};
use libtakt::models::*;
use libtakt::platform::PlatformBridge;
use libtakt::store::TaskStore;
use std::sync::{Arc, Mutex};

// ── MockBridge ────────────────────────────────────────────────────────

struct MockBridge {
    access_status: Mutex<CalendarAccessStatus>,
    calendars: Mutex<Vec<CalendarInfo>>,
    events_in_window: Mutex<Vec<CalendarEvent>>,
    event_instance: Mutex<Option<CalendarEvent>>,
}

impl MockBridge {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            access_status: Mutex::new(CalendarAccessStatus::Authorized),
            calendars: Mutex::new(vec![]),
            events_in_window: Mutex::new(vec![]),
            event_instance: Mutex::new(None),
        })
    }

    fn with_event_instance(self: Arc<Self>, event: CalendarEvent) -> Arc<Self> {
        *self.event_instance.lock().unwrap() = Some(event);
        self
    }

    fn with_no_event_instance(self: Arc<Self>) -> Arc<Self> {
        *self.event_instance.lock().unwrap() = None;
        self
    }
}

impl PlatformBridge for MockBridge {
    fn send_notification(&self, _: String, _: String, _: bool) {}
    fn run_on_main_sync(&self, _: u64) {}
    fn is_user_active(&self, _: u64) -> bool {
        true
    }
    fn get_calendar_access_status(
        &self,
    ) -> Result<CalendarAccessStatus, libtakt::error::TaktError> {
        Ok(self.access_status.lock().unwrap().clone())
    }
    fn request_calendar_access(
        &self,
    ) -> Result<CalendarAccessStatus, libtakt::error::TaktError> {
        Ok(CalendarAccessStatus::Authorized)
    }
    fn list_calendars(
        &self,
    ) -> Result<Vec<CalendarInfo>, libtakt::error::TaktError> {
        Ok(self.calendars.lock().unwrap().clone())
    }
    fn fetch_events_in_window(
        &self,
        _: String,
        _: u32,
        _: u32,
    ) -> Result<Vec<CalendarEvent>, libtakt::error::TaktError> {
        Ok(self.events_in_window.lock().unwrap().clone())
    }
    fn fetch_event_instance(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> Result<Option<CalendarEvent>, libtakt::error::TaktError> {
        Ok(self.event_instance.lock().unwrap().clone())
    }
}

// ── SpyExecutor ───────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct SpyExecutor {
    calls: Arc<Mutex<Vec<(Action, Option<CalendarEvent>)>>>,
}

#[async_trait::async_trait]
impl ActionExecutor for SpyExecutor {
    async fn execute(
        &self,
        action: &Action,
        event: Option<&CalendarEvent>,
    ) -> Result<ExecutionResult, ExecutorError> {
        self.calls
            .lock()
            .unwrap()
            .push((action.clone(), event.cloned()));
        Ok(ExecutionResult {
            stdout: None,
            stderr: None,
        })
    }
}

// ── Fixtures ──────────────────────────────────────────────────────────

async fn make_store(bridge: Arc<dyn PlatformBridge>) -> Arc<TaskStore> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrate");
    Arc::new(TaskStore::new(pool, bridge))
}

fn sample_event(id: &str, calendar_id: &str, start_iso: &str, title: &str) -> CalendarEvent {
    CalendarEvent {
        id: id.into(),
        title: title.into(),
        start: start_iso.into(),
        end: start_iso.into(),
        notes: None,
        location: None,
        url: None,
        conference_url: Some("https://meet.test/abc".into()),
        calendar_id: calendar_id.into(),
    }
}

/// Create a Calendar-scheduled task in the store. Returns the auto-generated task ID.
async fn create_calendar_task(
    store: &Arc<TaskStore>,
    calendar_id: &str,
    run_if_missed: bool,
) -> String {
    let task = store
        .create_task(
            "test-task".into(),
            None,
            run_if_missed,
            false,
            Schedule::Calendar {
                calendar_id: calendar_id.into(),
                title_contains: None,
                minutes_before: 0,
            },
            Action::Settings {
                pane_url: "x".into(),
            },
        )
        .await
        .expect("create_task");
    task.id
}

// ── Tests ─────────────────────────────────────────────────────────────

/// Event is live and matches calendar — dispatch should execute the action and
/// pass the CalendarEvent to the executor.
#[tokio::test]
async fn dispatch_uses_fresh_event_data() {
    // Use a timestamp 1 second in the past so the dispatch fires immediately
    // (within DISPATCH_TOLERANCE_SECS = 2, so it's treated as "on time").
    let now_ish = chrono::Utc::now() - chrono::Duration::seconds(1);
    let start_iso = now_ish.to_rfc3339();

    let fresh = sample_event("evt-1", "cal-1", &start_iso, "Standup — Updated Title");
    let bridge = MockBridge::new().with_event_instance(fresh.clone());
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    // run_if_missed=true to cover the "tolerable past delta" branch too.
    let task_id = create_calendar_task(&store, "cal-1", true).await;

    store
        .insert_calendar_dispatch(&task_id, "evt-1", &start_iso, "scheduled", &start_iso)
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        start_iso.clone(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    // execute_and_log calls the executor; expect exactly one call with the event attached.
    assert_eq!(calls.len(), 1, "expected one executor call");
    assert!(
        calls[0].1.is_some(),
        "executor call should carry the CalendarEvent"
    );
    assert_eq!(calls[0].1.as_ref().unwrap().id, "evt-1");
}

/// Bridge returns None (event cancelled/deleted) → dispatch is skipped.
#[tokio::test]
async fn dispatch_skips_when_fresh_event_cancelled() {
    let bridge = MockBridge::new().with_no_event_instance();
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    let task_id = create_calendar_task(&store, "cal-1", false).await;

    store
        .insert_calendar_dispatch(
            &task_id,
            "evt-1",
            "2026-04-12T09:00:00+00:00",
            "scheduled",
            "2026-04-12T09:00:00+00:00",
        )
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        "2026-04-12T09:00:00+00:00".into(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert!(calls.is_empty(), "executor must not be called for cancelled event");

    // The row should have been marked dispatched (not remain scheduled).
    let exists = store
        .dispatch_exists(&task_id, "evt-1", "2026-04-12T09:00:00+00:00")
        .await
        .unwrap();
    assert!(exists, "dispatch row should still exist (marked dispatched)");

    // Confirm a 'skipped' execution log was written.
    let logs = store.list_logs(Some(&task_id), 10).await.unwrap();
    assert!(
        logs.iter().any(|l| l.status == "skipped"),
        "expected a skipped log entry"
    );
}

/// Bridge returns event with a different calendar_id (moved calendars) → skipped.
#[tokio::test]
async fn dispatch_skips_when_calendar_mismatch() {
    // Task is watching "cal-1" but the fresh event now belongs to "cal-other".
    let mismatched = sample_event("evt-1", "cal-other", "2026-04-12T09:00:00+00:00", "Meeting");
    let bridge = MockBridge::new().with_event_instance(mismatched);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    let task_id = create_calendar_task(&store, "cal-1", false).await;

    store
        .insert_calendar_dispatch(
            &task_id,
            "evt-1",
            "2026-04-12T09:00:00+00:00",
            "scheduled",
            "2026-04-12T09:00:00+00:00",
        )
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        "2026-04-12T09:00:00+00:00".into(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert!(
        calls.is_empty(),
        "executor must not fire when calendar_id mismatches"
    );

    let logs = store.list_logs(Some(&task_id), 10).await.unwrap();
    assert!(
        logs.iter().any(|l| l.status == "skipped"),
        "expected a skipped log entry for calendar mismatch"
    );
}

/// Event start is within DISPATCH_TOLERANCE_SECS of now — the dispatch executes.
#[tokio::test]
async fn dispatch_runs_when_event_on_time() {
    // Use a timestamp 1 second in the past (within DISPATCH_TOLERANCE_SECS = 2).
    let now_ish = chrono::Utc::now() - chrono::Duration::seconds(1);
    let start_iso = now_ish.to_rfc3339();
    let fresh = sample_event("evt-1", "cal-1", &start_iso, "On-time event");
    let bridge = MockBridge::new().with_event_instance(fresh);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    let task_id = create_calendar_task(&store, "cal-1", true).await;

    store
        .insert_calendar_dispatch(&task_id, "evt-1", &start_iso, "scheduled", &start_iso)
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        start_iso.clone(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "executor should fire for on-time event");
}

/// Event start moved into the past and run_if_missed = true → executes.
#[tokio::test]
async fn dispatch_runs_immediately_when_moved_into_past_and_run_if_missed_true() {
    let past = chrono::Utc::now() - chrono::Duration::minutes(30);
    let start_iso = past.to_rfc3339();
    let fresh = sample_event("evt-1", "cal-1", &start_iso, "Missed meeting");
    let bridge = MockBridge::new().with_event_instance(fresh);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    // run_if_missed = true so it should run despite being in the past.
    let task_id = create_calendar_task(&store, "cal-1", true).await;

    store
        .insert_calendar_dispatch(&task_id, "evt-1", &start_iso, "scheduled", &start_iso)
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        start_iso.clone(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "executor should fire for missed event when run_if_missed=true"
    );
}

/// Event start moved into the past and run_if_missed = false → skips.
#[tokio::test]
async fn dispatch_skips_when_moved_into_past_and_run_if_missed_false() {
    let past = chrono::Utc::now() - chrono::Duration::minutes(30);
    let start_iso = past.to_rfc3339();
    let fresh = sample_event("evt-1", "cal-1", &start_iso, "Missed meeting");
    let bridge = MockBridge::new().with_event_instance(fresh);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    // run_if_missed = false → must NOT run.
    let task_id = create_calendar_task(&store, "cal-1", false).await;

    store
        .insert_calendar_dispatch(&task_id, "evt-1", &start_iso, "scheduled", &start_iso)
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        start_iso.clone(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert!(
        calls.is_empty(),
        "executor must not fire when run_if_missed=false and event is in the past"
    );

    let logs = store.list_logs(Some(&task_id), 10).await.unwrap();
    assert!(
        logs.iter().any(|l| l.status == "skipped"),
        "expected a skipped log"
    );
}

/// Event pushed well beyond the next poll window → dispatch row is deleted so
/// the next tick can re-reserve it at the correct time.
#[tokio::test]
async fn dispatch_drops_reservation_when_moved_beyond_poll_window() {
    // Event start 20 minutes in the future (> CALENDAR_POLL_INTERVAL_SECS = 300s).
    let future = chrono::Utc::now() + chrono::Duration::minutes(20);
    let start_iso = future.to_rfc3339();
    let fresh = sample_event("evt-1", "cal-1", &start_iso, "Rescheduled far out");
    let bridge = MockBridge::new().with_event_instance(fresh);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    let task_id = create_calendar_task(&store, "cal-1", false).await;

    // Reserve with an old (stale) trigger time so the row exists.
    let old_trigger = (chrono::Utc::now() + chrono::Duration::seconds(10)).to_rfc3339();
    store
        .insert_calendar_dispatch(
            &task_id,
            "evt-1",
            &old_trigger, // event_start key (the value that was reserved)
            "scheduled",
            &old_trigger,
        )
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        task_id.clone(),
        "evt-1".into(),
        old_trigger.clone(), // same event_start passed to run_dispatch_pub
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert!(
        calls.is_empty(),
        "executor must not fire when event moved beyond poll window"
    );

    // The stale row should have been deleted.
    let exists = store
        .dispatch_exists(&task_id, "evt-1", &old_trigger)
        .await
        .unwrap();
    assert!(
        !exists,
        "stale dispatch row should be deleted when event moves beyond poll window"
    );
}

/// Calling run_dispatch_pub for a non-existent task → no executor call, dispatch row cleaned up.
#[tokio::test]
async fn dispatch_is_no_op_when_task_not_found() {
    let fresh = sample_event("evt-1", "cal-1", "2026-04-12T09:00:00+00:00", "Ghost");
    let bridge = MockBridge::new().with_event_instance(fresh);
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;
    // Deliberately do NOT insert any task.

    store
        .insert_calendar_dispatch(
            "task-ghost",
            "evt-1",
            "2026-04-12T09:00:00+00:00",
            "scheduled",
            "2026-04-12T09:00:00+00:00",
        )
        .await
        .unwrap();

    let executor = Arc::new(SpyExecutor::default());
    calendar::run_dispatch_pub(
        Arc::clone(&store),
        executor.clone() as Arc<dyn ActionExecutor>,
        Arc::clone(&bridge),
        "task-ghost".into(),
        "evt-1".into(),
        "2026-04-12T09:00:00+00:00".into(),
    )
    .await;

    let calls = executor.calls.lock().unwrap();
    assert!(
        calls.is_empty(),
        "executor must not be called when task is missing"
    );
}

/// Inserting the same (task, event, start) twice should be idempotent (INSERT OR IGNORE).
#[tokio::test]
async fn insert_calendar_dispatch_is_idempotent() {
    let bridge = MockBridge::new();
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;

    let start = "2026-04-12T09:00:00+00:00";
    let trigger = "2026-04-12T08:55:00+00:00";

    store
        .insert_calendar_dispatch("task-1", "evt-1", start, "scheduled", trigger)
        .await
        .unwrap();
    // Second insert should be silently ignored (INSERT OR IGNORE).
    store
        .insert_calendar_dispatch("task-1", "evt-1", start, "dispatched", trigger)
        .await
        .unwrap();

    // Row should still exist.
    let exists = store
        .dispatch_exists("task-1", "evt-1", start)
        .await
        .unwrap();
    assert!(exists, "row should exist after idempotent inserts");
}

/// dispatch_exists returns true after insert and false after delete.
#[tokio::test]
async fn dispatch_exists_round_trip() {
    let bridge = MockBridge::new();
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;

    let start = "2026-04-13T10:00:00+00:00";
    let trigger = "2026-04-13T09:55:00+00:00";

    assert!(
        !store.dispatch_exists("t", "e", start).await.unwrap(),
        "should not exist before insert"
    );

    store
        .insert_calendar_dispatch("t", "e", start, "scheduled", trigger)
        .await
        .unwrap();
    assert!(
        store.dispatch_exists("t", "e", start).await.unwrap(),
        "should exist after insert"
    );

    store
        .delete_calendar_dispatch("t", "e", start)
        .await
        .unwrap();
    assert!(
        !store.dispatch_exists("t", "e", start).await.unwrap(),
        "should not exist after delete"
    );
}

/// list_pending_dispatches returns only 'scheduled' rows.
#[tokio::test]
async fn list_pending_dispatches_returns_scheduled_only() {
    let bridge = MockBridge::new();
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;

    store
        .insert_calendar_dispatch(
            "t1",
            "e1",
            "2026-04-14T10:00:00+00:00",
            "scheduled",
            "2026-04-14T09:55:00+00:00",
        )
        .await
        .unwrap();
    store
        .insert_calendar_dispatch(
            "t2",
            "e2",
            "2026-04-14T11:00:00+00:00",
            "dispatched",
            "2026-04-14T10:55:00+00:00",
        )
        .await
        .unwrap();

    let pending = store.list_pending_dispatches().await.unwrap();
    assert_eq!(pending.len(), 1, "only scheduled rows should be returned");
    assert_eq!(pending[0].task_id, "t1");
}

/// mark_calendar_dispatch_dispatched flips a 'scheduled' row to 'dispatched'.
#[tokio::test]
async fn mark_dispatched_changes_status() {
    let bridge = MockBridge::new();
    let bridge: Arc<dyn PlatformBridge> = bridge;
    let store = make_store(Arc::clone(&bridge)).await;

    let start = "2026-04-15T08:00:00+00:00";
    let trigger = "2026-04-15T07:55:00+00:00";

    store
        .insert_calendar_dispatch("t1", "e1", start, "scheduled", trigger)
        .await
        .unwrap();

    let pending_before = store.list_pending_dispatches().await.unwrap();
    assert_eq!(pending_before.len(), 1);

    store
        .mark_calendar_dispatch_dispatched("t1", "e1", start)
        .await
        .unwrap();

    let pending_after = store.list_pending_dispatches().await.unwrap();
    assert!(
        pending_after.is_empty(),
        "after marking dispatched, no pending rows should remain"
    );
}
