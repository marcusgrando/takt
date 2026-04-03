use crate::executor::{current_executor, ActionExecutor};
use crate::models::{Schedule, TaskDto};
use crate::store::TaskStore;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
}

impl AppScheduler {
    pub async fn new(store: Arc<TaskStore>) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        let executor = Arc::new(current_executor());
        Ok(Self {
            inner,
            executor,
            store,
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
                let expr = expression.clone();
                let job = Job::new_async(expr.as_str(), move |_uuid, _lock| {
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id.clone();
                    Box::pin(async move {
                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        let _ = store
                            .log_execution(&task_id, status, stdout, stderr, error)
                            .await;
                        let _ = store.update_last_run(&task_id, None).await;
                    })
                })?;
                self.inner.add(job).await?;
            }
            Schedule::OneShot { run_at } => {
                let run_at: chrono::DateTime<chrono::Utc> = run_at.parse()?;
                let now = chrono::Utc::now();
                if run_at > now {
                    let delay = (run_at - now).to_std()?;
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        let _ = store
                            .log_execution(&task_id, status, stdout, stderr, error)
                            .await;
                        let _ = store.update_last_run(&task_id, None).await;
                    });
                }
            }
            Schedule::OnLogin | Schedule::OnWake => {
                // Dispatched immediately on app startup
                tokio::spawn(async move {
                    let result = executor.execute(&action).await;
                    let (status, stdout, stderr, error) = match result {
                        Ok(r) => ("success", r.stdout, r.stderr, None),
                        Err(e) => ("failure", None, None, Some(e.to_string())),
                    };
                    let _ = store
                        .log_execution(&task_id, status, stdout, stderr, error)
                        .await;
                    let _ = store.update_last_run(&task_id, None).await;
                });
            }
        }
        Ok(())
    }

    pub async fn remove_task(&self, _task_id: &str) -> anyhow::Result<()> {
        // v1: restart re-loads all active tasks
        // Full removal by UUID requires storing job UUIDs — add in v2
        Ok(())
    }
}
