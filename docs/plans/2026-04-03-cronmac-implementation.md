# cronmac Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a macOS menu bar app for scheduling automated actions (open files/apps, run scripts, open URLs, send notifications, simulate shortcuts, fire webhooks) using Tauri 2 + React + Rust.

**Architecture:** Tauri 2 app with no Dock icon — lives exclusively in the menu bar. Rust backend handles scheduling (`tokio-cron-scheduler`), persistence (SQLite via sqlx), and action execution via an `ActionExecutor` trait with a `MacosExecutor` implementation. React frontend renders a popover on menu bar click and modal windows for task management.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Rust, tokio, tokio-cron-scheduler, sqlx (SQLite), @tanstack/query, tailwindcss, shadcn/ui

---

## Task 1: Project Scaffold

**Files:**
- Create: `src-tauri/` (Tauri Rust backend)
- Create: `src/` (React frontend)
- Create: `src-tauri/Cargo.toml`
- Create: `package.json`

**Step 1: Bootstrap Tauri 2 + React project**

```bash
npm create tauri-app@latest cronmac -- --template react-ts
cd cronmac
npm install
```

**Step 2: Verify dev server starts**

```bash
npm run tauri dev
```
Expected: App window opens in development mode.

**Step 3: Install frontend dependencies**

```bash
npm install @tanstack/react-query @tanstack/react-query-devtools
npm install tailwindcss @tailwindcss/vite
npm install lucide-react date-fns
npm install -D @types/node
npx shadcn@latest init
```

**Step 4: Install Rust dependencies**

Add to `src-tauri/Cargo.toml` under `[dependencies]`:
```toml
tokio = { version = "1", features = ["full"] }
tokio-cron-scheduler = "0.13"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "macros", "migrate", "uuid", "chrono"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
anyhow = "1"
thiserror = "1"
tauri-plugin-notification = "2"
tauri-plugin-shell = "2"
```

**Step 5: Install Tauri plugins**

```bash
npm run tauri add notification
npm run tauri add shell
```

**Step 6: Commit**

```bash
git add .
git commit -m "feat: scaffold Tauri 2 + React project with dependencies"
```

---

## Task 2: Configure Menu Bar — No Dock Icon

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/icons/tray-icon.png` (32x32 tray icon)

**Step 1: Configure tauri.conf.json for menu bar mode**

In `src-tauri/tauri.conf.json`, set:
```json
{
  "app": {
    "windows": [],
    "trayIcon": {
      "iconPath": "icons/tray-icon.png",
      "iconAsTemplate": true
    }
  },
  "bundle": {
    "macOS": {
      "minimumSystemVersion": "13.0",
      "activationPolicy": "Accessory"
    }
  }
}
```

`activationPolicy: "Accessory"` is the key — it hides the Dock icon.

**Step 2: Set up the tray in lib.rs**

Replace `src-tauri/src/lib.rs` with:

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager, AppHandle,
};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Hide from dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit cronmac", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle();
                toggle_main_window(app);
            }
        })
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
```

**Step 3: Create a placeholder tray icon**

Place a 32x32 PNG at `src-tauri/icons/tray-icon.png` (can use the default tauri icon initially, rename to tray-icon.png).

**Step 4: Verify no Dock icon appears**

```bash
npm run tauri dev
```
Expected: App runs in menu bar only. Clicking tray icon shows/hides window.

**Step 5: Commit**

```bash
git add src-tauri/
git commit -m "feat: configure menu bar only mode, no dock icon"
```

---

## Task 3: Data Models and SQLite Setup

**Files:**
- Create: `src-tauri/src/models.rs`
- Create: `src-tauri/migrations/001_initial.sql`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create the data models**

Create `src-tauri/src/models.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String,           // UUID as string for SQLite
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub schedule_json: String,  // JSON-serialized Schedule
    pub action_json: String,    // JSON-serialized Action
    pub created_at: String,     // ISO 8601
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
    OneShot { run_at: String },  // ISO 8601 DateTime
    OnLogin,
    OnWake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    OpenFile { path: String },
    OpenUrl { url: String, browser: Option<String> },
    RunCommand { command: String, args: Vec<String>, shell: Shell },
    Notify { title: String, body: String, sound: bool },
    Shortcut { keys: Vec<String> },
    Webhook {
        url: String,
        method: HttpMethod,
        headers: HashMap<String, String>,
        body: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Shell {
    Sh, Bash, Zsh, Python, AppleScript,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    GET, POST, PUT, PATCH, DELETE,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionLog {
    pub id: String,
    pub task_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,  // "success" | "failure" | "skipped"
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
            enabled: self.enabled,
            schedule,
            action,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_run_at: self.last_run_at.clone(),
            next_run_at: self.next_run_at.clone(),
        })
    }
}
```

**Step 2: Create database migration**

Create `src-tauri/migrations/001_initial.sql`:

```sql
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    schedule_json TEXT NOT NULL,
    action_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_run_at TEXT,
    next_run_at TEXT
);

CREATE TABLE IF NOT EXISTS execution_logs (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('success', 'failure', 'skipped')),
    stdout TEXT,
    stderr TEXT,
    error TEXT
);

CREATE INDEX idx_execution_logs_task_id ON execution_logs(task_id);
CREATE INDEX idx_execution_logs_started_at ON execution_logs(started_at);
```

**Step 3: Add module declarations to lib.rs**

Add to `src-tauri/src/lib.rs`:
```rust
mod models;
mod db;
```

**Step 4: Create db.rs**

Create `src-tauri/src/db.rs`:

```rust
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;

pub async fn connect() -> anyhow::Result<SqlitePool> {
    let db_path = db_path();
    std::fs::create_dir_all(db_path.parent().unwrap())?;

    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_str().unwrap());
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

fn db_path() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cronmac");
    base.join("cronmac.db")
}
```

Add `dirs = "5"` to `src-tauri/Cargo.toml` dependencies.

**Step 5: Write unit test for db connection**

Add to `src-tauri/src/db.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_connects_and_migrates() {
        // Use in-memory SQLite for tests
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }
}
```

**Step 6: Run tests**

