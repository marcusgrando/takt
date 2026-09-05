#[cfg(test)]
mod tests {
    use crate::db;
    use crate::error::TaktError;
    use crate::models::{Action, CreateTaskParams, Schedule, Shell, UpdateTaskParams};
    use crate::platform::PlatformBridge;
    use crate::TaktCore;
    use std::sync::Arc;

    struct MockBridge;

    impl PlatformBridge for MockBridge {
        fn send_notification(&self, _title: String, _body: String, _sound: bool) {}
        fn run_on_main_sync(&self, callback_id: u64) {
            crate::platform::execute_callback(callback_id);
        }
        fn get_user_activity_snapshot(&self) -> crate::models::UserActivitySnapshot {
            crate::models::UserActivitySnapshot {
                session_active: true,
                eligibility_generation: 0,
                input_event_count: 0,
                eligibility_input_event_count: 0,
                last_input_at_unix_millis: None,
            }
        }
        fn get_calendar_access_status(
            &self,
        ) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution {
                msg: "not_implemented".to_string(),
            })
        }
        fn request_calendar_access(
            &self,
        ) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution {
                msg: "not_implemented".to_string(),
            })
        }
        fn list_calendars(
            &self,
        ) -> Result<Vec<crate::models::CalendarInfo>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution {
                msg: "not_implemented".to_string(),
            })
        }
        fn fetch_events_in_window(
            &self,
            _calendar_id: String,
            _lookback_minutes: u32,
            _lookahead_minutes: u32,
        ) -> Result<Vec<crate::models::CalendarEvent>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution {
                msg: "not_implemented".to_string(),
            })
        }
        fn fetch_event_instance(
            &self,
            _calendar_id: String,
            _event_id: String,
            _event_start: String,
        ) -> Result<Option<crate::models::CalendarEvent>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution {
                msg: "not_implemented".to_string(),
            })
        }
    }

    async fn test_core() -> TaktCore {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);
        let pool = db::connect_in_memory().await.unwrap();
        core.start_with_pool(pool).await.unwrap();
        core
    }

    #[tokio::test]
    async fn test_start_is_idempotent() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);
        let pool = db::connect_in_memory().await.unwrap();
        core.start_with_pool(pool).await.unwrap();
        // Second call should return Ok immediately
        core.start().await.unwrap();
        let tasks = core.list_tasks().await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn concurrent_start_initializes_resources_once() {
        let core = TaktCore::new(Arc::new(MockBridge));
        let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let connect = || {
            let connections = connections.clone();
            move || async move {
                connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                db::connect_in_memory().await
            }
        };
        let (first, second) = tokio::join!(
            core.start_with_connector(connect()),
            core.start_with_connector(connect()),
        );
        first.unwrap();
        second.unwrap();
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(core.list_tasks().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_start_can_retry_without_publishing_partial_state() {
        let core = TaktCore::new(Arc::new(MockBridge));
        let failed = core
            .start_with_connector(|| async { anyhow::bail!("unavailable database") })
            .await;
        assert!(failed.is_err());
        assert!(matches!(
            core.list_tasks().await,
            Err(TaktError::NotInitialized)
        ));
        core.start_with_connector(db::connect_in_memory)
            .await
            .unwrap();
        assert!(core.list_tasks().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancelling_start_waiter_does_not_duplicate_initialization() {
        let core = Arc::new(TaktCore::new(Arc::new(MockBridge)));
        let starting_core = core.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async move {
            starting_core
                .start_with_connector(|| async move {
                    started_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    db::connect_in_memory().await
                })
                .await
        });
        started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release_tx.send(()).unwrap();
        core.start_with_connector(|| async {
            anyhow::bail!("initialization must remain owned by the original worker")
        })
        .await
        .unwrap();
        assert!(core.list_tasks().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_methods_fail_before_start() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);
        let result = core.list_tasks().await;
        assert!(matches!(result, Err(TaktError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let core = test_core().await;
        let params = CreateTaskParams {
            name: "Test Task".to_string(),
            description: None,
            run_if_missed: None,
            notify_on_run: None,
            schedule: Schedule::Cron {
                expression: "0 9 * * 1-5".to_string(),
            },
            action: Action::RunCommand {
                command: "echo test".to_string(),
                args: vec![],
                shell: Shell::Sh,
            },
        };
        let task = core.create_task(params).await.unwrap();
        assert_eq!(task.name, "Test Task");
        assert!(task.enabled);
        let tasks = core.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let core = test_core().await;
        let params = CreateTaskParams {
            name: "Delete Me".to_string(),
            description: None,
            run_if_missed: None,
            notify_on_run: None,
            schedule: Schedule::DailyFirstUse { delay_minutes: 5 },
            action: Action::Notify {
                title: "Hi".to_string(),
                body: "World".to_string(),
                sound: false,
            },
        };
        let task = core.create_task(params).await.unwrap();
        core.delete_task(task.id).await.unwrap();
        let tasks = core.list_tasks().await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_update_task_preserves_state() {
        let core = test_core().await;
        let params = CreateTaskParams {
            name: "Original".to_string(),
            description: None,
            run_if_missed: None,
            notify_on_run: None,
            schedule: Schedule::Cron {
                expression: "0 9 * * 1-5".to_string(),
            },
            action: Action::RunCommand {
                command: "echo test".to_string(),
                args: vec![],
                shell: Shell::Sh,
            },
        };
        let task = core.create_task(params).await.unwrap();
        let updated = core
            .update_task(UpdateTaskParams {
                id: task.id.clone(),
                name: Some("Updated".to_string()),
                description: None,
                enabled: None,
                run_if_missed: None,
                notify_on_run: None,
                schedule: None,
                action: None,
            })
            .await
            .unwrap();
        assert_eq!(updated.name, "Updated");
        assert!(updated.enabled);
    }

    #[tokio::test]
    async fn test_update_nonexistent_returns_not_found() {
        let core = test_core().await;
        let result = core
            .update_task(UpdateTaskParams {
                id: "nonexistent".to_string(),
                name: Some("Nope".to_string()),
                description: None,
                enabled: None,
                run_if_missed: None,
                notify_on_run: None,
                schedule: None,
                action: None,
            })
            .await;
        assert!(matches!(result, Err(TaktError::NotFound { .. })));
    }
}
