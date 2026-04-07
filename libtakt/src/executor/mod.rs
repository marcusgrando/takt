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
pub mod keymap;

#[cfg(test)]
mod tests;