```bash
cd src-tauri && cargo test
```
Expected: test passes.

**Step 7: Commit**

```bash
git add src-tauri/
git commit -m "feat: add data models, SQLite schema, and db connection"
```

---

## Task 4: ActionExecutor Trait and MacosExecutor

**Files:**
- Create: `src-tauri/src/executor/mod.rs`
- Create: `src-tauri/src/executor/macos.rs`
- Create: `src-tauri/src/executor/tests.rs`

**Step 1: Create executor module**

Create `src-tauri/src/executor/mod.rs`:

```rust
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
}

#[async_trait::async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError>;
    fn platform_name(&self) -> &'static str;
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacosExecutor;

/// Returns the correct executor for the current platform
pub fn current_executor() -> Box<dyn ActionExecutor> {
    #[cfg(target_os = "macos")]
    return Box::new(MacosExecutor::new());

    #[cfg(not(any(target_os = "macos")))]
    panic!("No executor available for this platform");
}
```

Add `async-trait = "0.1"` to `src-tauri/Cargo.toml`.

**Step 2: Implement MacosExecutor**

Create `src-tauri/src/executor/macos.rs`:

```rust
use super::{ActionExecutor, ExecutionResult, ExecutorError};
use crate::models::{Action, HttpMethod, Shell};
use async_trait::async_trait;
use std::process::Command;

pub struct MacosExecutor;

impl MacosExecutor {
    pub fn new() -> Self { MacosExecutor }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    fn platform_name(&self) -> &'static str { "macos" }

    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile { path } => open_file(path),
            Action::OpenUrl { url, browser } => open_url(url, browser.as_deref()),
            Action::RunCommand { command, args, shell } => run_command(command, args, shell),
            Action::Notify { title, body, sound: _ } => send_notification(title, body),
            Action::Shortcut { keys } => send_shortcut(keys),
            Action::Webhook { url, method, headers, body } => {
                send_webhook(url, method, headers, body.as_deref()).await
            }
        }
    }
}

fn open_file(path: &str) -> Result<ExecutionResult, ExecutorError> {
    let output = Command::new("open").arg(path).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}

fn open_url(url: &str, browser: Option<&str>) -> Result<ExecutionResult, ExecutorError> {
    let mut cmd = Command::new("open");
    if let Some(b) = browser {
        cmd.arg("-a").arg(b);
    }
    let output = cmd.arg(url).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}

fn run_command(command: &str, args: &[String], shell: &Shell) -> Result<ExecutionResult, ExecutorError> {
    let full_command = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    let (shell_bin, shell_flag) = match shell {
        Shell::Sh => ("/bin/sh", "-c"),
        Shell::Bash => ("/bin/bash", "-c"),
        Shell::Zsh => ("/bin/zsh", "-c"),
        Shell::Python => ("/usr/bin/python3", "-c"),
        Shell::AppleScript => ("/usr/bin/osascript", "-e"),
    };

    let output = Command::new(shell_bin)
        .arg(shell_flag)
        .arg(&full_command)
        .output()?;

    Ok(ExecutionResult {
        stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
        stderr: if output.stderr.is_empty() { None }
                else { Some(String::from_utf8_lossy(&output.stderr).to_string()) },
    })
}

fn send_notification(title: &str, body: &str) -> Result<ExecutionResult, ExecutorError> {
    let script = format!(
        r#"display notification "{}" with title "{}""#,
        body.replace('"', r#"\""#),
        title.replace('"', r#"\""#)
    );
    let output = Command::new("osascript").arg("-e").arg(&script).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}

fn send_shortcut(keys: &[String]) -> Result<ExecutionResult, ExecutorError> {
    // Build AppleScript keystroke
    let keystroke = keys.last().unwrap_or(&String::new()).clone();
    let modifiers: Vec<&str> = keys[..keys.len().saturating_sub(1)]
        .iter()
        .filter_map(|k| match k.as_str() {
            "cmd" | "command" => Some("command down"),
            "shift" => Some("shift down"),
            "opt" | "option" => Some("option down"),
            "ctrl" | "control" => Some("control down"),
            _ => None,
        })
        .collect();

    let using_clause = if modifiers.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", modifiers.join(", "))
    };

    let script = format!(
        r#"tell application "System Events" to keystroke "{}"{}"#,
        keystroke, using_clause
    );

    let output = Command::new("osascript").arg("-e").arg(&script).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}

async fn send_webhook(
    url: &str,
    method: &HttpMethod,
    headers: &std::collections::HashMap<String, String>,
    body: Option<&str>,
) -> Result<ExecutionResult, ExecutorError> {
    let client = reqwest::Client::new();
    let mut req = match method {
        HttpMethod::GET => client.get(url),
        HttpMethod::POST => client.post(url),
        HttpMethod::PUT => client.put(url),
        HttpMethod::PATCH => client.patch(url),
        HttpMethod::DELETE => client.delete(url),
    };

    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.body(b.to_string());
    }

    let response = req.send().await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;

    let status = response.status();
    let response_body = response.text().await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;

    if status.is_success() {
        Ok(ExecutionResult {
            stdout: Some(response_body),
            stderr: None,
        })
    } else {
        Err(ExecutorError::CommandFailed(
            format!("HTTP {} — {}", status, response_body)
        ))
    }
}
```

**Step 3: Write unit tests for executor**

Create `src-tauri/src/executor/tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::executor::macos::MacosExecutor;
    use crate::executor::ActionExecutor;
    use crate::models::Action;

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_open_url_succeeds() {
        let executor = MacosExecutor::new();
        // We only test that it doesn't panic with a reasonable URL
        // (actual open is side-effectful, skip in CI with env var)
        if std::env::var("CI").is_ok() { return; }

        let action = Action::OpenUrl {
            url: "https://example.com".to_string(),
            browser: None,
        };
        let result = executor.execute(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_run_shell_command() {
        use crate::models::Shell;
        let executor = MacosExecutor::new();
        let action = Action::RunCommand {
            command: "echo hello".to_string(),
            args: vec![],
            shell: Shell::Sh,
        };
        let result = executor.execute(&action).await.unwrap();
        assert_eq!(result.stdout.unwrap().trim(), "hello");
    }
}
```

