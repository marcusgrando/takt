use crate::models::Action;

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
    #[error("Unsupported action on this platform: {0}")]
    Unsupported(String),
    #[error("Accessibility permission required: {0}")]
    AccessibilityRequired(String),
}

#[async_trait::async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError>;
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub mod keymap;

#[cfg(target_os = "macos")]
pub use macos::MacosExecutor;

pub fn current_executor(
    bridge: std::sync::Arc<dyn crate::platform::PlatformBridge>,
) -> Box<dyn ActionExecutor> {
    #[cfg(target_os = "macos")]
    return Box::new(MacosExecutor::new(bridge));

    #[cfg(not(target_os = "macos"))]
    panic!("No executor available for this platform");
}

#[cfg(test)]
mod tests;
