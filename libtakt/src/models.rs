use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_shortcut_delay() -> u64 {
    1
}

fn default_first_use_delay() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String, // UUID as string for SQLite
    pub name: String,
    pub description: Option<String>,
    pub enabled: i64,          // SQLite stores booleans as INTEGER (0/1)
    pub run_if_missed: i64,    // catch-up: run on wake if missed
    pub notify_on_run: i64,    // send notification after execution
    pub schedule_json: String, // JSON-serialized Schedule
    pub action_json: String,   // JSON-serialized Action
    pub created_at: String,    // ISO 8601
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct TaskDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub run_if_missed: bool,
    pub notify_on_run: bool,
    pub schedule: Schedule,
    pub action: Action,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type")]
pub enum Schedule {
    Cron {
        expression: String,
    },
    OneShot {
        run_at: String,
    }, // ISO 8601 DateTime
    DailyFirstUse {
        #[serde(default = "default_first_use_delay")]
        delay_minutes: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum Modifier {
    Cmd,
    Shift,
    Opt,
    Ctrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type")]
pub enum Action {
    OpenFile {
        path: String,
        app: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
        #[serde(default = "default_shortcut_delay")]
        shortcut_delay_secs: u64,
    },
    OpenUrl {
        url: String,
        browser: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
        #[serde(default = "default_shortcut_delay")]
        shortcut_delay_secs: u64,
    },
    OpenApp {
        app_path: String,
        post_shortcuts: Vec<KeyCombo>,
        #[serde(default = "default_shortcut_delay")]
        shortcut_delay_secs: u64,
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
    Settings {
        pane_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum Shell {
    Sh,
    Bash,
    Zsh,
    Python,
    AppleScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[allow(clippy::upper_case_acronyms)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, uniffi::Record)]
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

#[derive(Debug, Clone, uniffi::Record)]
pub struct CreateTaskParams {
    pub name: String,
    pub description: Option<String>,
    pub run_if_missed: Option<bool>,
    pub notify_on_run: Option<bool>,
    pub schedule: Schedule,
    pub action: Action,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdateTaskParams {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub run_if_missed: Option<bool>,
    pub notify_on_run: Option<bool>,
    pub schedule: Option<Schedule>,
    pub action: Option<Action>,
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
            run_if_missed: self.run_if_missed != 0,
            notify_on_run: self.notify_on_run != 0,
            schedule,
            action,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_run_at: self.last_run_at.clone(),
            next_run_at: self.next_run_at.clone(),
        })
    }
}