**Step 4: Add module declarations**

Add to `src-tauri/src/lib.rs`:
```rust
mod executor;
```

**Step 5: Run tests**

```bash
cd src-tauri && cargo test executor
```
Expected: `test_run_shell_command` passes.

**Step 6: Commit**

```bash
git add src-tauri/
git commit -m "feat: add ActionExecutor trait and MacosExecutor implementation"
```

---

## Task 5: Task Store (CRUD operations)

**Files:**
- Create: `src-tauri/src/store.rs`

**Step 1: Create TaskStore**

Create `src-tauri/src/store.rs`:

```rust
use crate::models::{Task, TaskDto, ExecutionLog, Schedule, Action};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TaskStore {
    pool: SqlitePool,
}

impl TaskStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_tasks(&self) -> anyhow::Result<Vec<TaskDto>> {
        let rows: Vec<Task> = sqlx::query_as("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(|t| t.to_dto().map_err(|e| anyhow::anyhow!(e))).collect()
    }

    pub async fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskDto>> {
        let row: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|t| t.to_dto().map_err(|e| anyhow::anyhow!(e))).transpose()
    }

    pub async fn create_task(&self, name: String, description: Option<String>, schedule: Schedule, action: Action) -> anyhow::Result<TaskDto> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let schedule_json = serde_json::to_string(&schedule)?;
        let action_json = serde_json::to_string(&action)?;

        sqlx::query(
            "INSERT INTO tasks (id, name, description, enabled, schedule_json, action_json, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&name)
        .bind(&description)
        .bind(&schedule_json)
        .bind(&action_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get_task(&id).await?.ok_or_else(|| anyhow::anyhow!("Task not found after insert"))
    }

    pub async fn update_task(&self, id: &str, name: Option<String>, description: Option<Option<String>>, enabled: Option<bool>, schedule: Option<Schedule>, action: Option<Action>) -> anyhow::Result<TaskDto> {
        let existing = self.get_task(id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;

        let name = name.unwrap_or(existing.name);
        let description = description.unwrap_or(existing.description);
        let enabled = enabled.unwrap_or(existing.enabled);
        let schedule = schedule.unwrap_or(existing.schedule);
        let action = action.unwrap_or(existing.action);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE tasks SET name=?, description=?, enabled=?, schedule_json=?, action_json=?, updated_at=? WHERE id=?"
        )
        .bind(&name)
        .bind(&description)
        .bind(enabled)
        .bind(serde_json::to_string(&schedule)?)
        .bind(serde_json::to_string(&action)?)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        self.get_task(id).await?.ok_or_else(|| anyhow::anyhow!("Task not found after update"))
    }

    pub async fn delete_task(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_last_run(&self, id: &str, next_run: Option<String>) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE tasks SET last_run_at=?, next_run_at=?, updated_at=? WHERE id=?")
            .bind(&now)
            .bind(next_run)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn log_execution(&self, task_id: &str, status: &str, stdout: Option<String>, stderr: Option<String>, error: Option<String>) -> anyhow::Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO execution_logs (id, task_id, started_at, finished_at, status, stdout, stderr, error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(task_id)
        .bind(&now)
        .bind(&now)
        .bind(status)
        .bind(stdout)
        .bind(stderr)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_logs(&self, task_id: Option<&str>, limit: i64) -> anyhow::Result<Vec<ExecutionLog>> {
        let logs = if let Some(tid) = task_id {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs WHERE task_id = ? ORDER BY started_at DESC LIMIT ?"
            )
            .bind(tid)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ExecutionLog>(
                "SELECT * FROM execution_logs ORDER BY started_at DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(logs)
    }
}
```

**Step 2: Write CRUD tests**

Add `#[cfg(test)]` block to `src-tauri/src/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Schedule, Action, Shell};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_store() -> TaskStore {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        TaskStore::new(pool)
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let store = test_store().await;
        let task = store.create_task(
            "Test Task".to_string(),
            None,
            Schedule::Cron { expression: "0 8 * * 1-5".to_string() },
            Action::RunCommand {
                command: "echo test".to_string(),
                args: vec![],
                shell: Shell::Sh,
            }
        ).await.unwrap();

        assert_eq!(task.name, "Test Task");
        assert!(task.enabled);

        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let store = test_store().await;
        let task = store.create_task(
            "Delete Me".to_string(),
            None,
            Schedule::OnLogin,
            Action::Notify { title: "Hi".to_string(), body: "World".to_string(), sound: false },
        ).await.unwrap();

        store.delete_task(&task.id).await.unwrap();
        let tasks = store.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 0);
    }
}
```

**Step 3: Run tests**

```bash
cd src-tauri && cargo test store
```
Expected: all store tests pass.

**Step 4: Commit**

```bash
git add src-tauri/
git commit -m "feat: add TaskStore with CRUD and execution log persistence"
```

---

## Task 6: Scheduler Integration

**Files:**
- Create: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create scheduler module**

Create `src-tauri/src/scheduler.rs`:

