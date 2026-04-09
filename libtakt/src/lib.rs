uniffi::setup_scaffolding!();

mod db;
mod error;
mod executor;
mod launch_agent;
pub mod models;
pub mod platform;
mod scheduler;
mod store;

use std::sync::{Arc, OnceLock};

use error::TaktError;
use executor::{current_executor, ActionExecutor};
use models::{CreateTaskParams, ExecutionLog, TaskDto, UpdateTaskParams};
use platform::PlatformBridge;
use scheduler::AppScheduler;
use store::TaskStore;

/// Global tokio runtime shared by all UniFFI-exported async functions.
/// UniFFI polls async futures on a background thread without a tokio context,
/// so we must provide one for sqlx, tokio-cron-scheduler, and friends.
fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

/// All initialized resources bundled as a single unit.
/// Either all are present (init succeeded) or none are (init failed/pending).
struct InitializedState {
    store: Arc<TaskStore>,
    scheduler: Arc<AppScheduler>,
    executor: Arc<dyn ActionExecutor>,
}

#[derive(uniffi::Object)]
pub struct TaktCore {
    bridge: Arc<dyn PlatformBridge>,
    state: tokio::sync::OnceCell<InitializedState>,
}

/// Private helpers (not exported via UniFFI).
impl TaktCore {
    fn state(&self) -> Result<&InitializedState, TaktError> {
        self.state.get().ok_or(TaktError::NotInitialized)
    }

    fn store(&self) -> Result<&Arc<TaskStore>, TaktError> {
        Ok(&self.state()?.store)
    }

    fn scheduler(&self) -> Result<&Arc<AppScheduler>, TaktError> {
        Ok(&self.state()?.scheduler)
    }

    fn executor_ref(&self) -> Result<&Arc<dyn ActionExecutor>, TaktError> {
        Ok(&self.state()?.executor)
    }
}

#[uniffi::export]
impl TaktCore {
    #[uniffi::constructor]
    pub fn new(bridge: Arc<dyn PlatformBridge>) -> Self {
        Self {
            bridge,
            state: tokio::sync::OnceCell::new(),
        }
    }

    /// Initialize the core: connect DB, start scheduler, load tasks.
    /// Spawns async work on the global tokio runtime because UniFFI polls
    /// futures from non-tokio threads that lack the context needed by
    /// sqlx / tokio-cron-scheduler.
    pub async fn start(&self) -> Result<(), TaktError> {
        if self.state.get().is_some() {
            return Ok(());
        }
        let bridge = self.bridge.clone();
        let state = tokio_runtime()
            .spawn(async move {
                let pool = db::connect().await?;
                let store = Arc::new(TaskStore::new(pool));
                let executor = current_executor(bridge.clone());
                let scheduler = Arc::new(
                    AppScheduler::new(
                        Arc::clone(&store),
                        Arc::clone(&executor),
                        Arc::clone(&bridge),
                    )
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?,
                );
                scheduler
                    .start()
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;
                if let Err(e) = scheduler.load_all_tasks().await {
                    eprintln!("Warning: failed to load tasks: {}", e);
                }
                launch_agent::ensure_registered();
                Ok::<_, TaktError>(InitializedState {
                    store,
                    scheduler,
                    executor,
                })
            })
            .await
            .map_err(|e| TaktError::Database {
                msg: format!("runtime join error: {}", e),
            })??;
        // If another call raced us, discard our state (first writer wins).
        let _ = self.state.set(state);
        Ok(())
    }

    // -- Task CRUD -------------------------------------------------------

    pub async fn list_tasks(&self) -> Result<Vec<TaskDto>, TaktError> {
        let store = self.store()?.clone();
        tokio_runtime()
            .spawn(async move { Ok(store.list_tasks().await?) })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }

    pub async fn get_task(&self, id: String) -> Result<Option<TaskDto>, TaktError> {
        let store = self.store()?.clone();
        tokio_runtime()
            .spawn(async move { Ok(store.get_task(&id).await?) })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }

    /// Create task with rollback on scheduler failure.
    pub async fn create_task(&self, params: CreateTaskParams) -> Result<TaskDto, TaktError> {
        let store = self.store()?.clone();
        let scheduler = self.scheduler()?.clone();
        tokio_runtime()
            .spawn(async move {
                let task = store
                    .create_task(
                        params.name,
                        params.description,
                        params.run_if_missed.unwrap_or(true),
                        params.notify_on_run.unwrap_or(false),
                        params.schedule,
                        params.action,
                    )
                    .await?;

                if task.enabled {
                    if let Err(e) = scheduler.schedule_task(&task).await {
                        let mut errors = vec![e.to_string()];
                        if let Err(rb_err) = store.delete_task(&task.id).await {
                            errors.push(format!("rollback delete failed: {}", rb_err));
                            if let Err(dis_err) = store
                                .update_task(
                                    &task.id,
                                    None,
                                    None,
                                    Some(false),
                                    None,
                                    None,
                                    None,
                                    None,
                                )
                                .await
                            {
                                errors.push(format!(
                                    "disable failed: {} \u{2014} restart app to fix",
                                    dis_err
                                ));
                            }
                        }
                        return Err(TaktError::Scheduler {
                            msg: errors.join("; "),
                        });
                    }
                }
                Ok(task)
            })
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?
    }

