use crate::executor::ActionExecutor;
use crate::models::{Action, Schedule, TaskDto};
use crate::platform::PlatformBridge;
use crate::store::TaskStore;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(crate) fn normalize_cron(expr: &str) -> String {
    let expr = expr.trim();
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    }
}

/// Check if a cron task missed an execution while the system was asleep/off.
/// Returns true if there was a scheduled tick between `last_run_at` and now.
fn missed_cron_run(expression: &str, last_run_at: Option<&str>) -> bool {
    let last_run = match last_run_at {
        Some(s) => match DateTime::parse_from_rfc3339(s) {
            Ok(dt) => dt.with_timezone(&Local),
            Err(_) => return false,
        },
        None => return false, // never ran — not a missed execution, just new
    };

    let expr = normalize_cron(expression);
    let cron = match croner::Cron::new(&expr).parse() {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Find the next tick after last_run; if it's before now, we missed it
    if let Ok(next) = cron.find_next_occurrence(&last_run, false) {
        next < Local::now()
    } else {
        false
    }
}

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<dyn ActionExecutor>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,
    job_ids: Mutex<HashMap<String, Uuid>>,
    /// Cancellation tokens for DailyFirstUse/OneShot spawned tasks
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AppScheduler {
    pub async fn new(
        store: Arc<TaskStore>,
        executor: Arc<dyn ActionExecutor>,
        bridge: Arc<dyn PlatformBridge>,
    ) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        Ok(Self {
            inner,
            executor,
            store,
            bridge,
            job_ids: Mutex::new(HashMap::new()),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.inner.start().await?;
        Ok(())
    }

    pub async fn load_all_tasks(&self) -> anyhow::Result<()> {
        let tasks = self.store.list_tasks().await?;
        for task in &tasks {
            if task.enabled {
                if let Err(e) = self.schedule_task(task).await {
                    let err_msg = e.to_string();
                    eprintln!(
                        "Warning: failed to schedule task '{}' ({}): {}",
                        task.name, task.id, err_msg
                    );
                    // Log the error so it's visible in the history UI
                    let _ = self
                        .store
                        .log_execution(&task.id, "schedule_error", None, None, Some(err_msg))
                        .await;
                    // Disable in DB so the UI reflects that this task isn't running.
                    // User can re-enable to retry; the error will surface then.
                    if let Err(dis_err) = self
                        .store
                        .update_task(&task.id, None, None, Some(false), None, None, None, None)
                        .await
                    {
                        eprintln!(
                            "Error: could not disable task '{}' ({}): {} — will be fixed on restart",
                            task.name, task.id, dis_err
                        );
                    }
                }
            }
        }
        // Re-fetch from DB so catch-up sees tasks disabled by scheduling failures
        let tasks = self.store.list_tasks().await?;
        self.catch_up_missed(&tasks).await;
        Ok(())
    }

    /// Execute any enabled tasks that missed their scheduled run while the system was off.
    /// Only applies to Cron tasks with `run_if_missed` enabled. Runs once (latest missed).
    async fn catch_up_missed(&self, tasks: &[TaskDto]) {
        for task in tasks {
            if !task.enabled || !task.run_if_missed {
                continue;
            }
            let should_catch_up = match &task.schedule {
                Schedule::Cron { expression } => {
                    missed_cron_run(expression, task.last_run_at.as_deref())
                }
                Schedule::OneShot { run_at } => {
                    // If one-shot time has passed and never ran
                    if task.last_run_at.is_some() {
                        false
                    } else if let Ok(dt) = DateTime::parse_from_rfc3339(run_at) {
                        dt.with_timezone(&Local) < Local::now()
                    } else {
                        false
                    }
                }
                Schedule::DailyFirstUse { .. } => false, // has its own logic
            };

            if should_catch_up {
                let executor = Arc::clone(&self.executor);
                let store = Arc::clone(&self.store);
                let task_id = task.id.clone();
                let task_name = task.name.clone();
                let notify = task.notify_on_run;
                let bridge = self.bridge.clone();
                let action = task.action.clone();
                let schedule = task.schedule.clone();
                tokio::spawn(async move {
                    let _ = execute_and_log(
                        &*executor, &store, &*bridge, &task_id, &task_name, notify, &action,
                        &schedule,
                    )
                    .await;
                });
            }
        }
    }

    pub async fn schedule_task(&self, task: &TaskDto) -> anyhow::Result<()> {
        let task_id = task.id.clone();
        let task_name = task.name.clone();
        let notify = task.notify_on_run;
        let action = task.action.clone();
        let schedule = task.schedule.clone();
        let executor = Arc::clone(&self.executor);
        let store = Arc::clone(&self.store);
        let bridge = self.bridge.clone();

        match &task.schedule {
            Schedule::Cron { expression } => {
                let expr = normalize_cron(expression);
                let task_id_for_map = task_id.clone();

                let job = Job::new_async_tz(expr.as_str(), Local, move |_uuid, _lock| {
                    let action = action.clone();
                    let schedule = schedule.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id.clone();
                    let task_name = task_name.clone();
                    let bridge = bridge.clone();
                    Box::pin(async move {
                        // Guard: re-fetch task to check enabled/deleted
                        match store.get_task(&task_id).await {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                let _ = store
                                    .log_execution(&task_id, "skipped", None, None, None)
                                    .await;
                                return;
                            }
                            _ => return,
                        }
                        let _ = execute_and_log(
                            &*executor, &store, &*bridge, &task_id, &task_name, notify, &action,
                            &schedule,
                        )
                        .await;
                    })
                })?;

                let job_uuid = self.inner.add(job).await?;
                self.job_ids.lock().await.insert(task_id_for_map, job_uuid);
            }
            Schedule::OneShot { run_at } => {
                let run_at: chrono::DateTime<Local> = run_at
                    .parse::<chrono::DateTime<chrono::FixedOffset>>()?
                    .with_timezone(&Local);
                let now = Local::now();
                if run_at > now {
                    let delay = (run_at - now).to_std()?;
                    let token = CancellationToken::new();
                    let child_token = token.child_token();
                    let tokens = Arc::clone(&self.cancel_tokens);
                    tokens.lock().await.insert(task_id.clone(), token);
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = child_token.cancelled() => return,
                            _ = tokio::time::sleep(delay) => {}
                        }
                        match store.get_task(&task_id).await {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                let _ = store
                                    .log_execution(&task_id, "skipped", None, None, None)
                                    .await;
                                tokens.lock().await.remove(&task_id);
                                return;
                            }
                            _ => {
                                tokens.lock().await.remove(&task_id);
                                return;
                            }
                        }
                        let _ = execute_and_log(
                            &*executor, &store, &*bridge, &task_id, &task_name, notify, &action,
                            &schedule,
                        )
                        .await;
                        tokens.lock().await.remove(&task_id);
                    });
                }
            }
            Schedule::DailyFirstUse { delay_minutes } => {
                let required_secs: u64 = delay_minutes * 60;
                let token = CancellationToken::new();
                let child_token = token.child_token();
                self.cancel_tokens
                    .lock()
                    .await
                    .insert(task_id.clone(), token);
                tokio::spawn(async move {
                    daily_first_use_loop(
                        child_token,
                        required_secs,
                        executor,
                        store,
                        bridge,
                        task_id,
                        task_name,
                        notify,
                        action,
                        schedule,
                    )
                    .await;
                });
            }
        }
        Ok(())
    }

    pub async fn remove_task(&self, task_id: &str) -> anyhow::Result<()> {
        let mut ids = self.job_ids.lock().await;
        if let Some(job_uuid) = ids.remove(task_id) {
            self.inner.remove(&job_uuid).await?;
        }
        drop(ids);
        // Cancel any spawned background task (DailyFirstUse / OneShot)
        let mut tokens = self.cancel_tokens.lock().await;
        if let Some(token) = tokens.remove(task_id) {
            token.cancel();
        }
        Ok(())
    }
}