```rust
use crate::executor::{ActionExecutor, current_executor};
use crate::models::{Schedule, TaskDto};
use crate::store::TaskStore;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
}

impl AppScheduler {
    pub async fn new(store: Arc<TaskStore>) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        let executor = Arc::new(current_executor());
        Ok(Self { inner, executor, store })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.inner.start().await?;
        Ok(())
    }

    pub async fn load_all_tasks(&self) -> anyhow::Result<()> {
        let tasks = self.store.list_tasks().await?;
        for task in tasks {
            if task.enabled {
                self.schedule_task(&task).await?;
            }
        }
        Ok(())
    }

    pub async fn schedule_task(&self, task: &TaskDto) -> anyhow::Result<()> {
        let task_id = task.id.clone();
        let action = task.action.clone();
        let executor = Arc::clone(&self.executor);
        let store = Arc::clone(&self.store);

        match &task.schedule {
            Schedule::Cron { expression } => {
                let expr = expression.clone();
                let job = Job::new_async(expr.as_str(), move |_uuid, _lock| {
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id.clone();
                    Box::pin(async move {
                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    })
                })?;
                self.inner.add(job).await?;
            }
            Schedule::OneShot { run_at } => {
                // Parse ISO 8601 and schedule a one-time job
                use chrono::DateTime;
                let run_at: chrono::DateTime<chrono::Utc> = run_at.parse()?;
                let now = chrono::Utc::now();
                if run_at > now {
                    let delay = (run_at - now).to_std()?;
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    });
                }
            }
            Schedule::OnLogin | Schedule::OnWake => {
                // These are dispatched immediately on app startup
                let action = action.clone();
                let executor = Arc::clone(&executor);
                let store = Arc::clone(&store);
                let task_id = task_id.clone();
                tokio::spawn(async move {
                    let result = executor.execute(&action).await;
                    let (status, stdout, stderr, error) = match result {
                        Ok(r) => ("success", r.stdout, r.stderr, None),
                        Err(e) => ("failure", None, None, Some(e.to_string())),
                    };
                    let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                    let _ = store.update_last_run(&task_id, None).await;
                });
            }
        }
        Ok(())
    }

    pub async fn remove_task(&self, _task_id: &str) -> anyhow::Result<()> {
        // tokio-cron-scheduler supports removing by UUID
        // For simplicity in v1, a restart re-loads all active tasks
        // Full removal by ID requires storing job UUIDs — add in v2
        Ok(())
    }
}
```

**Step 2: Wire scheduler into app state in lib.rs**

Add `AppState` and connect scheduler to app startup:

```rust
use std::sync::Arc;
use crate::db;
use crate::store::TaskStore;
use crate::scheduler::AppScheduler;

pub struct AppState {
    pub store: Arc<TaskStore>,
    pub scheduler: Arc<AppScheduler>,
}
```

In `setup` callback of `tauri::Builder::default()`:

```rust
.setup(|app| {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    setup_tray(app.handle())?;

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let pool = db::connect().await.expect("DB connect failed");
        let store = Arc::new(TaskStore::new(pool));
        let scheduler = Arc::new(
            AppScheduler::new(Arc::clone(&store)).await.expect("Scheduler init failed")
        );
        scheduler.start().await.expect("Scheduler start failed");
        scheduler.load_all_tasks().await.expect("Load tasks failed");

        handle.manage(AppState { store, scheduler });
    });

    Ok(())
})
```

**Step 3: Commit**

```bash
git add src-tauri/
git commit -m "feat: add tokio-cron-scheduler integration and app state"
```

---

## Task 7: Tauri IPC Commands

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create commands.rs**

Create `src-tauri/src/commands.rs`:

```rust
use crate::models::{Schedule, Action, TaskDto, ExecutionLog};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskDto>, String> {
    state.store.list_tasks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_task(id: String, state: State<'_, AppState>) -> Result<Option<TaskDto>, String> {
    state.store.get_task(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_task(
    name: String,
    description: Option<String>,
    schedule: Schedule,
    action: Action,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    let task = state.store.create_task(name, description, schedule.clone(), action.clone())
        .await.map_err(|e| e.to_string())?;

    // Schedule it immediately
    state.scheduler.schedule_task(&task).await.map_err(|e| e.to_string())?;

    Ok(task)
}

#[tauri::command]
pub async fn update_task(
    id: String,
    name: Option<String>,
    description: Option<Option<String>>,
    enabled: Option<bool>,
    schedule: Option<Schedule>,
    action: Option<Action>,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    state.store.update_task(&id, name, description, enabled, schedule, action)
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_task(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.scheduler.remove_task(&id).await.map_err(|e| e.to_string())?;
    state.store.delete_task(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_task_now(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let task = state.store.get_task(&id).await.map_err(|e| e.to_string())?
        .ok_or_else(|| "Task not found".to_string())?;

    let executor = crate::executor::current_executor();
    let result = executor.execute(&task.action).await;

    let (status, stdout, stderr, error) = match result {
        Ok(r) => ("success", r.stdout, r.stderr, None),
        Err(e) => ("failure", None, None, Some(e.to_string())),
    };

    state.store.log_execution(&id, status, stdout, stderr, error)
        .await.map_err(|e| e.to_string())?;
    state.store.update_last_run(&id, None)
        .await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn list_logs(
    task_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ExecutionLog>, String> {
    state.store.list_logs(task_id.as_deref(), limit.unwrap_or(50))
        .await.map_err(|e| e.to_string())
}
```

**Step 2: Register commands in lib.rs**

In `invoke_handler`:
```rust
.invoke_handler(tauri::generate_handler![
    commands::list_tasks,
    commands::get_task,
    commands::create_task,
    commands::update_task,
    commands::delete_task,
    commands::run_task_now,
    commands::list_logs,
])
```

Add `mod commands;` to lib.rs.

**Step 3: Build to verify compilation**

```bash
cd src-tauri && cargo build 2>&1
```
Expected: no errors.

**Step 4: Commit**

```bash
git add src-tauri/
git commit -m "feat: add IPC command handlers for task CRUD and execution"
```

---

## Task 8: React — Tauri API Bindings and Query Setup

**Files:**
- Create: `src/lib/api.ts`
- Create: `src/lib/queryClient.ts`
- Modify: `src/main.tsx`

**Step 1: Create Tauri API wrapper**