    /// Update task with full rollback chain.
    pub async fn update_task(&self, params: UpdateTaskParams) -> Result<TaskDto, TaktError> {
        let store = self.store()?.clone();
        let scheduler = self.scheduler()?.clone();
        tokio_runtime()
            .spawn(async move {
                let old_task =
                    store
                        .get_task(&params.id)
                        .await?
                        .ok_or_else(|| TaktError::NotFound {
                            msg: "Task not found".to_string(),
                        })?;

                scheduler
                    .remove_task(&params.id)
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;

                let task = match store
                    .update_task(
                        &params.id,
                        params.name,
                        params.description,
                        params.enabled,
                        params.run_if_missed,
                        params.notify_on_run,
                        params.schedule,
                        params.action,
                    )
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        let mut errors = vec![e.to_string()];
                        if old_task.enabled {
                            if let Err(rb_err) = scheduler.schedule_task(&old_task).await {
                                errors.push(format!("rollback re-schedule failed: {}", rb_err));
                            }
                        }
                        return Err(TaktError::Database {
                            msg: errors.join("; "),
                        });
                    }
                };

                if task.enabled {
                    if let Err(e) = scheduler.schedule_task(&task).await {
                        let mut errors = vec![e.to_string()];
                        let db_reverted = store
                            .update_task(
                                &params.id,
                                Some(old_task.name.clone()),
                                Some(old_task.description.clone()),
                                Some(old_task.enabled),
                                Some(old_task.run_if_missed),
                                Some(old_task.notify_on_run),
                                Some(old_task.schedule.clone()),
                                Some(old_task.action.clone()),
                            )
                            .await;
                        match db_reverted {
                            Ok(_) => {
                                if old_task.enabled {
                                    if let Err(rb_err) = scheduler.schedule_task(&old_task).await {
                                        errors.push(format!(
                                            "rollback re-schedule failed: {}",
                                            rb_err
                                        ));
                                        if let Err(dis_err) = store
                                            .update_task(
                                                &params.id,
                                                None,
                                                None,
                                                Some(false),
                                                None,
                                                None,
                                                None,
                                                None,
                                            )
                                            .await
                                        {
                                            errors.push(format!(
                                                "disable failed: {} \u{2014} restart app to fix",
                                                dis_err
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(rb_err) => {
                                errors.push(format!("rollback failed: {}", rb_err));
                                if let Err(dis_err) = store
                                    .update_task(
                                        &params.id,
                                        None,
                                        None,
                                        Some(false),
                                        None,
                                        None,
                                        None,
                                        None,
                                    )
                                    .await
                                {
                                    errors.push(format!(
                                        "disable failed: {} \u{2014} restart app to fix",
                                        dis_err
                                    ));
                                }
                            }
                        }
                        return Err(TaktError::Scheduler {
                            msg: errors.join("; "),
                        });
                    }
                }
                Ok(task)
            })
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?
    }

    pub async fn delete_task(&self, id: String) -> Result<(), TaktError> {
        let store = self.store()?.clone();
        let scheduler = self.scheduler()?.clone();
        tokio_runtime()
            .spawn(async move {
                scheduler
                    .remove_task(&id)
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;
                store.delete_task(&id).await?;
                Ok(())
            })
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?
    }

    pub async fn run_task_now(&self, id: String) -> Result<(), TaktError> {
        let store = self.store()?.clone();
        let executor = self.executor_ref()?.clone();
        let bridge = self.bridge.clone();
        tokio_runtime()
            .spawn(async move {
                let task = store
                    .get_task(&id)
                    .await?
                    .ok_or_else(|| TaktError::NotFound {
                        msg: "Task not found".to_string(),
                    })?;
                scheduler::execute_and_log(
                    &*executor,
                    &store,
                    &*bridge,
                    &id,
                    &task.name,
                    task.notify_on_run,
                    &task.action,
                    &task.schedule,
                )
                .await
                .map_err(|msg| TaktError::Execution { msg })
            })
            .await
            .map_err(|e| TaktError::Execution { msg: e.to_string() })?
    }

    pub async fn list_logs(
        &self,
        task_id: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<ExecutionLog>, TaktError> {
        let store = self.store()?.clone();
        tokio_runtime()
            .spawn(async move {
                Ok(store
                    .list_logs(task_id.as_deref(), limit.unwrap_or(50).min(500))
                    .await?)
            })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }

    pub fn validate_cron(&self, expression: String) -> Result<(), TaktError> {
        croner::parser::CronParser::builder()
            .seconds(croner::parser::Seconds::Optional)
            .build()
            .parse(expression.trim())
            .map(|_| ())
            .map_err(|e| TaktError::Validation {
                msg: format!("Invalid cron expression: {}", e),
            })
    }
}

// Test-only: allow injecting an in-memory pool
#[cfg(test)]
impl TaktCore {
    async fn start_with_pool(&self, pool: sqlx::SqlitePool) -> Result<(), TaktError> {
        let bridge = self.bridge.clone();
        self.state
            .get_or_try_init(|| async {
                let store = Arc::new(TaskStore::new(pool));
                let executor = current_executor(bridge.clone());
                let scheduler = Arc::new(
                    AppScheduler::new(
                        Arc::clone(&store),
                        Arc::clone(&executor),
                        Arc::clone(&bridge),
                    )
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?,
                );
                scheduler
                    .start()
                    .await
                    .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;
                Ok::<_, TaktError>(InitializedState {
                    store,
                    scheduler,
                    executor,
                })
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