/// Runs the DailyFirstUse logic in a loop that re-arms each day.
/// Uses wall-clock elapsed time instead of a fixed threshold to detect sleep:
/// only the actual elapsed time beyond the expected tick counts as accumulated.
/// This avoids false sleep detection from macOS App Nap / timer throttling.
#[allow(clippy::too_many_arguments)]
async fn daily_first_use_loop(
    cancel: CancellationToken,
    required_secs: u64,
    executor: Arc<dyn ActionExecutor>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,
    task_id: String,
    task_name: String,
    notify: bool,
    action: Action,
    schedule: Schedule,
) {
    const TICK_SECS: u64 = 30;
    // If a tick takes more than 10 minutes, the system was truly asleep (not just throttled)
    const SLEEP_THRESHOLD_SECS: u64 = 600;

    loop {
        // Skip if already ran today
        if ran_today(&store, &task_id).await {
            // Wait until just past midnight, then re-check
            if wait_until_tomorrow_or_cancel(&cancel).await {
                return; // cancelled
            }
            continue;
        }

        // Accumulate active time in TICK_SECS intervals
        let mut accumulated_secs: u64 = 0;
        loop {
            let before = std::time::Instant::now();
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)) => {}
            }
            let elapsed = before.elapsed().as_secs();

            if elapsed > SLEEP_THRESHOLD_SECS {
                // System was truly asleep — reset
                accumulated_secs = 0;
                if ran_today(&store, &task_id).await {
                    break;
                }
                continue;
            }

            // Count actual elapsed time (handles minor throttling gracefully)
            accumulated_secs += elapsed.min(TICK_SECS * 2);
            if accumulated_secs >= required_secs {
                // Final guard: re-check enabled + not yet ran today
                match store.get_task(&task_id).await {
                    Ok(Some(t)) if t.enabled => {
                        if ran_today(&store, &task_id).await {
                            break;
                        }
                    }
                    Ok(Some(_)) => {
                        let _ = store
                            .log_execution(&task_id, "skipped", None, None, None)
                            .await;
                        break;
                    }
                    _ => return, // deleted
                }
                let _ = execute_and_log(
                    &*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule,
                )
                .await;
                break;
            }
        }

        // Wait until tomorrow to re-arm
        if wait_until_tomorrow_or_cancel(&cancel).await {
            return; // cancelled
        }
    }
}

