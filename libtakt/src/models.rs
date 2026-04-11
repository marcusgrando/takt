use serde::de::{self, Deserializer};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
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
#[cfg_attr(test, derive(PartialEq))]
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
    Calendar {
        calendar_id: String,
        title_contains: Option<String>,
        minutes_before: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Modifier {
    Cmd,
    Shift,
    Opt,
    Ctrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(test, derive(PartialEq))]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

// ── Calendar feature: shared types ───────────────────────────────────
// These types land in Phase 1 as dormant data. They are populated and
// consumed by Phases 2–4. In Phase 1 nothing writes them.

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CalendarAccessStatus {
    NotDetermined,
    Denied,
    Authorized,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum TaskHealth {
    Healthy,
    CalendarNotFound,
    CalendarAccessDenied,
    CalendarAccessNotDetermined,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    pub source: String,
    pub color_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub notes: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub conference_url: Option<String>,
    pub calendar_id: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Action {
    OpenFile {
        path: String,
        app: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
        shortcut_delay_secs: u64,
    },
    OpenUrl {
        urls: Vec<String>,
        browser: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
        shortcut_delay_secs: u64,
    },
    OpenApp {
        app_path: String,
        post_shortcuts: Vec<KeyCombo>,
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

// Custom Serialize/Deserialize for Action to handle backward compatibility
// (old format: "url": "string" → new format: "urls": ["string", ...])
impl Serialize for Action {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Action::OpenFile { path, app, post_shortcuts, shortcut_delay_secs } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "OpenFile")?;
                map.serialize_entry("path", path)?;
                map.serialize_entry("app", app)?;
                map.serialize_entry("post_shortcuts", post_shortcuts)?;
                map.serialize_entry("shortcut_delay_secs", shortcut_delay_secs)?;
                map.end()
            }
            Action::OpenUrl { urls, browser, post_shortcuts, shortcut_delay_secs } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "OpenUrl")?;
                map.serialize_entry("urls", urls)?;
                map.serialize_entry("browser", browser)?;
                map.serialize_entry("post_shortcuts", post_shortcuts)?;
                map.serialize_entry("shortcut_delay_secs", shortcut_delay_secs)?;
                map.end()
            }
            Action::OpenApp { app_path, post_shortcuts, shortcut_delay_secs } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "OpenApp")?;
                map.serialize_entry("app_path", app_path)?;
                map.serialize_entry("post_shortcuts", post_shortcuts)?;
                map.serialize_entry("shortcut_delay_secs", shortcut_delay_secs)?;
                map.end()
            }
            Action::RunCommand { command, args, shell } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "RunCommand")?;
                map.serialize_entry("command", command)?;
                map.serialize_entry("args", args)?;
                map.serialize_entry("shell", shell)?;
                map.end()
            }
            Action::Notify { title, body, sound } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "Notify")?;
                map.serialize_entry("title", title)?;
                map.serialize_entry("body", body)?;
                map.serialize_entry("sound", sound)?;
                map.end()
            }
            Action::Webhook { url, method, headers, body } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "Webhook")?;
                map.serialize_entry("url", url)?;
                map.serialize_entry("method", method)?;
                map.serialize_entry("headers", headers)?;
                map.serialize_entry("body", body)?;
                map.end()
            }
            Action::Settings { pane_url } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "Settings")?;
                map.serialize_entry("pane_url", pane_url)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value.as_object().ok_or_else(|| de::Error::custom("expected object"))?;
        let action_type = obj.get("type").and_then(|v| v.as_str()).ok_or_else(|| de::Error::missing_field("type"))?;

        match action_type {
            "OpenFile" => Ok(Action::OpenFile {
                path: obj.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                app: obj.get("app").and_then(|v| v.as_str()).map(String::from),
                post_shortcuts: obj.get("post_shortcuts").map(|v| serde_json::from_value(v.clone()).unwrap_or_default()).unwrap_or_default(),
                shortcut_delay_secs: obj.get("shortcut_delay_secs").and_then(|v| v.as_u64()).unwrap_or_else(default_shortcut_delay),
            }),
            "OpenUrl" => {
                // Backward compat: accept "url" (string) or "urls" (array)
                let urls = if let Some(arr) = obj.get("urls").and_then(|v| v.as_array()) {
                    arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                } else if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
                    vec![url.to_string()]
                } else {
                    vec![]
                };
                Ok(Action::OpenUrl {
                    urls,
                    browser: obj.get("browser").and_then(|v| v.as_str()).map(String::from),
                    post_shortcuts: obj.get("post_shortcuts").map(|v| serde_json::from_value(v.clone()).unwrap_or_default()).unwrap_or_default(),
                    shortcut_delay_secs: obj.get("shortcut_delay_secs").and_then(|v| v.as_u64()).unwrap_or_else(default_shortcut_delay),
                })
            }
            "OpenApp" => Ok(Action::OpenApp {
                app_path: obj.get("app_path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                post_shortcuts: obj.get("post_shortcuts").map(|v| serde_json::from_value(v.clone()).unwrap_or_default()).unwrap_or_default(),
                shortcut_delay_secs: obj.get("shortcut_delay_secs").and_then(|v| v.as_u64()).unwrap_or_else(default_shortcut_delay),
            }),
            "RunCommand" => Ok(Action::RunCommand {
                command: obj.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                args: obj.get("args").map(|v| serde_json::from_value(v.clone()).unwrap_or_default()).unwrap_or_default(),
                shell: obj.get("shell").map(|v| serde_json::from_value(v.clone()).unwrap_or(Shell::Zsh)).unwrap_or(Shell::Zsh),
            }),
            "Notify" => Ok(Action::Notify {
                title: obj.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                body: obj.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                sound: obj.get("sound").and_then(|v| v.as_bool()).unwrap_or(false),
            }),
            "Webhook" => Ok(Action::Webhook {
                url: obj.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                method: obj.get("method").map(|v| serde_json::from_value(v.clone()).unwrap_or(HttpMethod::GET)).unwrap_or(HttpMethod::GET),
                headers: obj.get("headers").map(|v| serde_json::from_value(v.clone()).unwrap_or_default()).unwrap_or_default(),
                body: obj.get("body").and_then(|v| v.as_str()).map(String::from),
            }),
            "Settings" => Ok(Action::Settings {
                pane_url: obj.get("pane_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }),
            other => Err(de::Error::unknown_variant(other, &["OpenFile", "OpenUrl", "OpenApp", "RunCommand", "Notify", "Webhook", "Settings"])),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Shell {
    Sh,
    Bash,
    Zsh,
    Python,
    AppleScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(test, derive(PartialEq))]
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