Create `src/lib/api.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";

export interface Schedule {
  type: "Cron" | "OneShot" | "OnLogin" | "OnWake";
  expression?: string;  // for Cron
  run_at?: string;      // for OneShot
}

export interface Action {
  type: "OpenFile" | "OpenUrl" | "RunCommand" | "Notify" | "Shortcut" | "Webhook";
  // OpenFile
  path?: string;
  // OpenUrl
  url?: string;
  browser?: string;
  // RunCommand
  command?: string;
  args?: string[];
  shell?: "Sh" | "Bash" | "Zsh" | "Python" | "AppleScript";
  // Notify
  title?: string;
  body?: string;
  sound?: boolean;
  // Shortcut
  keys?: string[];
  // Webhook
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  headers?: Record<string, string>;
}

export interface Task {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  schedule: Schedule;
  action: Action;
  created_at: string;
  updated_at: string;
  last_run_at?: string;
  next_run_at?: string;
}

export interface ExecutionLog {
  id: string;
  task_id: string;
  started_at: string;
  finished_at: string;
  status: "success" | "failure" | "skipped";
  stdout?: string;
  stderr?: string;
  error?: string;
}

export const api = {
  listTasks: () => invoke<Task[]>("list_tasks"),
  getTask: (id: string) => invoke<Task | null>("get_task", { id }),
  createTask: (params: { name: string; description?: string; schedule: Schedule; action: Action }) =>
    invoke<Task>("create_task", params),
  updateTask: (params: { id: string; name?: string; enabled?: boolean; schedule?: Schedule; action?: Action }) =>
    invoke<Task>("update_task", params),
  deleteTask: (id: string) => invoke<void>("delete_task", { id }),
  runTaskNow: (id: string) => invoke<void>("run_task_now", { id }),
  listLogs: (params?: { task_id?: string; limit?: number }) =>
    invoke<ExecutionLog[]>("list_logs", params ?? {}),
};
```

**Step 2: Create QueryClient**

Create `src/lib/queryClient.ts`:

```typescript
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5000,
      retry: 1,
    },
  },
});
```

**Step 3: Wrap app in QueryClientProvider**

In `src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/queryClient";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>
);
```

**Step 4: Commit**

```bash
git add src/
git commit -m "feat: add Tauri API bindings and React Query setup"
```

---

## Task 9: React — Menu Bar Popover (Main Window)

**Files:**
- Create: `src/components/TaskList.tsx`
- Create: `src/components/TaskItem.tsx`
- Modify: `src/App.tsx`
- Modify: `src/index.css`

**Step 1: Configure the main window as a popover in tauri.conf.json**

In `src-tauri/tauri.conf.json`, add the main window config:
```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "cronmac",
        "width": 280,
        "height": 400,
        "resizable": false,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "visible": false,
        "skipTaskbar": true
      }
    ]
  }
}
```

**Step 2: Create TaskItem component**

Create `src/components/TaskItem.tsx`:

```tsx
import { Task } from "../lib/api";
import { formatDistanceToNow } from "date-fns";

interface Props {
  task: Task;
  onToggle: (id: string, enabled: boolean) => void;
  onEdit: (task: Task) => void;
  onRun: (id: string) => void;
}

export function TaskItem({ task, onToggle, onEdit, onRun }: Props) {
  const nextRun = task.next_run_at
    ? formatDistanceToNow(new Date(task.next_run_at), { addSuffix: true })
    : null;

  return (
    <div className={`flex items-center gap-2 p-2 rounded-lg hover:bg-gray-100 group ${!task.enabled ? "opacity-50" : ""}`}>
      <button
        onClick={() => onToggle(task.id, !task.enabled)}
        className={`w-2 h-2 rounded-full flex-shrink-0 ${task.enabled ? "bg-green-500" : "bg-gray-300"}`}
      />
      <div className="flex-1 min-w-0" onClick={() => onEdit(task)}>
        <p className="text-sm font-medium truncate">{task.name}</p>
        {nextRun && <p className="text-xs text-gray-400">{nextRun}</p>}
      </div>
      <button
        onClick={() => onRun(task.id)}
        className="opacity-0 group-hover:opacity-100 text-xs text-gray-400 hover:text-gray-600 px-1"
        title="Run now"
      >
        ▶
      </button>
    </div>
  );
}
```

**Step 3: Create TaskList component**

Create `src/components/TaskList.tsx`:

```tsx
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api, Task } from "../lib/api";
import { TaskItem } from "./TaskItem";

interface Props {
  onNewTask: () => void;
  onEditTask: (task: Task) => void;
  onShowHistory: () => void;
}

export function TaskList({ onNewTask, onEditTask, onShowHistory }: Props) {
  const qc = useQueryClient();
  const { data: tasks = [], isLoading } = useQuery({
    queryKey: ["tasks"],
    queryFn: api.listTasks,
  });

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.updateTask({ id, enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tasks"] }),
  });

  const runMutation = useMutation({
    mutationFn: (id: string) => api.runTaskNow(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tasks"] }),
  });

  return (
    <div className="flex flex-col h-full bg-white/95 backdrop-blur-sm rounded-xl shadow-2xl border border-gray-200 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-100">
        <span className="text-sm font-semibold text-gray-700">cronmac</span>
        <button
          onClick={onNewTask}
          className="text-xs bg-blue-500 text-white px-2 py-1 rounded-md hover:bg-blue-600"
        >
          + New
        </button>
      </div>

      {/* Task list */}
      <div className="flex-1 overflow-y-auto px-2 py-1">
        {isLoading ? (
          <p className="text-xs text-gray-400 p-2">Loading...</p>
        ) : tasks.length === 0 ? (
          <p className="text-xs text-gray-400 p-2 text-center">
            No tasks yet. Click + New to create one.
          </p>
        ) : (
          tasks.map((task) => (
            <TaskItem
              key={task.id}
              task={task}
              onToggle={(id, enabled) => toggleMutation.mutate({ id, enabled })}
              onEdit={onEditTask}
              onRun={(id) => runMutation.mutate(id)}
            />
          ))
        )}
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between px-3 py-2 border-t border-gray-100">
        <button
          onClick={onShowHistory}
          className="text-xs text-gray-500 hover:text-gray-700"
        >
          History
        </button>
        <button className="text-xs text-gray-500 hover:text-gray-700">
          Settings
        </button>
      </div>
    </div>
  );
}
```

**Step 4: Wire up App.tsx**

Replace `src/App.tsx`:

```tsx
import { useState } from "react";
import { Task } from "./lib/api";
import { TaskList } from "./components/TaskList";

type View = "list" | "new" | "edit" | "history";

export default function App() {
  const [view, setView] = useState<View>("list");
  const [editingTask, setEditingTask] = useState<Task | null>(null);

  return (
    <div className="w-full h-screen">
      {view === "list" && (
        <TaskList
          onNewTask={() => setView("new")}
          onEditTask={(task) => { setEditingTask(task); setView("edit"); }}
          onShowHistory={() => setView("history")}
        />
      )}
      {/* Other views added in subsequent tasks */}
    </div>
  );
}
```