/// Sleep until just past local midnight. Returns true if cancelled.
async fn wait_until_tomorrow_or_cancel(cancel: &CancellationToken) -> bool {
    let now = Local::now();
    let tomorrow = (now + chrono::Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 5)
        .unwrap();
    let tomorrow_local = tomorrow
        .and_local_timezone(Local)
        .single()
        .unwrap_or_else(|| now + chrono::Duration::hours(24));
    let duration = (tomorrow_local - now)
        .to_std()
        .unwrap_or(std::time::Duration::from_secs(3600));
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

/// Compute the next fire time for a cron expression from now.
fn next_cron_fire(expression: &str) -> Option<String> {
    let expr = normalize_cron(expression);
    let cron = croner::Cron::new(&expr).parse().ok()?;
    let next = cron.find_next_occurrence(&Local::now(), false).ok()?;
    Some(next.to_rfc3339())
}

/// Execute an action, log the result, and optionally notify.
/// Returns Ok(()) on success, Err(message) on execution failure.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_and_log(
    executor: &dyn ActionExecutor,
    store: &TaskStore,
    bridge: &dyn PlatformBridge,
    task_id: &str,
    task_name: &str,
    notify: bool,
    action: &Action,
    schedule: &Schedule,
) -> Result<(), String> {
    let result = executor.execute(action).await;
    let (status, stdout, stderr, error) = match &result {
        Ok(r) => ("success", r.stdout.clone(), r.stderr.clone(), None),
        Err(e) => ("failure", None, None, Some(e.to_string())),
    };
    if notify {
        send_run_notification(bridge, task_name, status == "success");
    }
    let _ = store
        .log_execution(task_id, status, stdout, stderr, error.clone())
        .await;
    let next_run = match schedule {
        Schedule::Cron { expression } => next_cron_fire(expression),
        _ => None,
    };
    let _ = store.update_last_run(task_id, next_run).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn send_run_notification(bridge: &dyn PlatformBridge, task_name: &str, success: bool) {
    let body = if success {
        format!("Executed: {}", task_name)
    } else {
        format!("Failed: {}", task_name)
    };
    bridge.send_notification("Takt".to_string(), body, false);
}

/// Check if a task's last_run_at is today in local timezone.
async fn ran_today(store: &TaskStore, task_id: &str) -> bool {
    match store.get_task(task_id).await {
        Ok(Some(t)) => {
            if let Some(ref last_run) = t.last_run_at {
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(last_run) {
                    let last_local = last.with_timezone(&chrono::Local);
                    return last_local.date_naive() == chrono::Local::now().date_naive();
                }
            }
            false
        }
        _ => false,
    }
}
