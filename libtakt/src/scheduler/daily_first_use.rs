use super::{execute_and_log, ran_today};
use crate::executor::ActionExecutor;
use crate::models::{Action, Schedule, UserActivitySnapshot};
use crate::platform::PlatformBridge;
use crate::store::TaskStore;
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const CLOCK_DISCONTINUITY_TOLERANCE: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum Phase {
    WaitingForEligible,
    WaitingForInput { baseline_input_event_count: u64 },
    Counting { started_at: Instant },
}

#[derive(Debug)]
struct DailyFirstUseTracker {
    phase: Phase,
    armed_date: NaiveDate,
    eligibility_generation: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
enum TrackerOutcome {
    Continue,
    Ready,
}

#[derive(Clone, Copy, Debug)]
struct ClockSample {
    wall: DateTime<Utc>,
    monotonic: Instant,
}

fn clock_discontinuity(previous: ClockSample, current: ClockSample) -> bool {
    let wall_elapsed = current.wall.signed_duration_since(previous.wall);
    if wall_elapsed < chrono::Duration::zero() {
        return true;
    }

    let wall_elapsed = wall_elapsed
        .to_std()
        .expect("non-negative chrono duration must convert to std duration");
    let monotonic_elapsed = current
        .monotonic
        .saturating_duration_since(previous.monotonic);

    wall_elapsed.abs_diff(monotonic_elapsed) > CLOCK_DISCONTINUITY_TOLERANCE
}

#[derive(Debug, PartialEq, Eq)]
enum DateWaitOutcome {
    DateChanged,
    Cancelled,
}

async fn wait_for_date_change_or_cancel<N, S, SF>(
    cancel: &CancellationToken,
    current_date: NaiveDate,
    mut local_date: N,
    mut sleep_one_poll: S,
) -> DateWaitOutcome
where
    N: FnMut() -> NaiveDate,
    S: FnMut() -> SF,
    SF: Future<Output = ()>,
{
    loop {
        if cancel.is_cancelled() {
            return DateWaitOutcome::Cancelled;
        }
        if local_date() != current_date {
            return DateWaitOutcome::DateChanged;
        }

        tokio::select! {
            _ = cancel.cancelled() => return DateWaitOutcome::Cancelled,
            _ = sleep_one_poll() => {}
        }
    }
}

async fn wait_until_next_local_day_or_cancel(
    cancel: &CancellationToken,
    current_date: NaiveDate,
    poll_interval: Duration,
    local_date: &(dyn Fn() -> NaiveDate + Send + Sync),
) -> DateWaitOutcome {
    wait_for_date_change_or_cancel(cancel, current_date, local_date, || {
        tokio::time::sleep(poll_interval)
    })
    .await
}

impl DailyFirstUseTracker {
    fn new(armed_date: NaiveDate) -> Self {
        Self {
            phase: Phase::WaitingForEligible,
            armed_date,
            eligibility_generation: None,
        }
    }

