use crate::models::{Action, CalendarEvent};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[allow(dead_code)]
    #[error("Unsupported action on this platform: {0}")]
    Unsupported(String),
    #[error("Accessibility permission required: {0}")]
    AccessibilityRequired(String),
    #[error("{0}")]
    MissingEventContext(String),
}

#[async_trait::async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(
        &self,
        action: &Action,
        event: Option<&CalendarEvent>,
    ) -> Result<ExecutionResult, ExecutorError>;
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub mod keymap;

#[cfg(target_os = "macos")]
pub use macos::MacosExecutor;

pub fn current_executor(
    bridge: std::sync::Arc<dyn crate::platform::PlatformBridge>,
) -> Arc<dyn ActionExecutor> {
    #[cfg(target_os = "macos")]
    return Arc::new(MacosExecutor::new(bridge));

    #[cfg(not(target_os = "macos"))]
    panic!("No executor available for this platform");
}

#[cfg(test)]
mod tests;
