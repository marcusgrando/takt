use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String, // UUID as string for SQLite
    pub name: String,
    pub description: Option<String>,
    pub enabled: i64,          // SQLite stores booleans as INTEGER (0/1)
    pub schedule_json: String, // JSON-serialized Schedule
    pub action_json: String,   // JSON-serialized Action
    pub created_at: String,    // ISO 8601
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub schedule: Schedule,
    pub action: Action,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Schedule {
    Cron { expression: String },
    OneShot { run_at: String }, // ISO 8601 DateTime
    OnLogin,
    OnWake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Modifier {
    Cmd,
    Shift,
    Opt,
    Ctrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    OpenFile {
        path: String,
        app: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
    },
    OpenUrl {
        url: String,
        browser: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
    },
    OpenApp {
        app_path: String,
        post_shortcuts: Vec<KeyCombo>,
    },
    RunCommand {
        command: String,
        args: Vec<String>,
        shell: Shell,
    },
    Notify {
        title: String,
        body: String,
        sound: bool,
    },
    Webhook {
        url: String,
        method: HttpMethod,
        headers: HashMap<String, String>,
        body: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Shell {
    Sh,
    Bash,
    Zsh,
    Python,
    AppleScript,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionLog {
    pub id: String,
    pub task_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String, // "success" | "failure" | "skipped"
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub error: Option<String>,
}

impl Task {
    pub fn to_dto(&self) -> Result<TaskDto, serde_json::Error> {
        let schedule: Schedule = serde_json::from_str(&self.schedule_json)?;
        let action: Action = serde_json::from_str(&self.action_json)?;
        Ok(TaskDto {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            enabled: self.enabled != 0,
            schedule,
            action,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_run_at: self.last_run_at.clone(),
            next_run_at: self.next_run_at.clone(),
        })
    }
}