    fn transition(
        &mut self,
        snapshot: &UserActivitySnapshot,
        local_date: NaiveDate,
        now: Instant,
        configured_delay: Duration,
        clock_discontinuity: bool,
    ) -> TrackerOutcome {
        let date_changed = local_date != self.armed_date;
        if date_changed {
            self.armed_date = local_date;
        }

        if !snapshot.session_active {
            self.phase = Phase::WaitingForEligible;
            self.eligibility_generation = Some(snapshot.eligibility_generation);
            return TrackerOutcome::Continue;
        }

        let eligibility_generation_changed = self
            .eligibility_generation
            .is_some_and(|generation| generation != snapshot.eligibility_generation);

        if clock_discontinuity && !eligibility_generation_changed {
            self.phase = Phase::WaitingForInput {
                baseline_input_event_count: snapshot.input_event_count,
            };
            self.eligibility_generation = Some(snapshot.eligibility_generation);
            return TrackerOutcome::Continue;
        }

        if date_changed
            || eligibility_generation_changed
            || self.eligibility_generation.is_none()
            || matches!(self.phase, Phase::WaitingForEligible)
        {
            self.phase = Phase::WaitingForInput {
                baseline_input_event_count: snapshot.eligibility_input_event_count,
            };
            self.eligibility_generation = Some(snapshot.eligibility_generation);
        }

        if let Phase::Counting { started_at } = self.phase {
            return if now.saturating_duration_since(started_at) >= configured_delay {
                TrackerOutcome::Ready
            } else {
                TrackerOutcome::Continue
            };
        }

        if let Phase::WaitingForInput {
            baseline_input_event_count,
        } = self.phase
        {
            if snapshot.input_event_count != baseline_input_event_count
                && last_input_is_on_local_date(snapshot.last_input_at_unix_millis, local_date)
            {
                self.phase = Phase::Counting { started_at: now };
                if configured_delay.is_zero() {
                    return TrackerOutcome::Ready;
                }
            }
        }

        TrackerOutcome::Continue
    }
}

fn last_input_is_on_local_date(last_input_at_unix_millis: Option<i64>, date: NaiveDate) -> bool {
    last_input_at_unix_millis
        .and_then(|timestamp| Local.timestamp_millis_opt(timestamp).single())
        .is_some_and(|last_input| last_input.date_naive() == date)
}

pub(super) fn last_run_is_on_local_date(last_run_at: Option<&str>, date: NaiveDate) -> bool {
    last_run_at
        .and_then(|last_run| DateTime::parse_from_rfc3339(last_run).ok())
        .is_some_and(|last_run| last_run.with_timezone(&Local).date_naive() == date)
}

fn task_is_executable_on_local_date(
    task: Option<&crate::models::TaskDto>,
    ready_date: NaiveDate,
    current_date: NaiveDate,
) -> bool {
    matches!(
        task,
        Some(task)
            if task.enabled
                && current_date == ready_date
                && !last_run_is_on_local_date(task.last_run_at.as_deref(), ready_date)
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    cancel: CancellationToken,
    required_delay: Duration,
    executor: Arc<dyn ActionExecutor>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    task_name: String,
    notify: bool,
    action: Action,
    schedule: Schedule,
) {
    run_with_date_source(
        cancel,
        required_delay,
        POLL_INTERVAL,
        POLL_INTERVAL,
        executor,
        store,
        bridge,
        task_id,
        task_name,
        notify,
        action,
        schedule,
        Arc::new(|| Local::now().date_naive()),
    )
    .await;
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn run_with_poll_interval(
    cancel: CancellationToken,
    required_delay: Duration,
    poll_interval: Duration,
    executor: Arc<dyn ActionExecutor>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    task_name: String,
    notify: bool,
    action: Action,
    schedule: Schedule,
) {
    run_with_date_source(
        cancel,
        required_delay,
        poll_interval,
        POLL_INTERVAL,
        executor,
        store,
        bridge,
        task_id,
        task_name,
        notify,
        action,
        schedule,
        Arc::new(|| Local::now().date_naive()),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn run_with_date_source(
    cancel: CancellationToken,
    required_delay: Duration,
    poll_interval: Duration,
    date_wait_poll_interval: Duration,
    executor: Arc<dyn ActionExecutor>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    task_name: String,
    notify: bool,
    action: Action,
    schedule: Schedule,
    local_date: Arc<dyn Fn() -> NaiveDate + Send + Sync>,
) {
    let mut snapshot_immediately = false;

    loop {
        let current_date = local_date();
        if ran_today(&store, &task_id, current_date).await {
            if wait_until_next_local_day_or_cancel(
                &cancel,
                current_date,
                date_wait_poll_interval,
                &*local_date,
            )
            .await
                == DateWaitOutcome::Cancelled
            {
                return;
            }
            snapshot_immediately = true;
        }

        let current_date = local_date();
        let mut tracker = DailyFirstUseTracker::new(current_date);
        let mut previous_clock = ClockSample {
            wall: Utc::now(),
            monotonic: Instant::now(),
        };

        let ready_date = loop {
            if snapshot_immediately {
                snapshot_immediately = false;
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(poll_interval) => {}
                }
            }

            if cancel.is_cancelled() {
                return;
            }
            let current_clock = ClockSample {
                wall: Utc::now(),
                monotonic: Instant::now(),
            };
            let current_date = local_date();
            let activity = bridge.get_user_activity_snapshot();
            let discontinuity = clock_discontinuity(previous_clock, current_clock);
            previous_clock = current_clock;

            if tracker.transition(
                &activity,
                current_date,
                current_clock.monotonic,
                required_delay,
                discontinuity,
            ) == TrackerOutcome::Ready
            {
                break current_date;
            }
        };

        if cancel.is_cancelled() {
            return;
        }
        let task = match store.get_task(&task_id).await {
            Ok(task) => task,
            Err(_) => return,
        };
        let execution_date = local_date();
        if task_is_executable_on_local_date(task.as_ref(), ready_date, execution_date) {
            if cancel.is_cancelled() {
                return;
            }
            let _ = execute_and_log(
                &*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule,
                None,
            )
            .await;
        } else if task.as_ref().is_some_and(|task| !task.enabled) {
            let _ = store
                .log_execution(&task_id, "skipped", None, None, None)
                .await;
        } else if task.is_none() {
            return;
        }

        if wait_until_next_local_day_or_cancel(
            &cancel,
            ready_date,
            date_wait_poll_interval,
            &*local_date,
        )
        .await
            == DateWaitOutcome::Cancelled
        {
            return;
        }
        snapshot_immediately = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        session_active: bool,
        eligibility_generation: u64,
        input_event_count: u64,
    ) -> UserActivitySnapshot {
        UserActivitySnapshot {
            session_active,
            eligibility_generation,
            input_event_count,
            eligibility_input_event_count: 0,
            last_input_at_unix_millis: (input_event_count > 0)
                .then(|| Utc::now().timestamp_millis()),
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 22).unwrap()
    }

    fn counting_tracker(now: Instant, delay: Duration) -> DailyFirstUseTracker {
        let mut tracker = DailyFirstUseTracker::new(date());
        tracker.transition(
            &native_snapshot(true, 1, 10, 10, None),
            date(),
            now,
            delay,
            false,
        );
        tracker.transition(
            &native_snapshot(
                true,
                1,
                11,
                10,
                Some(local_day_start_unix_millis(date()) + 1_000),
            ),
            date(),
            now + Duration::from_secs(10),
            delay,
            false,
        );
        tracker
    }

    fn task(enabled: bool, last_run_at: Option<String>) -> crate::models::TaskDto {
        crate::models::TaskDto {
            id: "task-id".to_string(),
            name: "Daily task".to_string(),
            description: None,
            enabled,
            run_if_missed: false,
            notify_on_run: false,
            schedule: crate::models::Schedule::DailyFirstUse { delay_minutes: 5 },
            action: crate::models::Action::Settings {
                pane_url: "test".to_string(),
            },
            created_at: "2026-07-01T00:00:00Z".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
            last_run_at,
            next_run_at: None,
            health: crate::models::TaskHealth::Healthy,
        }
    }

    struct ScriptedBridge {
        snapshots: std::sync::Mutex<std::collections::VecDeque<UserActivitySnapshot>>,
        snapshot_calls: std::sync::atomic::AtomicUsize,
        cancel_on_snapshot: Option<(usize, CancellationToken)>,
    }

    impl ScriptedBridge {
        fn new(snapshots: Vec<UserActivitySnapshot>) -> Self {
            Self {
                snapshots: std::sync::Mutex::new(snapshots.into()),
                snapshot_calls: std::sync::atomic::AtomicUsize::new(0),
                cancel_on_snapshot: None,
            }
        }

        fn cancelling_on_snapshot(
            snapshots: Vec<UserActivitySnapshot>,
            call: usize,
            cancel: CancellationToken,
        ) -> Self {
            Self {
                snapshots: std::sync::Mutex::new(snapshots.into()),
                snapshot_calls: std::sync::atomic::AtomicUsize::new(0),
                cancel_on_snapshot: Some((call, cancel)),
            }
        }
    }

    impl crate::platform::PlatformBridge for ScriptedBridge {
        fn send_notification(&self, _: String, _: String, _: bool) {}

        fn run_on_main_sync(&self, _: u64) {}

        fn get_user_activity_snapshot(&self) -> UserActivitySnapshot {
            let call = self
                .snapshot_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            if let Some((cancel_on_call, cancel)) = &self.cancel_on_snapshot {
                if call == *cancel_on_call {
                    cancel.cancel();
                }
            }
            self.snapshots
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| snapshot(true, 1, 1))
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
            Ok(Vec::new())
        }

        fn fetch_events_in_window(
            &self,
            _: String,
            _: u32,
            _: u32,
        ) -> Result<Vec<crate::models::CalendarEvent>, crate::error::TaktError> {
            Ok(Vec::new())
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

    #[derive(Default)]
    struct SpyExecutor {
        calls: std::sync::atomic::AtomicUsize,
        executed: tokio::sync::Notify,
    }

    #[derive(Default)]
    struct FailingExecutor {
        calls: std::sync::atomic::AtomicUsize,
        executed: tokio::sync::Notify,
    }

    #[async_trait::async_trait]
    impl crate::executor::ActionExecutor for SpyExecutor {
        async fn execute(
            &self,
            _: &crate::models::Action,
            _: Option<&crate::models::CalendarEvent>,
        ) -> Result<crate::executor::ExecutionResult, crate::executor::ExecutorError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.executed.notify_one();
            Ok(crate::executor::ExecutionResult {
                stdout: None,
                stderr: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl crate::executor::ActionExecutor for FailingExecutor {
        async fn execute(
            &self,
            _: &crate::models::Action,
            _: Option<&crate::models::CalendarEvent>,
        ) -> Result<crate::executor::ExecutionResult, crate::executor::ExecutorError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.executed.notify_one();
            Err(crate::executor::ExecutorError::CommandFailed(
                "expected failure".to_string(),
            ))
        }
    }

    async fn stored_daily_task(
        bridge: std::sync::Arc<dyn crate::platform::PlatformBridge>,
    ) -> (
        std::sync::Arc<crate::store::TaskStore>,
        crate::models::TaskDto,
    ) {
        let pool = crate::db::connect_in_memory().await.unwrap();
        let store = std::sync::Arc::new(crate::store::TaskStore::new(pool, bridge));
        let task = store
            .create_task(
                "Daily task".to_string(),
                None,
                false,
                false,
                crate::models::Schedule::DailyFirstUse { delay_minutes: 0 },
                crate::models::Action::Settings {
                    pane_url: "test".to_string(),
                },
            )
            .await
            .unwrap();
        (store, task)
    }

    #[test]
    fn last_run_on_the_same_local_date_is_detected() {
        let last_run = "2026-07-22T12:00:00Z";
        let local_date = DateTime::parse_from_rfc3339(last_run)
            .unwrap()
            .with_timezone(&Local)
            .date_naive();

        assert!(last_run_is_on_local_date(Some(last_run), local_date));
    }

    #[test]
    fn last_run_on_the_previous_local_date_is_not_today() {
        let last_run = "2026-07-22T12:00:00Z";
        let local_date = DateTime::parse_from_rfc3339(last_run)
            .unwrap()
            .with_timezone(&Local)
            .date_naive();

        assert!(!last_run_is_on_local_date(
            Some(last_run),
            local_date.succ_opt().unwrap()
        ));
    }

    #[test]
    fn final_guard_rejects_removed_or_disabled_task() {
        let disabled = task(false, None);

        assert!(!task_is_executable_on_local_date(None, date(), date()));
        assert!(!task_is_executable_on_local_date(
            Some(&disabled),
            date(),
            date()
        ));
    }

    #[test]
    fn final_guard_rejects_date_change_between_readiness_and_execution() {
        let task = task(true, None);

        assert!(!task_is_executable_on_local_date(
            Some(&task),
            date(),
            date().succ_opt().unwrap()
        ));
    }

    #[test]
    fn final_guard_rejects_task_that_already_ran_on_ready_date() {
        let last_run = "2026-07-22T12:00:00Z";
        let ready_date = DateTime::parse_from_rfc3339(last_run)
            .unwrap()
            .with_timezone(&Local)
            .date_naive();
        let task = task(true, Some(last_run.to_string()));

        assert!(!task_is_executable_on_local_date(
            Some(&task),
            ready_date,
            ready_date
        ));
    }

    #[test]
    fn final_guard_allows_enabled_task_not_run_on_ready_date() {
        let task = task(true, None);

        assert!(task_is_executable_on_local_date(
            Some(&task),
            date(),
            date()
        ));
    }

    #[tokio::test]
    async fn runner_takes_one_immediate_snapshot_after_date_wait_poll() {
        let cancel = CancellationToken::new();
        let bridge = std::sync::Arc::new(ScriptedBridge::cancelling_on_snapshot(
            vec![snapshot(true, 1, 0)],
            1,
            cancel.clone(),
        ));
        let bridge_trait: std::sync::Arc<dyn crate::platform::PlatformBridge> = bridge.clone();
        let (store, task) = stored_daily_task(std::sync::Arc::clone(&bridge_trait)).await;
        store.update_last_run(&task.id, None).await.unwrap();
        let executor: std::sync::Arc<dyn crate::executor::ActionExecutor> =
            std::sync::Arc::new(SpyExecutor::default());
        let today = Local::now().date_naive();
        let tomorrow = today.succ_opt().unwrap();
        let date_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let date_calls_in_source = std::sync::Arc::clone(&date_calls);
        let local_date: std::sync::Arc<dyn Fn() -> NaiveDate + Send + Sync> =
            std::sync::Arc::new(move || {
                let call = date_calls_in_source.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if call < 2 {
                    today
                } else {
                    tomorrow
                }
            });

        tokio::time::timeout(
            Duration::from_millis(100),
            run_with_date_source(
                cancel,
                Duration::ZERO,
                Duration::from_secs(60),
                Duration::ZERO,
                executor,
                store,
                bridge_trait,
                task.id,
                task.name,
                task.notify_on_run,
                task.action,
                task.schedule,
                local_date,
            ),
        )
        .await
        .expect("snapshot after date waiter must not wait for the activity poll");

        assert_eq!(
            bridge
                .snapshot_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert!(date_calls.load(std::sync::atomic::Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn runner_executes_once_after_qualifying_input() {
        let bridge = std::sync::Arc::new(ScriptedBridge::new(vec![
            snapshot(true, 1, 0),
            snapshot(true, 1, 1),
        ]));
        let bridge_trait: std::sync::Arc<dyn crate::platform::PlatformBridge> = bridge;
        let (store, task) = stored_daily_task(std::sync::Arc::clone(&bridge_trait)).await;
        let executor = std::sync::Arc::new(SpyExecutor::default());
        let executor_trait: std::sync::Arc<dyn crate::executor::ActionExecutor> = executor.clone();
        let cancel = CancellationToken::new();
        let run_cancel = cancel.clone();

        let runner = tokio::spawn(run_with_poll_interval(
            run_cancel,
            Duration::ZERO,
            Duration::ZERO,
            executor_trait,
            store,
            bridge_trait,
            task.id,
            task.name,
            task.notify_on_run,
            task.action,
            task.schedule,
        ));

        tokio::time::timeout(Duration::from_millis(100), executor.executed.notified())
            .await
            .expect("runner must execute after baseline and qualifying input");
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(executor.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        cancel.cancel();
        runner.await.unwrap();
    }

    #[tokio::test]
    async fn failed_execution_is_logged_and_not_retried_on_the_same_day() {
        let bridge = std::sync::Arc::new(ScriptedBridge::new(vec![
            snapshot(true, 1, 0),
            snapshot(true, 1, 1),
        ]));
        let bridge_trait: std::sync::Arc<dyn crate::platform::PlatformBridge> = bridge.clone();
        let (store, task) = stored_daily_task(std::sync::Arc::clone(&bridge_trait)).await;
        let executor = std::sync::Arc::new(FailingExecutor::default());
        let executor_trait: std::sync::Arc<dyn crate::executor::ActionExecutor> = executor.clone();
        let cancel = CancellationToken::new();
        let run_cancel = cancel.clone();

        let runner = tokio::spawn(run_with_poll_interval(
            run_cancel,
            Duration::ZERO,
            Duration::ZERO,
            executor_trait,
            std::sync::Arc::clone(&store),
            bridge_trait,
            task.id.clone(),
            task.name,
            task.notify_on_run,
            task.action,
            task.schedule,
        ));

        tokio::time::timeout(Duration::from_millis(100), executor.executed.notified())
            .await
            .expect("runner must attempt the failing execution");
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(executor.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            bridge
                .snapshot_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            2,
            "each poll must obtain exactly one native snapshot"
        );
        let logs = store.list_logs(Some(&task.id), 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "failure");
        assert_eq!(
            logs[0].error.as_deref(),
            Some("Command failed: expected failure")
        );
        assert!(store
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap()
            .last_run_at
            .is_some());

        cancel.cancel();
        runner.await.unwrap();
    }

    #[tokio::test]
    async fn runner_cancels_while_poll_sleep_is_pending() {
        let bridge = std::sync::Arc::new(ScriptedBridge::new(Vec::new()));
        let bridge_trait: std::sync::Arc<dyn crate::platform::PlatformBridge> = bridge;
        let (store, task) = stored_daily_task(std::sync::Arc::clone(&bridge_trait)).await;
        let executor: std::sync::Arc<dyn crate::executor::ActionExecutor> =
            std::sync::Arc::new(SpyExecutor::default());
        let cancel = CancellationToken::new();
        let run_cancel = cancel.clone();

        let runner = tokio::spawn(run(
            run_cancel,
            Duration::ZERO,
            executor,
            store,
            bridge_trait,
            task.id,
            task.name,
            task.notify_on_run,
            task.action,
            task.schedule,
        ));
        tokio::task::yield_now().await;
        cancel.cancel();

        tokio::time::timeout(Duration::from_millis(100), runner)
            .await
            .expect("runner cancellation must interrupt the pending poll")
            .expect("runner task must complete");
    }

    #[tokio::test]
    async fn runner_does_not_execute_when_cancelled_at_readiness() {
        let cancel = CancellationToken::new();
        let bridge = std::sync::Arc::new(ScriptedBridge::cancelling_on_snapshot(
            vec![snapshot(true, 1, 0), snapshot(true, 1, 1)],
            2,
            cancel.clone(),
        ));
        let bridge_trait: std::sync::Arc<dyn crate::platform::PlatformBridge> = bridge;
        let (store, task) = stored_daily_task(std::sync::Arc::clone(&bridge_trait)).await;
        let executor = std::sync::Arc::new(SpyExecutor::default());
        let executor_trait: std::sync::Arc<dyn crate::executor::ActionExecutor> = executor.clone();

        let runner = tokio::spawn(run_with_poll_interval(
            cancel,
            Duration::ZERO,
            Duration::ZERO,
            executor_trait,
            store,
            bridge_trait,
            task.id,
            task.name,
            task.notify_on_run,
            task.action,
            task.schedule,
        ));

        tokio::time::timeout(Duration::from_millis(100), runner)
            .await
            .expect("runner must stop after readiness cancellation")
            .expect("runner task must complete");
        assert_eq!(executor.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn wall_monotonic_gap_detects_suspend() {
        let monotonic = Instant::now();
        let previous = ClockSample {
            wall: chrono::DateTime::from_timestamp(1_000, 0).unwrap(),
            monotonic,
        };
        let current = ClockSample {
            wall: chrono::DateTime::from_timestamp(1_015, 0).unwrap(),
            monotonic: monotonic + Duration::from_secs(10),
        };

        assert!(clock_discontinuity(previous, current));
    }

    #[test]
    fn equally_delayed_clocks_do_not_treat_app_nap_as_suspend() {
        let monotonic = Instant::now();
        let previous = ClockSample {
            wall: chrono::DateTime::from_timestamp(1_000, 0).unwrap(),
            monotonic,
        };
        let current = ClockSample {
            wall: chrono::DateTime::from_timestamp(1_060, 0).unwrap(),
            monotonic: monotonic + Duration::from_secs(60),
        };

        assert!(!clock_discontinuity(previous, current));
    }

    #[test]
    fn backward_wall_clock_change_breaks_continuity() {
        let monotonic = Instant::now();
        let previous = ClockSample {
            wall: chrono::DateTime::from_timestamp(1_000, 0).unwrap(),
            monotonic,
        };
        let current = ClockSample {
            wall: chrono::DateTime::from_timestamp(999, 0).unwrap(),
            monotonic: monotonic + Duration::from_secs(10),
        };

        assert!(clock_discontinuity(previous, current));
    }

    #[tokio::test]
    async fn date_wait_exits_after_one_short_poll_when_date_changes() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let current_date = date();
        let next_date = current_date.succ_opt().unwrap();
        let poll_count = std::cell::Cell::new(0);

        let outcome = wait_for_date_change_or_cancel(
            &cancel,
            current_date,
            || {
                if poll_count.get() == 0 {
                    current_date
                } else {
                    next_date
                }
            },
            || {
                poll_count.set(poll_count.get() + 1);
                std::future::ready(())
            },
        )
        .await;

        assert_eq!(outcome, DateWaitOutcome::DateChanged);
        assert_eq!(poll_count.get(), 1);
    }

    #[tokio::test]
    async fn date_wait_returns_promptly_when_cancelled() {
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();

        let outcome = tokio::time::timeout(
            Duration::from_millis(100),
            wait_for_date_change_or_cancel(&cancel, date(), date, std::future::pending::<()>),
        )
        .await
        .expect("cancelled date wait must not await the pending poll");

        assert_eq!(outcome, DateWaitOutcome::Cancelled);
    }

    #[tokio::test]
    async fn date_wait_cancels_while_poll_sleep_is_pending() {
        let cancel = CancellationToken::new();
        let wait_cancel = cancel.clone();
        let poll_started = std::sync::Arc::new(tokio::sync::Notify::new());
        let poll_started_in_waiter = std::sync::Arc::clone(&poll_started);

        let waiter = tokio::spawn(async move {
            wait_for_date_change_or_cancel(&wait_cancel, date(), date, move || {
                let poll_started = std::sync::Arc::clone(&poll_started_in_waiter);
                async move {
                    poll_started.notify_one();
                    std::future::pending::<()>().await;
                }
            })
            .await
        });

        tokio::time::timeout(Duration::from_millis(100), poll_started.notified())
            .await
            .expect("poll sleep must become pending");
        cancel.cancel();

        let outcome = tokio::time::timeout(Duration::from_millis(100), waiter)
            .await
            .expect("cancellation must interrupt the pending poll")
            .expect("date waiter task must complete");
        assert_eq!(outcome, DateWaitOutcome::Cancelled);
    }

    fn local_day_start_unix_millis(local_date: NaiveDate) -> i64 {
        local_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .single()
            .unwrap()
            .timestamp_millis()
    }

    fn native_snapshot(
        session_active: bool,
        eligibility_generation: u64,
        input_event_count: u64,
        eligibility_input_event_count: u64,
        last_input_at_unix_millis: Option<i64>,
    ) -> UserActivitySnapshot {
        UserActivitySnapshot {
            session_active,
            eligibility_generation,
            input_event_count,
            eligibility_input_event_count,
            last_input_at_unix_millis,
        }
    }

    #[test]
    fn input_after_eligibility_before_first_rust_snapshot_starts_counting() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        let outcome = tracker.transition(
            &native_snapshot(
                true,
                1,
                11,
                10,
                Some(local_day_start_unix_millis(date()) + 1_000),
            ),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(tracker.phase, Phase::Counting { started_at } if started_at == now));
    }

    #[test]
    fn input_after_midnight_before_first_poll_starts_counting() {
        let now = Instant::now();
        let next_date = date().succ_opt().unwrap();
        let mut tracker = DailyFirstUseTracker::new(next_date);

        tracker.transition(
            &native_snapshot(
                true,
                1,
                21,
                20,
                Some(local_day_start_unix_millis(next_date) + 1_000),
            ),
            next_date,
            now,
            Duration::from_secs(30),
            false,
        );

        assert!(matches!(tracker.phase, Phase::Counting { started_at } if started_at == now));
    }

    #[test]
    fn input_before_midnight_does_not_start_on_new_day() {
        let now = Instant::now();
        let next_date = date().succ_opt().unwrap();
        let mut tracker = DailyFirstUseTracker::new(next_date);

        tracker.transition(
            &native_snapshot(
                true,
                1,
                21,
                20,
                Some(local_day_start_unix_millis(next_date) - 1),
            ),
            next_date,
            now,
            Duration::from_secs(30),
            false,
        );

        assert!(matches!(tracker.phase, Phase::WaitingForInput { .. }));
    }

    #[test]
    fn first_eligible_snapshot_only_establishes_baseline() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        let outcome = tracker.transition(
            &native_snapshot(true, 1, 10, 10, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(
            tracker.phase,
            Phase::WaitingForInput {
                baseline_input_event_count: 10
            }
        ));
    }

    #[test]
    fn unlock_input_does_not_start_counting() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        tracker.transition(
            &native_snapshot(false, 1, 9, 9, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );
        let outcome = tracker.transition(
            &native_snapshot(
                true,
                2,
                10,
                10,
                Some(local_day_start_unix_millis(date()) + 1_000),
            ),
            date(),
            now + Duration::from_secs(10),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(tracker.phase, Phase::WaitingForInput { .. }));
    }

    #[test]
    fn input_after_eligibility_baseline_starts_counting() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        tracker.transition(
            &native_snapshot(true, 1, 10, 10, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );
        let outcome = tracker.transition(
            &native_snapshot(
                true,
                1,
                11,
                10,
                Some(local_day_start_unix_millis(date()) + 1_000),
            ),
            date(),
            now + Duration::from_secs(10),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(
            matches!(tracker.phase, Phase::Counting { started_at } if started_at == now + Duration::from_secs(10))
        );
    }

    #[test]
    fn unchanged_native_input_counter_does_not_start_counting() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());
        tracker.transition(
            &native_snapshot(true, 1, 10, 10, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );

        tracker.transition(
            &native_snapshot(
                true,
                1,
                10,
                10,
                Some(local_day_start_unix_millis(date()) + 1_000),
            ),
            date(),
            now + Duration::from_secs(10),
            Duration::from_secs(30),
            false,
        );

        assert!(matches!(tracker.phase, Phase::WaitingForInput { .. }));
    }

    #[test]
    fn missing_native_input_timestamp_fails_closed() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        tracker.transition(
            &native_snapshot(true, 1, 11, 10, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );

        assert!(matches!(tracker.phase, Phase::WaitingForInput { .. }));
    }

    #[test]
    fn counting_does_not_require_additional_input() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        let outcome = tracker.transition(
            &native_snapshot(true, 1, 11, 10, None),
            date(),
            now + Duration::from_secs(40),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Ready);
    }

    #[test]
    fn counting_requires_the_full_configured_delay() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        assert_eq!(
            tracker.transition(
                &native_snapshot(true, 1, 11, 10, None),
                date(),
                now + Duration::from_secs(39),
                Duration::from_secs(30),
                false,
            ),
            TrackerOutcome::Continue
        );
        assert_eq!(
            tracker.transition(
                &native_snapshot(true, 1, 11, 10, None),
                date(),
                now + Duration::from_secs(40),
                Duration::from_secs(30),
                false,
            ),
            TrackerOutcome::Ready
        );
    }

    #[test]
    fn zero_delay_still_requires_a_qualifying_input() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());

        assert_eq!(
            tracker.transition(
                &native_snapshot(true, 1, 10, 10, None),
                date(),
                now,
                Duration::ZERO,
                false,
            ),
            TrackerOutcome::Continue
        );
        assert_eq!(
            tracker.transition(
                &native_snapshot(
                    true,
                    1,
                    11,
                    10,
                    Some(local_day_start_unix_millis(date()) + 1_000),
                ),
                date(),
                now + Duration::from_secs(1),
                Duration::ZERO,
                false,
            ),
            TrackerOutcome::Ready
        );
    }

    #[test]
    fn current_ineligibility_resets_counting() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        let outcome = tracker.transition(
            &native_snapshot(false, 2, 12, 12, None),
            date(),
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(tracker.phase, Phase::WaitingForEligible));
    }

    #[test]
    fn eligibility_generation_change_resets_counting_even_when_currently_active() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        let outcome = tracker.transition(
            &native_snapshot(true, 2, 20, 20, None),
            date(),
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(
            tracker.phase,
            Phase::WaitingForInput {
                baseline_input_event_count: 20
            }
        ));
        assert_eq!(tracker.eligibility_generation, Some(2));
    }

    #[test]
    fn input_after_regain_before_next_snapshot_starts_counting() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        tracker.transition(
            &native_snapshot(
                true,
                2,
                21,
                20,
                Some(local_day_start_unix_millis(date()) + 2_000),
            ),
            date(),
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            false,
        );

        assert!(
            matches!(tracker.phase, Phase::Counting { started_at } if started_at == now + Duration::from_secs(20))
        );
    }

    #[test]
    fn resume_requires_new_input_and_full_delay() {
        let now = Instant::now();
        let delay = Duration::from_secs(30);
        let mut tracker = counting_tracker(now, delay);
        tracker.transition(
            &native_snapshot(false, 2, 20, 20, None),
            date(),
            now + Duration::from_secs(15),
            delay,
            false,
        );

        assert_eq!(
            tracker.transition(
                &native_snapshot(
                    true,
                    2,
                    20,
                    20,
                    Some(local_day_start_unix_millis(date()) + 2_000),
                ),
                date(),
                now + Duration::from_secs(20),
                delay,
                false,
            ),
            TrackerOutcome::Continue
        );
        assert!(matches!(tracker.phase, Phase::WaitingForInput { .. }));

        tracker.transition(
            &native_snapshot(
                true,
                2,
                21,
                20,
                Some(local_day_start_unix_millis(date()) + 3_000),
            ),
            date(),
            now + Duration::from_secs(30),
            delay,
            false,
        );
        assert_eq!(
            tracker.transition(
                &native_snapshot(true, 2, 21, 20, None),
                date(),
                now + Duration::from_secs(59),
                delay,
                false,
            ),
            TrackerOutcome::Continue
        );
        assert_eq!(
            tracker.transition(
                &native_snapshot(true, 2, 21, 20, None),
                date(),
                now + Duration::from_secs(60),
                delay,
                false,
            ),
            TrackerOutcome::Ready
        );
    }

    #[test]
    fn local_date_change_resets_counting_and_accepts_post_midnight_input() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));
        let next_date = date().succ_opt().unwrap();

        let outcome = tracker.transition(
            &native_snapshot(
                true,
                1,
                12,
                10,
                Some(local_day_start_unix_millis(next_date) + 1_000),
            ),
            next_date,
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            false,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert_eq!(tracker.armed_date, next_date);
        assert!(
            matches!(tracker.phase, Phase::Counting { started_at } if started_at == now + Duration::from_secs(20))
        );
    }

    #[test]
    fn generation_change_during_clock_discontinuity_preserves_post_wake_input() {
        let now = Instant::now();
        let mut tracker = DailyFirstUseTracker::new(date());
        tracker.transition(
            &native_snapshot(true, 1, 10, 10, None),
            date(),
            now,
            Duration::from_secs(30),
            false,
        );

        let outcome = tracker.transition(
            &native_snapshot(
                true,
                2,
                21,
                20,
                Some(local_day_start_unix_millis(date()) + 2_000),
            ),
            date(),
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            true,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(
            matches!(tracker.phase, Phase::Counting { started_at } if started_at == now + Duration::from_secs(20))
        );
    }

    #[test]
    fn clock_discontinuity_without_generation_change_discards_current_input_observation() {
        let now = Instant::now();
        let mut tracker = counting_tracker(now, Duration::from_secs(30));

        let outcome = tracker.transition(
            &native_snapshot(
                true,
                1,
                12,
                10,
                Some(local_day_start_unix_millis(date()) + 2_000),
            ),
            date(),
            now + Duration::from_secs(20),
            Duration::from_secs(30),
            true,
        );

        assert_eq!(outcome, TrackerOutcome::Continue);
        assert!(matches!(
            tracker.phase,
            Phase::WaitingForInput {
                baseline_input_event_count: 12
            }
        ));

        tracker.transition(
            &native_snapshot(
                true,
                1,
                13,
                10,
                Some(local_day_start_unix_millis(date()) + 3_000),
            ),
            date(),
            now + Duration::from_secs(30),
            Duration::from_secs(30),
            false,
        );
        assert!(matches!(tracker.phase, Phase::Counting { .. }));
    }
}
