use crate::executor::{current_executor, ActionExecutor};
use crate::models::{Schedule, TaskDto};
use crate::store::TaskStore;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::sync::Arc;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

fn normalize_cron(expr: &str) -> String {
    let parts: Vec<&str> = expr.trim().split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", expr.trim())
    } else {
        expr.trim().to_string()
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
        None => return true, // never ran — consider missed
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
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
    app_handle: tauri::AppHandle,
    job_ids: Mutex<HashMap<String, Uuid>>,
}

impl AppScheduler {
    pub async fn new(store: Arc<TaskStore>, app_handle: tauri::AppHandle) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        let executor = Arc::new(current_executor(app_handle.clone()));
        Ok(Self {
            inner,
            executor,
            store,
            app_handle,
            job_ids: Mutex::new(HashMap::new()),
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
                self.schedule_task(task).await?;
            }
        }
        // Run catch-up for missed tasks after scheduling
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
                let app_handle = self.app_handle.clone();
                let action = task.action.clone();
                tokio::spawn(async move {
                    let result = executor.execute(&action).await;
                    let (status, stdout, stderr, error) = match result {
                        Ok(r) => ("success", r.stdout, r.stderr, None),
                        Err(e) => ("failure", None, None, Some(e.to_string())),
                    };
                    if notify && status == "success" {
                        send_run_notification(&app_handle, &task_name);
                    }
                    let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                    let _ = store.update_last_run(&task_id, None).await;
                });
            }
        }
    }

    pub async fn schedule_task(&self, task: &TaskDto) -> anyhow::Result<()> {
        let task_id = task.id.clone();
        let task_name = task.name.clone();
        let notify = task.notify_on_run;
        let action = task.action.clone();
        let executor = Arc::clone(&self.executor);
        let store = Arc::clone(&self.store);
        let app_handle = self.app_handle.clone();

        match &task.schedule {
            Schedule::Cron { expression } => {
                let expr = normalize_cron(expression);
                let store_guard = Arc::clone(&store);
                let task_id_for_closure = task_id.clone();
                let task_id_guard = task_id.clone();
                let task_name = task_name.clone();
                let app_handle = app_handle.clone();

                let job = Job::new_async_tz(expr.as_str(), Local, move |_uuid, _lock| {
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id_for_closure.clone();
                    let store_guard = Arc::clone(&store_guard);
                    let task_id_guard = task_id_guard.clone();
                    let task_name = task_name.clone();
                    let app_handle = app_handle.clone();
                    Box::pin(async move {
                        // Guard: re-fetch task to check enabled/deleted
                        match store_guard.get_task(&task_id_guard).await {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                let _ = store.log_execution(&task_id, "skipped", None, None, None).await;
                                return;
                            }
                            _ => return, // deleted or error — silently skip
                        }

                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        if notify && status == "success" {
                            send_run_notification(&app_handle, &task_name);
                        }
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    })
                })?;

                let job_uuid = self.inner.add(job).await?;
                self.job_ids.lock().await.insert(task_id, job_uuid);
            }
            Schedule::OneShot { run_at } => {
                let run_at: chrono::DateTime<Local> = run_at.parse::<chrono::DateTime<chrono::FixedOffset>>()?.with_timezone(&Local);
                let now = Local::now();
                if run_at > now {
                    let delay = (run_at - now).to_std()?;
                    let store_guard = Arc::clone(&store);
                    let task_id_guard = task_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        match store_guard.get_task(&task_id_guard).await {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                let _ = store.log_execution(&task_id, "skipped", None, None, None).await;
                                return;
                            }
                            _ => return,
                        }
                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        if notify && status == "success" {
                            send_run_notification(&app_handle, &task_name);
                        }
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    });
                }
            }
            Schedule::DailyFirstUse { delay_minutes } => {
                // Executes once per day after N minutes of continuous active use.
                // If the Mac sleeps/locks before the threshold, the timer resets.
                // Uses wall-clock gap detection: if a 30s tick takes much longer,
                // the system was asleep — reset accumulated time.
                let required_secs: u64 = delay_minutes * 60;
                let store_guard = Arc::clone(&store);
                let task_id_guard = task_id.clone();
                tokio::spawn(async move {
                    // Check if already executed today
                    if ran_today(&store_guard, &task_id_guard).await {
                        return;
                    }

                    // Accumulate active use in 30s ticks
                    let required_secs = required_secs;
                    const TICK_SECS: u64 = 30;
                    // If a tick takes more than 3x expected, system was likely asleep
                    const SLEEP_THRESHOLD_SECS: u64 = TICK_SECS * 3;

                    let mut accumulated_secs: u64 = 0;
                    loop {
                        let before = std::time::Instant::now();
                        tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
                        let elapsed = before.elapsed().as_secs();

                        if elapsed > SLEEP_THRESHOLD_SECS {
                            // System was asleep — reset accumulated time
                            accumulated_secs = 0;
                            // Re-check if already ran today (date might have changed)
                            if ran_today(&store_guard, &task_id_guard).await {
                                return;
                            }
                            continue;
                        }

                        accumulated_secs += TICK_SECS;
                        if accumulated_secs >= required_secs {
                            break;
                        }
                    }

                    // Final checks: enabled, not deleted, not yet ran today
                    match store_guard.get_task(&task_id_guard).await {
                        Ok(Some(t)) if t.enabled => {
                            if ran_today(&store_guard, &task_id_guard).await {
                                return;
                            }
                        }
                        Ok(Some(_)) => {
                            let _ = store.log_execution(&task_id, "skipped", None, None, None).await;
                            return;
                        }
                        _ => return,
                    }

                    let result = executor.execute(&action).await;
                    let (status, stdout, stderr, error) = match result {
                        Ok(r) => ("success", r.stdout, r.stderr, None),
                        Err(e) => ("failure", None, None, Some(e.to_string())),
                    };
                    if notify && status == "success" {
                        send_run_notification(&app_handle, &task_name);
                    }
                    let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                    let _ = store.update_last_run(&task_id, None).await;
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
        Ok(())
    }
}

fn send_run_notification(app: &tauri::AppHandle, task_name: &str) {
    let _ = app
        .notification()
        .builder()
        .title(task_name)
        .body("Task executed successfully")
        .show();
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