**Step 5: Commit**

```bash
git add src/ src-tauri/
git commit -m "feat: add menu bar popover with task list UI"
```

---

## Task 10: React — Create/Edit Task Wizard

**Files:**
- Create: `src/components/TaskWizard.tsx`
- Create: `src/components/ScheduleBuilder.tsx`
- Create: `src/components/ActionBuilder.tsx`

**Step 1: Create ScheduleBuilder**

Create `src/components/ScheduleBuilder.tsx`:

```tsx
import { Schedule } from "../lib/api";

interface Props {
  value: Schedule;
  onChange: (s: Schedule) => void;
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function ScheduleBuilder({ value, onChange }: Props) {
  return (
    <div className="space-y-3">
      {/* Schedule type selector */}
      <div className="flex gap-2">
        {(["Cron", "OneShot", "OnLogin", "OnWake"] as const).map((type) => (
          <button
            key={type}
            onClick={() => onChange({ type })}
            className={`px-2 py-1 text-xs rounded-md border ${
              value.type === type
                ? "bg-blue-500 text-white border-blue-500"
                : "border-gray-200 text-gray-600 hover:border-gray-300"
            }`}
          >
            {type === "Cron" ? "Recurring" : type === "OneShot" ? "One-time" : type}
          </button>
        ))}
      </div>

      {/* Cron: visual day/time picker */}
      {value.type === "Cron" && (
        <div className="space-y-2">
          <div className="flex gap-1">
            {DAYS.map((day, i) => {
              const selected = value.expression?.includes(`${i}`) ?? false;
              return (
                <button
                  key={day}
                  className={`flex-1 py-1 text-xs rounded ${
                    selected ? "bg-blue-500 text-white" : "bg-gray-100 text-gray-600"
                  }`}
                  onClick={() => {
                    // Toggle day in cron expression
                    // Simplified: build "0 HH * * D,D,D" expression
                    const expr = value.expression ?? "0 9 * * 1-5";
                    onChange({ ...value, expression: expr }); // simplified — full impl in wizard
                  }}
                >
                  {day}
                </button>
              );
            })}
          </div>
          <input
            type="time"
            className="w-full border rounded-md px-2 py-1 text-sm"
            onChange={(e) => {
              const [h, m] = e.target.value.split(":");
              const expr = `${m} ${h} * * 1-5`; // simplified weekdays
              onChange({ type: "Cron", expression: expr });
            }}
          />
        </div>
      )}

      {/* OneShot: date/time picker */}
      {value.type === "OneShot" && (
        <input
          type="datetime-local"
          className="w-full border rounded-md px-2 py-1 text-sm"
          onChange={(e) => onChange({ type: "OneShot", run_at: new Date(e.target.value).toISOString() })}
        />
      )}

      {value.type === "OnLogin" && (
        <p className="text-xs text-gray-500">Runs every time cronmac starts (at login)</p>
      )}
      {value.type === "OnWake" && (
        <p className="text-xs text-gray-500">Runs every time the Mac wakes from sleep</p>
      )}
    </div>
  );
}
```

**Step 2: Create ActionBuilder**

Create `src/components/ActionBuilder.tsx`:

```tsx
import { Action } from "../lib/api";

interface Props {
  value: Action;
  onChange: (a: Action) => void;
}

const ACTION_TYPES = [
  { type: "OpenFile", label: "Open File/App", icon: "📂" },
  { type: "OpenUrl", label: "Open URL", icon: "🌐" },
  { type: "RunCommand", label: "Run Script", icon: "⚡" },
  { type: "Notify", label: "Notification", icon: "🔔" },
  { type: "Shortcut", label: "Keyboard Shortcut", icon: "⌨️" },
  { type: "Webhook", label: "Webhook", icon: "🔗" },
] as const;

export function ActionBuilder({ value, onChange }: Props) {
  return (
    <div className="space-y-3">
      {/* Action type grid */}
      <div className="grid grid-cols-3 gap-2">
        {ACTION_TYPES.map(({ type, label, icon }) => (
          <button
            key={type}
            onClick={() => onChange({ type } as Action)}
            className={`p-2 rounded-lg border text-center text-xs ${
              value.type === type
                ? "bg-blue-50 border-blue-500 text-blue-700"
                : "border-gray-200 text-gray-600 hover:border-gray-300"
            }`}
          >
            <div className="text-lg">{icon}</div>
            <div>{label}</div>
          </button>
        ))}
      </div>

      {/* Dynamic fields per action type */}
      {value.type === "OpenFile" && (
        <input
          type="text"
          placeholder="File or app path (e.g. /Applications/Spotify.app)"
          className="w-full border rounded-md px-2 py-1 text-sm"
          value={value.path ?? ""}
          onChange={(e) => onChange({ ...value, path: e.target.value })}
        />
      )}

      {value.type === "OpenUrl" && (
        <input
          type="url"
          placeholder="https://example.com"
          className="w-full border rounded-md px-2 py-1 text-sm"
          value={value.url ?? ""}
          onChange={(e) => onChange({ ...value, url: e.target.value })}
        />
      )}

      {value.type === "RunCommand" && (
        <div className="space-y-2">
          <select
            className="w-full border rounded-md px-2 py-1 text-sm"
            value={value.shell ?? "Sh"}
            onChange={(e) => onChange({ ...value, shell: e.target.value as Action["shell"] })}
          >
            <option value="Sh">sh</option>
            <option value="Bash">bash</option>
            <option value="Zsh">zsh</option>
            <option value="Python">python3</option>
            <option value="AppleScript">AppleScript</option>
          </select>
          <textarea
            placeholder="Enter command or script..."
            rows={4}
            className="w-full border rounded-md px-2 py-1 text-sm font-mono"
            value={value.command ?? ""}
            onChange={(e) => onChange({ ...value, command: e.target.value, args: [] })}
          />
        </div>
      )}

      {value.type === "Notify" && (
        <div className="space-y-2">
          <input
            type="text"
            placeholder="Title"
            className="w-full border rounded-md px-2 py-1 text-sm"
            value={value.title ?? ""}
            onChange={(e) => onChange({ ...value, title: e.target.value })}
          />
          <textarea
            placeholder="Body"
            rows={2}
            className="w-full border rounded-md px-2 py-1 text-sm"
            value={value.body ?? ""}
            onChange={(e) => onChange({ ...value, body: e.target.value })}
          />
        </div>
      )}

      {value.type === "Webhook" && (
        <div className="space-y-2">
          <div className="flex gap-2">
            <select
              className="border rounded-md px-2 py-1 text-sm w-24"
              value={value.method ?? "POST"}
              onChange={(e) => onChange({ ...value, method: e.target.value as Action["method"] })}
            >
              {["GET", "POST", "PUT", "PATCH", "DELETE"].map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
            <input
              type="url"
              placeholder="https://hooks.example.com/..."
              className="flex-1 border rounded-md px-2 py-1 text-sm"
              value={value.url ?? ""}
              onChange={(e) => onChange({ ...value, url: e.target.value })}
            />
          </div>
          <textarea
            placeholder="Request body (optional, JSON)"
            rows={3}
            className="w-full border rounded-md px-2 py-1 text-sm font-mono"
            value={value.body ?? ""}
            onChange={(e) => onChange({ ...value, body: e.target.value })}
          />
        </div>
      )}
    </div>
  );
}
```

