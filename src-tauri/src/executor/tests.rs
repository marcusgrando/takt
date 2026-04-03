#[cfg(test)]
mod tests {
    use crate::executor::macos::MacosExecutor;
    use crate::executor::ActionExecutor;
    use crate::models::{Action, Shell};

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_run_shell_command() {
        let executor = MacosExecutor::new();
        let action = Action::RunCommand {
            command: "echo hello".to_string(),
            args: vec![],
            shell: Shell::Sh,
        };
        let result = executor.execute(&action).await.unwrap();
        assert_eq!(result.stdout.unwrap().trim(), "hello");
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_open_url_skipped_in_ci() {
        // Only test that it doesn't panic with a reasonable URL
        if std::env::var("CI").is_ok() {
            return;
        }
        let executor = MacosExecutor::new();
        let action = Action::OpenUrl {
            url: "https://example.com".to_string(),
            browser: None,
        };
        let result = executor.execute(&action).await;
        assert!(result.is_ok());
    }
}
