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
        fn is_user_active(&self, _idle_threshold_secs: u64) -> bool {
            true
        }
        fn get_calendar_access_status(&self) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution { msg: "not_implemented".to_string() })
        }
        fn request_calendar_access(&self) -> Result<crate::models::CalendarAccessStatus, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution { msg: "not_implemented".to_string() })
        }
        fn list_calendars(&self) -> Result<Vec<crate::models::CalendarInfo>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution { msg: "not_implemented".to_string() })
        }
        fn fetch_events_in_window(
            &self,
            _calendar_id: String,
            _lookback_minutes: u32,
            _lookahead_minutes: u32,
        ) -> Result<Vec<crate::models::CalendarEvent>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution { msg: "not_implemented".to_string() })
        }
        fn fetch_event_instance(
            &self,
            _calendar_id: String,
            _event_id: String,
            _event_start: String,
        ) -> Result<Option<crate::models::CalendarEvent>, crate::error::TaktError> {
            Err(crate::error::TaktError::Execution { msg: "not_implemented".to_string() })
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
