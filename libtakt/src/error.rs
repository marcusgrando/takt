#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TaktError {
    #[error("Not found: {msg}")]
    NotFound { msg: String },
    #[error("Database error: {msg}")]
    Database { msg: String },
    #[error("Scheduler error: {msg}")]
    Scheduler { msg: String },
    #[error("Execution error: {msg}")]
    Execution { msg: String },
    #[error("Validation error: {msg}")]
    Validation { msg: String },
    #[error("Not initialized: call start() first")]
    NotInitialized,
    #[error("calendar access denied")]
    CalendarAccessDenied,
    #[error("calendar not found: {id}")]
    CalendarNotFound { id: String },
}

impl From<anyhow::Error> for TaktError {
    fn from(e: anyhow::Error) -> Self {
        let msg = e.to_string();
        if msg.contains("scheduler") || msg.contains("cron") || msg.contains("job") {
            TaktError::Scheduler { msg }
        } else {
            TaktError::Database { msg }
        }
    }
}
