use crate::executor::{current_executor, ActionExecutor};
use crate::models::{Schedule, TaskDto};
use crate::store::TaskStore;
use std::collections::HashMap;
use std::sync::Arc;
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

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
    job_ids: Mutex<HashMap<String, Uuid>>,
}

impl AppScheduler {
    pub async fn new(store: Arc<TaskStore>, app_handle: tauri::AppHandle) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        let executor = Arc::new(current_executor(app_handle));
        Ok(Self {
            inner,
            executor,
            store,
            job_ids: Mutex::new(HashMap::new()),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.inner.start().await?;
        Ok(())
    }

    pub async fn load_all_tasks(&self) -> anyhow::Result<()> {
        let tasks = self.store.list_tasks().await?;
        for task in tasks {
            if task.enabled {
                self.schedule_task(&task).await?;
            }
        }
        Ok(())
    }

    pub async fn schedule_task(&self, task: &TaskDto) -> anyhow::Result<()> {
        let task_id = task.id.clone();
        let action = task.action.clone();
        let executor = Arc::clone(&self.executor);
        let store = Arc::clone(&self.store);

        match &task.schedule {
            Schedule::Cron { expression } => {
                let expr = normalize_cron(expression);
                let store_guard = Arc::clone(&store);
                let task_id_for_closure = task_id.clone();
                let task_id_guard = task_id.clone();

                let job = Job::new_async(expr.as_str(), move |_uuid, _lock| {
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id_for_closure.clone();
                    let store_guard = Arc::clone(&store_guard);
                    let task_id_guard = task_id_guard.clone();
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
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    })
                })?;

                let job_uuid = self.inner.add(job).await?;
                self.job_ids.lock().await.insert(task_id, job_uuid);
            }
            Schedule::OneShot { run_at } => {
                let run_at: chrono::DateTime<chrono::Utc> = run_at.parse()?;
                let now = chrono::Utc::now();
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
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    });
                }
            }
            Schedule::OnLogin | Schedule::OnWake => {
                tokio::spawn(async move {
                    let result = executor.execute(&action).await;
                    let (status, stdout, stderr, error) = match result {
                        Ok(r) => ("success", r.stdout, r.stderr, None),
                        Err(e) => ("failure", None, None, Some(e.to_string())),
                    };
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