**Step 3: Create TaskWizard**

Create `src/components/TaskWizard.tsx`:

```tsx
import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, Task, Schedule, Action } from "../lib/api";
import { ScheduleBuilder } from "./ScheduleBuilder";
import { ActionBuilder } from "./ActionBuilder";

interface Props {
  task?: Task;         // if editing existing
  onDone: () => void;
  onCancel: () => void;
}

type Step = 1 | 2 | 3;

export function TaskWizard({ task, onDone, onCancel }: Props) {
  const qc = useQueryClient();
  const [step, setStep] = useState<Step>(1);
  const [name, setName] = useState(task?.name ?? "");
  const [schedule, setSchedule] = useState<Schedule>(
    task?.schedule ?? { type: "Cron", expression: "0 9 * * 1-5" }
  );
  const [action, setAction] = useState<Action>(
    task?.action ?? { type: "OpenUrl", url: "" }
  );

  const createMutation = useMutation({
    mutationFn: () => api.createTask({ name, schedule, action }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["tasks"] }); onDone(); },
  });

  const updateMutation = useMutation({
    mutationFn: () => api.updateTask({ id: task!.id, name, schedule, action }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["tasks"] }); onDone(); },
  });

  const isEdit = !!task;
  const canProceed = step === 1 ? name.trim().length > 0 : step === 2 ? true : true;

  return (
    <div className="flex flex-col h-full bg-white rounded-xl shadow-2xl border border-gray-200 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-100">
        <h2 className="text-sm font-semibold">{isEdit ? "Edit Task" : "New Task"}</h2>
        <button onClick={onCancel} className="text-gray-400 hover:text-gray-600 text-lg leading-none">×</button>
      </div>

      {/* Step indicator */}
      <div className="flex px-4 pt-3 gap-2">
        {([1, 2, 3] as Step[]).map((s) => (
          <div
            key={s}
            className={`flex-1 h-1 rounded-full ${s <= step ? "bg-blue-500" : "bg-gray-200"}`}
          />
        ))}
      </div>

      {/* Step content */}
      <div className="flex-1 overflow-y-auto px-4 py-3">
        {step === 1 && (
          <div className="space-y-3">
            <p className="text-xs font-medium text-gray-500 uppercase tracking-wide">Schedule</p>
            <input
              type="text"
              placeholder="Task name..."
              autoFocus
              className="w-full border rounded-md px-3 py-2 text-sm"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <ScheduleBuilder value={schedule} onChange={setSchedule} />
          </div>
        )}

        {step === 2 && (
          <div className="space-y-3">
            <p className="text-xs font-medium text-gray-500 uppercase tracking-wide">Action</p>
            <ActionBuilder value={action} onChange={setAction} />
          </div>
        )}

        {step === 3 && (
          <div className="space-y-3">
            <p className="text-xs font-medium text-gray-500 uppercase tracking-wide">Review</p>
            <div className="bg-gray-50 rounded-lg p-3 space-y-2 text-sm">
              <p><span className="text-gray-500">Name:</span> {name}</p>
              <p><span className="text-gray-500">Schedule:</span> {JSON.stringify(schedule)}</p>
              <p><span className="text-gray-500">Action:</span> {action.type}</p>
            </div>
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="flex gap-2 px-4 py-3 border-t border-gray-100">
        {step > 1 && (
          <button
            onClick={() => setStep((s) => (s - 1) as Step)}
            className="flex-1 py-2 text-sm border rounded-md hover:bg-gray-50"
          >
            Back
          </button>
        )}
        {step < 3 ? (
          <button
            disabled={!canProceed}
            onClick={() => setStep((s) => (s + 1) as Step)}
            className="flex-1 py-2 text-sm bg-blue-500 text-white rounded-md hover:bg-blue-600 disabled:opacity-50"
          >
            Next
          </button>
        ) : (
          <button
            onClick={() => isEdit ? updateMutation.mutate() : createMutation.mutate()}
            className="flex-1 py-2 text-sm bg-blue-500 text-white rounded-md hover:bg-blue-600"
          >
            {isEdit ? "Save" : "Create"}
          </button>
        )}
      </div>
    </div>
  );
}
```

**Step 4: Wire TaskWizard into App.tsx**

Update `src/App.tsx` to render `TaskWizard` for "new" and "edit" views, passing the appropriate props.

**Step 5: Commit**

```bash
git add src/
git commit -m "feat: add task creation/edit wizard with schedule and action builders"
```

---

## Task 11: React — Execution History View

**Files:**
- Create: `src/components/HistoryView.tsx`

**Step 1: Create HistoryView**

Create `src/components/HistoryView.tsx`:

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, ExecutionLog } from "../lib/api";
import { formatDistanceToNow } from "date-fns";

interface Props {
  onBack: () => void;
}

export function HistoryView({ onBack }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);

  const { data: logs = [], isLoading } = useQuery({
    queryKey: ["logs"],
    queryFn: () => api.listLogs({ limit: 100 }),
  });

  return (
    <div className="flex flex-col h-full bg-white rounded-xl shadow-2xl border border-gray-200 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-100">
        <button onClick={onBack} className="text-gray-400 hover:text-gray-600">←</button>
        <span className="text-sm font-semibold">Execution History</span>
      </div>

      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <p className="text-xs text-gray-400 p-3">Loading...</p>
        ) : logs.length === 0 ? (
          <p className="text-xs text-gray-400 p-3 text-center">No executions yet</p>
        ) : (
          logs.map((log) => (
            <div key={log.id} className="border-b border-gray-50">
              <button
                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-gray-50 text-left"
                onClick={() => setExpanded(expanded === log.id ? null : log.id)}
              >
                <span className={`w-2 h-2 rounded-full flex-shrink-0 ${
                  log.status === "success" ? "bg-green-500" :
                  log.status === "failure" ? "bg-red-500" : "bg-yellow-400"
                }`} />
                <span className="text-xs flex-1 truncate text-gray-700">
                  {log.task_id.slice(0, 8)}...
                </span>
                <span className="text-xs text-gray-400">
                  {formatDistanceToNow(new Date(log.started_at), { addSuffix: true })}
                </span>
              </button>

              {expanded === log.id && (log.stdout || log.stderr || log.error) && (
                <div className="px-3 pb-2 text-xs font-mono bg-gray-50">
                  {log.stdout && <pre className="text-green-700 whitespace-pre-wrap">{log.stdout}</pre>}
                  {log.stderr && <pre className="text-yellow-700 whitespace-pre-wrap">{log.stderr}</pre>}
                  {log.error && <pre className="text-red-700 whitespace-pre-wrap">{log.error}</pre>}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
```

**Step 2: Wire into App.tsx**

In `App.tsx`, add rendering of `HistoryView` when `view === "history"`.

**Step 3: Commit**

```bash
git add src/
git commit -m "feat: add execution history view"
```

---

## Task 12: Launch Agent — Auto-start on Login

**Files:**
- Create: `src-tauri/src/launch_agent.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create launch_agent.rs**

Create `src-tauri/src/launch_agent.rs`:

```rust
use std::path::PathBuf;
use std::fs;

pub fn register() -> anyhow::Result<()> {
    let plist_path = plist_path();
    if plist_path.exists() {
        return Ok(()); // already registered
    }

    let app_path = std::env::current_exe()?;
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.cronmac.app</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
</dict>
</plist>"#,
        app_path.to_str().unwrap()
    );

    fs::create_dir_all(plist_path.parent().unwrap())?;
    fs::write(&plist_path, plist_content)?;

    // Load the agent
    std::process::Command::new("launchctl")
        .args(["load", plist_path.to_str().unwrap()])
        .output()?;

    Ok(())
}

pub fn unregister() -> anyhow::Result<()> {
    let plist_path = plist_path();
    if plist_path.exists() {
        std::process::Command::new("launchctl")
            .args(["unload", plist_path.to_str().unwrap()])
            .output()?;
        fs::remove_file(&plist_path)?;
    }
    Ok(())
}

pub fn is_registered() -> bool {
    plist_path().exists()
}

fn plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents/com.cronmac.app.plist")
}
```

**Step 2: Add Tauri commands for settings**

In `commands.rs`, add:
```rust
#[tauri::command]
pub fn get_launch_agent_status() -> bool {
    crate::launch_agent::is_registered()
}

#[tauri::command]
pub fn set_launch_agent(enabled: bool) -> Result<(), String> {
    if enabled {
        crate::launch_agent::register().map_err(|e| e.to_string())
    } else {
        crate::launch_agent::unregister().map_err(|e| e.to_string())
    }
}
```

Register in `invoke_handler`. Call `launch_agent::register()` in the `setup` callback on first run.

**Step 3: Commit**

```bash
git add src-tauri/
git commit -m "feat: add Launch Agent registration for auto-start on login"
```

---

## Task 13: Final Integration and E2E Smoke Test

**Step 1: Full build**

```bash
npm run tauri build 2>&1
```
Expected: `.app` bundle created in `src-tauri/target/release/bundle/macos/`.

**Step 2: Manual smoke test checklist**

1. Launch app — verify no Dock icon, only menu bar icon
2. Click tray icon — verify 280x400 popover appears and hides
3. Create a "Run command" task with `echo hello` and schedule "Every minute"
4. Wait 60s — verify task runs and appears in History with `stdout: hello`
5. Toggle task disabled — verify it stops running
6. Create a "One-shot" task — verify it fires once at the specified time
7. Quit and relaunch — verify tasks are loaded from SQLite

**Step 3: Run all Rust tests**

```bash
cd src-tauri && cargo test --all
```
Expected: all tests pass.

**Step 4: Tag release**

```bash
git tag v0.1.0
git commit --allow-empty -m "chore: v0.1.0 initial release"
```

---

## Summary

| # | Task | Key Tech |
|---|------|----------|
| 1 | Project scaffold | `npm create tauri-app`, Cargo.toml deps |
| 2 | Menu bar, no Dock | `activationPolicy: Accessory`, `TrayIconBuilder` |
| 3 | SQLite models + schema | `sqlx`, migrations, `Task`/`ExecutionLog` |
| 4 | `ActionExecutor` + macOS | `MacosExecutor`, `open`, `osascript`, `reqwest` |
| 5 | Task CRUD store | `TaskStore`, in-memory test DB |
| 6 | Scheduler | `tokio-cron-scheduler`, `AppScheduler` |
| 7 | IPC commands | `#[tauri::command]` handlers |
| 8 | API bindings + Query | `invoke`, `@tanstack/react-query` |
| 9 | Popover UI | `TaskList`, `TaskItem`, popover window config |
| 10 | Create/Edit wizard | `TaskWizard`, `ScheduleBuilder`, `ActionBuilder` |
| 11 | History view | `HistoryView`, expandable log entries |
| 12 | Launch Agent | `launchctl`, plist generation |
| 13 | E2E + release | smoke test, `cargo test`, tag |
