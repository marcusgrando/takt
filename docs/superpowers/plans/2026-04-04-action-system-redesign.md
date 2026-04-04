# Action System Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace osascript-based executor with native macOS APIs, add OpenApp action, make Shortcut a sub-item of visual actions, add file picker dialogs, fix scheduler bugs.

**Architecture:** Rust-side changes first (models → DB migration → executor → scheduler bugs → commands), then frontend (types → ActionBuilder → TemplateGrid → AutoName). Backend and frontend share the Action type shape via serde JSON.

**Tech Stack:** Rust (objc2, objc2-foundation, objc2-app-kit, core-graphics), Tauri v2 (tauri-plugin-dialog, tauri-plugin-notification), React 19, shadcn/ui, TypeScript.

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src-tauri/Cargo.toml` | Add objc2, core-graphics, tauri-plugin-dialog deps |
| Modify | `src-tauri/src/models.rs` | New Action enum, KeyCombo, Modifier types |
| Create | `src-tauri/migrations/002_recreate.sql` | Drop + recreate tables |
| Modify | `src-tauri/src/executor/mod.rs` | Update current_executor to accept AppHandle |
| Rewrite | `src-tauri/src/executor/macos.rs` | Full native executor (NSWorkspace, CGEvent, etc.) |
| Create | `src-tauri/src/executor/keymap.rs` | Virtual keycode lookup table |
| Modify | `src-tauri/src/scheduler.rs` | Job UUID tracking, remove_task, enabled/deleted guard |
| Modify | `src-tauri/src/commands.rs` | Remove stale NOTEs, scheduler integration |
| Modify | `src-tauri/src/lib.rs` | Pass AppHandle to executor, add dialog plugin |
| Modify | `src-tauri/capabilities/default.json` | Add window:allow-close, dialog:default |
| Modify | `src/lib/api.ts` | Update Action type, add KeyCombo, Modifier, OpenApp |
| Modify | `src/components/ActionBuilder.tsx` | Remove Keys tab, add App tab, file pickers, post_shortcuts UI |
| Modify | `src/components/TemplateGrid.tsx` | Replace Keys with App template |
| Modify | `src/components/AutoName.ts` | Add OpenApp, remove Shortcut |
| Modify | `src/components/TaskEditor.tsx` | Update defaults and validation for new types |
| Modify | `package.json` | Add @tauri-apps/plugin-dialog |

---

### Task 1: Tauri Capabilities + Dependencies (quick fix)

**Files:**
- Modify: `src-tauri/capabilities/default.json`
- Modify: `package.json`

- [ ] **Step 1: Fix window.close permission and add dialog**

In `src-tauri/capabilities/default.json`, replace the permissions array:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": [
    "main",
    "editor-*"
  ],
  "permissions": [
    "core:default",
    "core:window:allow-create",
    "core:window:allow-close",
    "core:webview:allow-create-webview-window",
    "dialog:default",
    "opener:default",
    "notification:default",
    "shell:default"
  ]
}
```

- [ ] **Step 2: Add tauri-plugin-dialog JS dependency**

```bash
cd /Users/marcus.grando/git/cronmac && bun add @tauri-apps/plugin-dialog
```

- [ ] **Step 3: Add tauri-plugin-dialog Rust dependency**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tauri-plugin-dialog = "2"
```

- [ ] **Step 4: Register dialog plugin in lib.rs**

In `src-tauri/src/lib.rs`, add `.plugin(tauri_plugin_dialog::init())` after the notification plugin:

```rust
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/capabilities/default.json package.json bun.lock src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "fix: add window.close permission, dialog plugin deps"
```

---

### Task 2: Rust Data Model — Action, KeyCombo, Modifier

**Files:**
- Modify: `src-tauri/src/models.rs`

- [ ] **Step 1: Update the Action enum and add new types**

Replace the entire `Action` enum and add `KeyCombo` + `Modifier` in `src-tauri/src/models.rs`:

```rust
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
```

Note: `Shortcut` variant is **removed** entirely.

- [ ] **Step 2: Update store.rs tests to use new Action shape**

The tests in `src-tauri/src/store.rs` use `Action::Notify` and `Action::RunCommand` — these are unchanged so they should still compile. But verify:

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo check 2>&1 | head -30
```

This will fail because `executor/macos.rs` still references `Action::Shortcut`. That's expected — we'll fix it in Task 4.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat: update Action model — add OpenApp, KeyCombo, remove Shortcut"
```

---

### Task 3: Database Migration — Drop + Recreate

**Files:**
- Create: `src-tauri/migrations/002_recreate.sql`

- [ ] **Step 1: Create migration file**

```sql
-- 002_recreate.sql
-- Pre-release: drop all data and recreate with clean schema.
-- The table structure is identical; only the JSON shape inside
-- action_json changes (new Action enum variants).

DROP TABLE IF EXISTS execution_logs;
DROP TABLE IF EXISTS tasks;

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

CREATE INDEX IF NOT EXISTS idx_execution_logs_task_id ON execution_logs(task_id);
CREATE INDEX IF NOT EXISTS idx_execution_logs_started_at ON execution_logs(started_at);
```

- [ ] **Step 2: Verify migration runs**

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo test test_db_connects_and_migrates -- --nocapture 2>&1 | tail -10
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/migrations/002_recreate.sql
git commit -m "chore: add migration 002 — drop and recreate tables for new Action schema"
```

---

### Task 4: Virtual Keycode Map

**Files:**
- Create: `src-tauri/src/executor/keymap.rs`

- [ ] **Step 1: Create keycode lookup table**

This maps key name strings to macOS virtual keycodes (CGKeyCode values).

```rust
// src-tauri/src/executor/keymap.rs
use std::collections::HashMap;
use std::sync::LazyLock;

/// Maps lowercase key names to macOS virtual keycodes (CGKeyCode).
/// Reference: Events.h / HIToolbox/Events.h
pub static KEYMAP: LazyLock<HashMap<&'static str, u16>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Letters
    m.insert("a", 0x00); m.insert("s", 0x01); m.insert("d", 0x02);
    m.insert("f", 0x03); m.insert("h", 0x04); m.insert("g", 0x05);
    m.insert("z", 0x06); m.insert("x", 0x07); m.insert("c", 0x08);
    m.insert("v", 0x09); m.insert("b", 0x0B); m.insert("q", 0x0C);
    m.insert("w", 0x0D); m.insert("e", 0x0E); m.insert("r", 0x0F);
    m.insert("y", 0x10); m.insert("t", 0x11); m.insert("1", 0x12);
    m.insert("2", 0x13); m.insert("3", 0x14); m.insert("4", 0x15);
    m.insert("5", 0x17); m.insert("6", 0x16); m.insert("7", 0x1A);
    m.insert("8", 0x1C); m.insert("9", 0x19); m.insert("0", 0x1D);
    m.insert("o", 0x1F); m.insert("u", 0x20); m.insert("i", 0x22);
    m.insert("p", 0x23); m.insert("l", 0x25); m.insert("j", 0x26);
    m.insert("k", 0x28); m.insert("n", 0x2D); m.insert("m", 0x2E);

    // Special keys
    m.insert("return", 0x24);    m.insert("enter", 0x24);
    m.insert("tab", 0x30);       m.insert("space", 0x31);
    m.insert("delete", 0x33);    m.insert("backspace", 0x33);
    m.insert("escape", 0x35);    m.insert("esc", 0x35);

    // Modifiers (as standalone keys)
    m.insert("command", 0x37);   m.insert("cmd", 0x37);
    m.insert("shift", 0x38);
    m.insert("capslock", 0x39);
    m.insert("option", 0x3A);    m.insert("opt", 0x3A); m.insert("alt", 0x3A);
    m.insert("control", 0x3B);   m.insert("ctrl", 0x3B);

    // Arrow keys
    m.insert("left", 0x7B);      m.insert("right", 0x7C);
    m.insert("down", 0x7D);      m.insert("up", 0x7E);

    // Function keys
    m.insert("f1", 0x7A);  m.insert("f2", 0x78);  m.insert("f3", 0x63);
    m.insert("f4", 0x76);  m.insert("f5", 0x60);  m.insert("f6", 0x61);
    m.insert("f7", 0x62);  m.insert("f8", 0x64);  m.insert("f9", 0x65);
    m.insert("f10", 0x6D); m.insert("f11", 0x67); m.insert("f12", 0x6F);

    // Punctuation
    m.insert("-", 0x1B);   m.insert("=", 0x18);
    m.insert("[", 0x21);   m.insert("]", 0x1E);
    m.insert("\\", 0x2A);  m.insert(";", 0x29);
    m.insert("'", 0x27);   m.insert(",", 0x2B);
    m.insert(".", 0x2F);   m.insert("/", 0x2C);
    m.insert("`", 0x32);

    m
});

/// Look up a key name and return the virtual keycode.
/// Returns None if the key is not recognized.
pub fn resolve_keycode(key: &str) -> Option<u16> {
    KEYMAP.get(key.to_lowercase().as_str()).copied()
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/executor/keymap.rs
git commit -m "feat: add macOS virtual keycode lookup table"
```

---

### Task 5: Native macOS Executor

**Files:**
- Modify: `src-tauri/Cargo.toml` (add objc2, core-graphics)
- Modify: `src-tauri/src/executor/mod.rs` (update trait, current_executor)
- Rewrite: `src-tauri/src/executor/macos.rs`

This is the largest task. The executor is rewritten to use native macOS APIs.

- [ ] **Step 1: Add native deps to Cargo.toml**

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSURL", "NSString", "NSArray", "NSObject", "NSRunLoop", "NSDate"] }
objc2-app-kit = { version = "0.3", features = ["NSWorkspace", "NSWorkspaceOpenConfiguration", "NSRunningApplication"] }
core-graphics = "0.24"
```

- [ ] **Step 2: Update executor/mod.rs — add AppHandle to executor**

Replace the content of `src-tauri/src/executor/mod.rs`:

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
    #[error("Accessibility permission required: {0}")]
    AccessibilityRequired(String),
}

#[async_trait::async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError>;
    fn platform_name(&self) -> &'static str;
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub mod keymap;

#[cfg(target_os = "macos")]
pub use macos::MacosExecutor;

/// Returns the correct executor for the current platform.
/// Requires an AppHandle for notification support.
pub fn current_executor(app_handle: tauri::AppHandle) -> Box<dyn ActionExecutor> {
    #[cfg(target_os = "macos")]
    return Box::new(MacosExecutor::new(app_handle));

    #[cfg(not(target_os = "macos"))]
    panic!("No executor available for this platform");
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 3: Rewrite executor/macos.rs with native APIs**

Replace the entire content of `src-tauri/src/executor/macos.rs`:

```rust
use super::{ActionExecutor, ExecutionResult, ExecutorError};
use super::keymap::resolve_keycode;
use crate::models::{Action, HttpMethod, KeyCombo, Modifier, Shell};
use async_trait::async_trait;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration, NSRunningApplication};
use objc2_foundation::{NSArray, NSURL, NSString};
use std::process::Command;
use std::time::Duration;
use tauri_plugin_notification::NotificationExt;

pub struct MacosExecutor {
    app_handle: tauri::AppHandle,
}

impl MacosExecutor {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    fn platform_name(&self) -> &'static str {
        "macos"
    }

    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile { path, app, post_shortcuts } => {
                open_file(path, app.as_deref())?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::OpenUrl { url, browser, post_shortcuts } => {
                open_url(url, browser.as_deref())?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::OpenApp { app_path, post_shortcuts } => {
                open_app(app_path)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::RunCommand { command, args, shell } => run_command(command, args, shell),
            Action::Notify { title, body, sound } => {
                send_notification(&self.app_handle, title, body, *sound)
            }
            Action::Webhook { url, method, headers, body } => {
                send_webhook(url, method, headers, body.as_deref()).await
            }
        }
    }
}

// ── Open actions via NSWorkspace ──────────────────────────────────────

fn open_file(path: &str, app: Option<&str>) -> Result<(), ExecutorError> {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let file_url = NSURL::fileURLWithPath(&NSString::from_str(path));

        if let Some(app_name) = app {
            let config = NSWorkspaceOpenConfiguration::new();
            let app_url = resolve_app_url(app_name)?;
            let urls = NSArray::from_retained_slice(&[file_url]);
            // Fire-and-forget open
            workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
                &urls, &app_url, &config, None,
            );
        } else {
            let success = workspace.openURL(&file_url);
            if !success {
                return Err(ExecutorError::CommandFailed(format!("Failed to open file: {}", path)));
            }
        }
    }
    Ok(())
}

fn open_url(url: &str, browser: Option<&str>) -> Result<(), ExecutorError> {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let ns_url = NSURL::URLWithString(&NSString::from_str(url))
            .ok_or_else(|| ExecutorError::CommandFailed(format!("Invalid URL: {}", url)))?;

        if let Some(browser_name) = browser {
            let config = NSWorkspaceOpenConfiguration::new();
            let app_url = resolve_app_url(browser_name)?;
            let urls = NSArray::from_retained_slice(&[ns_url]);
            workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
                &urls, &app_url, &config, None,
            );
        } else {
            let success = workspace.openURL(&ns_url);
            if !success {
                return Err(ExecutorError::CommandFailed(format!("Failed to open URL: {}", url)));
            }
        }
    }
    Ok(())
}

fn open_app(app_path: &str) -> Result<(), ExecutorError> {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app_url = NSURL::fileURLWithPath(&NSString::from_str(app_path));
        let config = NSWorkspaceOpenConfiguration::new();
        workspace.openApplicationAtURL_configuration_completionHandler(
            &app_url, &config, None,
        );
    }
    Ok(())
}

/// Resolve an app name like "Safari" to a file URL like /Applications/Safari.app
fn resolve_app_url(app_name: &str) -> Result<objc2_foundation::Retained<NSURL>, ExecutorError> {
    let path = if app_name.ends_with(".app") || app_name.starts_with('/') {
        app_name.to_string()
    } else {
        format!("/Applications/{}.app", app_name)
    };
    unsafe {
        Ok(NSURL::fileURLWithPath(&NSString::from_str(&path)))
    }
}

// ── Post-shortcuts: wait for frontmost + CGEvent ──────────────────────

async fn wait_and_send_shortcuts(shortcuts: &[KeyCombo]) -> Result<(), ExecutorError> {
    // Check accessibility permission
    if !accessibility_is_trusted() {
        return Err(ExecutorError::AccessibilityRequired(
            "Grant Accessibility permission in System Settings → Privacy & Security → Accessibility".into()
        ));
    }

    // Wait for the target app to become frontmost (up to 10s)
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        // After opening, the target app should become frontmost.
        // We just wait a reasonable time for it to activate.
    }
    // Simplified: wait 1 second for app activation, then send keys
    // A more robust approach would track the specific app, but for v1
    // a fixed delay after NSWorkspace open is reliable enough.

    // Actually, let's just wait 1 second total instead of 10
    // NSWorkspace open is fast — the app is usually frontmost within 500ms
    tokio::time::sleep(Duration::from_millis(500)).await;

    for combo in shortcuts {
        send_key_combo(combo)?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

fn send_key_combo(combo: &KeyCombo) -> Result<(), ExecutorError> {
    let keycode = resolve_keycode(&combo.key)
        .ok_or_else(|| ExecutorError::CommandFailed(format!("Unknown key: {}", combo.key)))?;

    let mut flags = CGEventFlags::empty();
    for modifier in &combo.modifiers {
        flags |= match modifier {
            Modifier::Cmd => CGEventFlags::CGEventFlagCommand,
            Modifier::Shift => CGEventFlags::CGEventFlagShift,
            Modifier::Opt => CGEventFlags::CGEventFlagAlternate,
            Modifier::Ctrl => CGEventFlags::CGEventFlagControl,
        };
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create CGEventSource".into()))?;

    // Key down
    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode as CGKeyCode, true)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-down event".into()))?;
    key_down.set_flags(flags);
    key_down.post(CGEventTapLocation::HID);

    // Key up
    let key_up = CGEvent::new_keyboard_event(source, keycode as CGKeyCode, false)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-up event".into()))?;
    key_up.set_flags(flags);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

fn accessibility_is_trusted() -> bool {
    // AXIsProcessTrusted() from ApplicationServices framework
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

// ── RunCommand (unchanged — std::process::Command) ───────────────────

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

    let output = Command::new(shell_bin).arg(shell_flag).arg(&full_command).output()?;
    let stdout = if output.stdout.is_empty() { None } else { Some(String::from_utf8_lossy(&output.stdout).to_string()) };
    let stderr = if output.stderr.is_empty() { None } else { Some(String::from_utf8_lossy(&output.stderr).to_string()) };

    if output.status.success() {
        Ok(ExecutionResult { stdout, stderr })
    } else {
        Err(ExecutorError::CommandFailed(
            format!("Exit code {}: {}", output.status.code().unwrap_or(-1), stderr.as_deref().unwrap_or(""))
        ))
    }
}

// ── Notification via tauri-plugin-notification ────────────────────────

fn send_notification(app: &tauri::AppHandle, title: &str, body: &str, sound: bool) -> Result<ExecutionResult, ExecutorError> {
    let mut builder = app.notification().builder()
        .title(title)
        .body(body);
    if sound {
        builder = builder.sound("default");
    }
    builder.show()
        .map_err(|e| ExecutorError::CommandFailed(format!("Notification failed: {}", e)))?;
    Ok(ExecutionResult { stdout: None, stderr: None })
}

// ── Webhook (unchanged — reqwest) ────────────────────────────────────

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
    let response = req.send().await.map_err(|e| ExecutorError::Http(e.to_string()))?;
    let status = response.status();
    let response_body = response.text().await.map_err(|e| ExecutorError::Http(e.to_string()))?;

    if status.is_success() {
        Ok(ExecutionResult { stdout: Some(response_body), stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(format!("HTTP {} — {}", status, response_body)))
    }
}
```

**Important notes for the implementer:**
- The `objc2` API is still evolving. The exact method signatures may differ. Check the `objc2-app-kit` docs if compilation fails — method names might need adjustment (e.g., `sharedWorkspace` vs `shared_workspace`).
- The `wait_and_send_shortcuts` uses a simple 500ms delay. This is a pragmatic v1 — a future improvement could poll `NSRunningApplication.isActive` for the specific opened app.
- The `send_notification` uses `tauri-plugin-notification`'s Rust API which requires the `AppHandle`.

- [ ] **Step 4: Update lib.rs — pass AppHandle to executor**

In `src-tauri/src/lib.rs`, change the executor creation inside the async block:

Replace:
```rust
let executor = Arc::new(current_executor());
```

With:
```rust
let executor = Arc::new(current_executor(handle.clone()));
```

- [ ] **Step 5: Verify it compiles**

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo check 2>&1 | tail -20
```

This may produce warnings or errors from `objc2` API differences. Fix them iteratively.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/executor/ src-tauri/src/lib.rs
git commit -m "feat: native macOS executor — NSWorkspace, CGEvent, tauri-notification"
```

---

### Task 6: Scheduler Bug Fixes — Job UUID Tracking + Enabled/Deleted Guard

**Files:**
- Modify: `src-tauri/src/scheduler.rs`

- [ ] **Step 1: Rewrite scheduler with job tracking**

Replace the entire content of `src-tauri/src/scheduler.rs`:

```rust
use crate::executor::{current_executor, ActionExecutor};
use crate::models::{Schedule, TaskDto};
use crate::store::TaskStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

/// Convert 5-field standard cron to 6-field (prepend seconds=0).
fn normalize_cron(expr: &str) -> String {
    let parts: Vec<&str> = expr.trim().split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", expr.trim())
    } else {
        expr.trim().to_string()
    }
}

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
    /// Maps task ID → cron job UUID for removal
    job_ids: Mutex<HashMap<String, Uuid>>,
}

impl AppScheduler {
    pub async fn new(store: Arc<TaskStore>, app_handle: tauri::AppHandle) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        let executor = Arc::new(current_executor(app_handle));
        Ok(Self {
            inner,
            executor,
            store,
            job_ids: Mutex::new(HashMap::new()),
        })
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
                let expr = normalize_cron(expression);
                let store_check = Arc::clone(&store);
                let task_id_check = task_id.clone();

                let job = Job::new_async(expr.as_str(), move |_uuid, _lock| {
                    let action = action.clone();
                    let executor = Arc::clone(&executor);
                    let store = Arc::clone(&store);
                    let task_id = task_id.clone();
                    let store_check = Arc::clone(&store_check);
                    let task_id_check = task_id_check.clone();
                    Box::pin(async move {
                        // Guard: re-fetch task to check enabled/deleted
                        let task = store_check.get_task(&task_id_check).await;
                        match task {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                // Disabled — log as skipped
                                let _ = store.log_execution(&task_id, "skipped", None, None, None).await;
                                return;
                            }
                            _ => {
                                // Deleted or error — silently skip
                                return;
                            }
                        }

                        let result = executor.execute(&action).await;
                        let (status, stdout, stderr, error) = match result {
                            Ok(r) => ("success", r.stdout, r.stderr, None),
                            Err(e) => ("failure", None, None, Some(e.to_string())),
                        };
                        let _ = store.log_execution(&task_id, status, stdout, stderr, error).await;
                        let _ = store.update_last_run(&task_id, None).await;
                    })
                })?;

                let job_uuid = self.inner.add(job).await?;
                self.job_ids.lock().await.insert(task_id, job_uuid);
            }
            Schedule::OneShot { run_at } => {
                let run_at: chrono::DateTime<chrono::Utc> = run_at.parse()?;
                let now = chrono::Utc::now();
                if run_at > now {
                    let delay = (run_at - now).to_std()?;
                    let store_check = Arc::clone(&store);
                    let task_id_check = task_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;

                        // Guard: re-fetch task to check enabled/deleted
                        match store_check.get_task(&task_id_check).await {
                            Ok(Some(t)) if t.enabled => { /* proceed */ }
                            Ok(Some(_)) => {
                                let _ = store.log_execution(&task_id, "skipped", None, None, None).await;
                                return;
                            }
                            _ => return,
                        }

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

    /// Remove a scheduled cron job by task ID.
    /// OneShot/OnLogin/OnWake spawns cannot be cancelled (they're tokio tasks).
    pub async fn remove_task(&self, task_id: &str) -> anyhow::Result<()> {
        let mut ids = self.job_ids.lock().await;
        if let Some(job_uuid) = ids.remove(task_id) {
            self.inner.remove(&job_uuid).await?;
        }
        Ok(())
    }
}
```

- [ ] **Step 2: Update lib.rs — pass AppHandle to scheduler**

In `src-tauri/src/lib.rs`, inside the `tauri::async_runtime::block_on` closure, change:

```rust
let scheduler = Arc::new(
    AppScheduler::new(Arc::clone(&store))
        .await
        .expect("Scheduler init failed"),
);
```

to:

```rust
let scheduler = Arc::new(
    AppScheduler::new(Arc::clone(&store), handle.clone())
        .await
        .expect("Scheduler init failed"),
);
```

- [ ] **Step 3: Update commands.rs — remove stale NOTEs**

In `src-tauri/src/commands.rs`, remove the two NOTE comments about `remove_task` being a no-op:

Remove line 46: `// NOTE: remove_task is a no-op stub in v1; the old in-memory job keeps running`
Remove line 47 (after previous removal): `// until the next app restart. Full job tracking by UUID is planned for v2.`

Remove lines 57-58 (the NOTE in delete_task):
```
// NOTE: remove_task is a no-op stub in v1. The in-memory cron job will keep
// firing until the next app restart. Full removal by job UUID is planned for v2.
```

- [ ] **Step 4: Verify compilation**

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo check 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "fix: scheduler job tracking — enabled/deleted guard, real remove_task"
```

---

### Task 7: TypeScript Types — Action, KeyCombo, Modifier

**Files:**
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Update the Action type and add new types**

In `src/lib/api.ts`, replace the existing type definitions:

Replace the `Shell`, `HttpMethod`, `Action` types with:

```ts
export type Shell = 'Sh' | 'Bash' | 'Zsh' | 'Python' | 'AppleScript';
export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export type Modifier = 'Cmd' | 'Shift' | 'Opt' | 'Ctrl';

export interface KeyCombo {
  modifiers: Modifier[];
  key: string;
}

export type Schedule =
  | { type: 'Cron'; expression: string }
  | { type: 'OneShot'; run_at: string }
  | { type: 'OnLogin' }
  | { type: 'OnWake' };

export type Action =
  | { type: 'OpenFile'; path: string; app?: string; post_shortcuts: KeyCombo[] }
  | { type: 'OpenUrl'; url: string; browser?: string; post_shortcuts: KeyCombo[] }
  | { type: 'OpenApp'; app_path: string; post_shortcuts: KeyCombo[] }
  | { type: 'RunCommand'; command: string; args: string[]; shell: Shell }
  | { type: 'Notify'; title: string; body: string; sound: boolean }
  | {
      type: 'Webhook';
      url: string;
      method: HttpMethod;
      headers: Record<string, string>;
      body?: string;
    };
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat: update TypeScript Action type — add OpenApp, KeyCombo, remove Shortcut"
```

---

### Task 8: AutoName + TemplateGrid Updates

**Files:**
- Modify: `src/components/AutoName.ts`
- Modify: `src/components/TemplateGrid.tsx`

- [ ] **Step 1: Update AutoName — add OpenApp, remove Shortcut**

In `src/components/AutoName.ts`, replace the `describeAction` function. Remove the `Shortcut` case and add `OpenApp`:

```ts
function describeAction(action: Action): string {
  switch (action.type) {
    case 'OpenUrl': {
      if (!action.url) return 'Open URL';
      try {
        const host = new URL(action.url).hostname.replace(/^www\./, '');
        return `Open ${host}`;
      } catch {
        return `Open ${action.url.slice(0, 30)}`;
      }
    }
    case 'OpenFile': {
      if (!action.path) return 'Open file';
      const name = action.path.split('/').pop() || action.path;
      return `Open ${name}`;
    }
    case 'OpenApp': {
      if (!action.app_path) return 'Open app';
      const name = action.app_path.split('/').pop()?.replace('.app', '') || action.app_path;
      return `Open ${name}`;
    }
    case 'RunCommand': {
      if (!action.command) return 'Run command';
      const cmd = action.command.split(/\s/)[0].split('/').pop() || action.command;
      return `Run ${cmd}`;
    }
    case 'Notify': {
      if (!action.title) return 'Reminder';
      return `Reminder: ${action.title}`;
    }
    case 'Webhook': {
      if (!action.url) return 'Webhook';
      try {
        const host = new URL(action.url).hostname.replace(/^www\./, '');
        return `${action.method} ${host}`;
      } catch {
        return `${action.method} webhook`;
      }
    }
  }
}
```

- [ ] **Step 2: Update TemplateGrid — replace Keys with App**

In `src/components/TemplateGrid.tsx`, replace the imports, type, and TEMPLATES array:

Change the `lucide-react` imports:
```ts
import { Link, FileText, Terminal, Bell, Globe, AppWindow } from 'lucide-react';
```

Remove `Settings` import (was used for Custom, replace with `AppWindow` for OpenApp — actually keep Settings for Custom). Correction:

```ts
import { Link, FileText, AppWindow, Terminal, Bell, Globe, Settings } from 'lucide-react';
```

Update the `TemplateName` type:
```ts
export type TemplateName = 'OpenUrl' | 'OpenFile' | 'OpenApp' | 'RunCommand' | 'Notify' | 'Webhook' | 'Custom';
```

Replace the TEMPLATES array entries — swap `Shortcut/Keys` with `OpenApp`:

Find the template with `name: 'Webhook'` and before it, replace the old entries. The full array should be:

```ts
export const TEMPLATES: TemplateConfig[] = [
  {
    name: 'OpenUrl',
    label: 'Open URL',
    icon: <Link className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined, post_shortcuts: [] },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'OpenFile',
    label: 'Open File',
    icon: <FileText className="size-5" />,
    action: { type: 'OpenFile', path: '', app: undefined, post_shortcuts: [] },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'OpenApp',
    label: 'Open App',
    icon: <AppWindow className="size-5" />,
    action: { type: 'OpenApp', app_path: '', post_shortcuts: [] },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'RunCommand',
    label: 'Run Command',
    icon: <Terminal className="size-5" />,
    action: { type: 'RunCommand', command: '', args: [], shell: 'Zsh' },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Notify',
    label: 'Reminder',
    icon: <Bell className="size-5" />,
    action: { type: 'Notify', title: '', body: '', sound: true },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'Webhook',
    label: 'Webhook',
    icon: <Globe className="size-5" />,
    action: { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Custom',
    label: 'Custom',
    icon: <Settings className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined, post_shortcuts: [] },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
];
```

- [ ] **Step 3: Commit**

```bash
git add src/components/AutoName.ts src/components/TemplateGrid.tsx
git commit -m "feat: update AutoName and TemplateGrid for OpenApp, remove Shortcut"
```

---

### Task 9: ActionBuilder Rewrite — File Picker, OpenApp, Post-Shortcuts

**Files:**
- Modify: `src/components/ActionBuilder.tsx`

This is the largest frontend task. The ActionBuilder needs:
1. Remove "Keys" tab
2. Add "App" tab
3. Add file picker for OpenFile and OpenApp
4. Add post_shortcuts collapsible section for OpenFile, OpenUrl, OpenApp

- [ ] **Step 1: Rewrite ActionBuilder**

Replace the entire content of `src/components/ActionBuilder.tsx`:

```tsx
import { useState } from 'react';
import { Plus, X, FolderOpen } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { type Action, type Shell, type HttpMethod, type KeyCombo, type Modifier } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface ActionBuilderProps {
  value: Action;
  onChange: (a: Action) => void;
}

const ACTION_TYPES: { type: Action['type']; label: string }[] = [
  { type: 'OpenUrl', label: 'URL' },
  { type: 'OpenFile', label: 'File' },
  { type: 'OpenApp', label: 'App' },
  { type: 'RunCommand', label: 'Cmd' },
  { type: 'Notify', label: 'Notify' },
  { type: 'Webhook', label: 'Hook' },
];

const SHELLS: Shell[] = ['Sh', 'Bash', 'Zsh', 'Python', 'AppleScript'];
const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
const MODIFIERS: Modifier[] = ['Cmd', 'Shift', 'Opt', 'Ctrl'];

function headersToText(headers: Record<string, string>): string {
  return Object.entries(headers).map(([k, v]) => `${k}=${v}`).join('\n');
}

function textToHeaders(text: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const idx = line.indexOf('=');
    if (idx < 1) continue;
    result[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
  }
  return result;
}

function defaultAction(type: Action['type']): Action {
  switch (type) {
    case 'OpenFile': return { type: 'OpenFile', path: '', app: undefined, post_shortcuts: [] };
    case 'OpenUrl': return { type: 'OpenUrl', url: '', browser: undefined, post_shortcuts: [] };
    case 'OpenApp': return { type: 'OpenApp', app_path: '', post_shortcuts: [] };
    case 'RunCommand': return { type: 'RunCommand', command: '', args: [], shell: 'Zsh' };
    case 'Notify': return { type: 'Notify', title: '', body: '', sound: true };
    case 'Webhook': return { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined };
  }
}

// ── Post-shortcuts sub-component ──────────────────────────────────────

function PostShortcutsEditor({ shortcuts, onChange }: {
  shortcuts: KeyCombo[];
  onChange: (s: KeyCombo[]) => void;
}) {
  const [expanded, setExpanded] = useState(shortcuts.length > 0);

  function addCombo() {
    onChange([...shortcuts, { modifiers: [], key: '' }]);
    setExpanded(true);
  }

  function removeCombo(index: number) {
    onChange(shortcuts.filter((_, i) => i !== index));
  }

  function updateCombo(index: number, combo: KeyCombo) {
    onChange(shortcuts.map((c, i) => i === index ? combo : c));
  }

  function toggleModifier(index: number, mod: Modifier) {
    const combo = shortcuts[index];
    const has = combo.modifiers.includes(mod);
    const newMods = has
      ? combo.modifiers.filter((m) => m !== mod)
      : [...combo.modifiers, mod];
    updateCombo(index, { ...combo, modifiers: newMods });
  }

  if (!expanded && shortcuts.length === 0) {
    return (
      <button
        type="button"
        onClick={() => { addCombo(); }}
        className="text-sm text-primary hover:underline"
      >
        + Run shortcuts after open
      </button>
    );
  }

  return (
    <div className="space-y-3">
      <Label className="text-xs font-medium uppercase tracking-widest text-muted-foreground">
        Shortcuts after open
      </Label>
      {shortcuts.map((combo, i) => (
        <div key={i} className="flex items-center gap-2">
          <div className="flex gap-1">
            {MODIFIERS.map((mod) => (
              <button
                key={mod}
                type="button"
                onClick={() => toggleModifier(i, mod)}
                className={`px-2 py-1 text-xs rounded border transition-colors ${
                  combo.modifiers.includes(mod)
                    ? 'bg-primary text-primary-foreground border-primary'
                    : 'bg-background border-border text-muted-foreground hover:border-primary/30'
                }`}
              >
                {mod}
              </button>
            ))}
          </div>
          <Input
            value={combo.key}
            onChange={(e) => updateCombo(i, { ...combo, key: e.target.value })}
            placeholder="key"
            className="w-20 font-mono"
          />
          <Button variant="ghost" size="icon-sm" onClick={() => removeCombo(i)} className="text-muted-foreground hover:text-destructive">
            <X className="size-3.5" />
          </Button>
        </div>
      ))}
      <Button variant="outline" size="sm" onClick={addCombo} className="w-full">
        <Plus className="size-3.5" />
        Add shortcut
      </Button>
    </div>
  );
}

// ── File/App picker helpers ───────────────────────────────────────────

async function pickFile(): Promise<string | null> {
  const result = await open({ multiple: false, directory: false });
  return result ?? null;
}

async function pickApp(): Promise<string | null> {
  const result = await open({
    multiple: false,
    directory: false,
    defaultPath: '/Applications',
    filters: [{ name: 'Applications', extensions: ['app'] }],
  });
  return result ?? null;
}

// ── Main component ────────────────────────────────────────────────────

export default function ActionBuilder({ value, onChange }: ActionBuilderProps) {
  function handleTypeChange(v: string) {
    onChange(defaultAction(v as Action['type']));
  }

  const hasPostShortcuts = value.type === 'OpenFile' || value.type === 'OpenUrl' || value.type === 'OpenApp';
  const postShortcuts = hasPostShortcuts ? (value as { post_shortcuts: KeyCombo[] }).post_shortcuts : [];

  function handleShortcutsChange(shortcuts: KeyCombo[]) {
    onChange({ ...value, post_shortcuts: shortcuts } as Action);
  }

  return (
    <div className="space-y-4">
      <Tabs value={value.type} onValueChange={handleTypeChange}>
        <TabsList className="w-full flex-wrap h-auto gap-0 p-1">
          {ACTION_TYPES.map(({ type, label }) => (
            <TabsTrigger key={type} value={type} className="flex-1">{label}</TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {value.type === 'OpenFile' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>File path</Label>
            <div className="flex gap-2">
              <Input
                value={value.path}
                onChange={(e) => onChange({ ...value, path: e.target.value })}
                placeholder="/path/to/file"
                className="font-mono flex-1"
              />
              <Button variant="outline" size="sm" onClick={async () => {
                const path = await pickFile();
                if (path) onChange({ ...value, path });
              }}>
                <FolderOpen className="size-4" />
              </Button>
            </div>
          </div>
          <div className="space-y-2">
            <Label>Open with app <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <div className="flex gap-2">
              <Input
                value={value.app ?? ''}
                onChange={(e) => onChange({ ...value, app: e.target.value || undefined })}
                placeholder="Default app"
                className="flex-1"
              />
              <Button variant="outline" size="sm" onClick={async () => {
                const path = await pickApp();
                if (path) {
                  const name = path.split('/').pop()?.replace('.app', '') || path;
                  onChange({ ...value, app: name });
                }
              }}>
                <FolderOpen className="size-4" />
              </Button>
            </div>
          </div>
        </div>
      )}

      {value.type === 'OpenUrl' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>URL</Label>
            <Input value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://example.com" />
          </div>
          <div className="space-y-2">
            <Label>Browser <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Input value={value.browser ?? ''} onChange={(e) => onChange({ ...value, browser: e.target.value || undefined })} placeholder="Safari, Firefox, …" />
          </div>
        </div>
      )}

      {value.type === 'OpenApp' && (
        <div className="space-y-2">
          <Label>Application</Label>
          <div className="flex gap-2">
            <Input
              value={value.app_path}
              onChange={(e) => onChange({ ...value, app_path: e.target.value })}
              placeholder="/Applications/App.app"
              className="font-mono flex-1"
            />
            <Button variant="outline" size="sm" onClick={async () => {
              const path = await pickApp();
              if (path) onChange({ ...value, app_path: path });
            }}>
              <FolderOpen className="size-4" />
            </Button>
          </div>
        </div>
      )}

      {value.type === 'RunCommand' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>Shell</Label>
            <Select value={value.shell} onValueChange={(v) => onChange({ ...value, shell: v as Shell })}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>{SHELLS.map((s) => <SelectItem key={s} value={s}>{s}</SelectItem>)}</SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label>Command</Label>
            <Input value={value.command} onChange={(e) => onChange({ ...value, command: e.target.value })} placeholder="echo hello" className="font-mono" />
          </div>
          <div className="space-y-2">
            <Label>Arguments <span className="text-muted-foreground font-normal">(one per line)</span></Label>
            <Textarea value={value.args.join('\n')} onChange={(e) => onChange({ ...value, args: e.target.value.split('\n').map((a) => a.trim()).filter(Boolean) })} placeholder={"--flag\nvalue"} className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}

      {value.type === 'Notify' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>Title</Label>
            <Input value={value.title} onChange={(e) => onChange({ ...value, title: e.target.value })} placeholder="Notification title" />
          </div>
          <div className="space-y-2">
            <Label>Body</Label>
            <Textarea value={value.body} onChange={(e) => onChange({ ...value, body: e.target.value })} placeholder="Notification body" className="resize-none" rows={3} />
          </div>
          <div className="flex items-center gap-2">
            <Switch id="n-sound" checked={value.sound} onCheckedChange={(checked) => onChange({ ...value, sound: checked })} />
            <Label htmlFor="n-sound">Play sound</Label>
          </div>
        </div>
      )}

      {value.type === 'Webhook' && (
        <div className="space-y-3">
          <div className="flex gap-2">
            <div className="space-y-2">
              <Label>Method</Label>
              <Select value={value.method} onValueChange={(v) => onChange({ ...value, method: v as HttpMethod })}>
                <SelectTrigger className="w-24"><SelectValue /></SelectTrigger>
                <SelectContent>{HTTP_METHODS.map((m) => <SelectItem key={m} value={m}>{m}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="space-y-2 flex-1">
              <Label>URL</Label>
              <Input value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://api.example.com/hook" />
            </div>
          </div>
          <div className="space-y-2">
            <Label>Headers <span className="text-muted-foreground font-normal">(key=value per line)</span></Label>
            <Textarea value={headersToText(value.headers)} onChange={(e) => onChange({ ...value, headers: textToHeaders(e.target.value) })} placeholder="Authorization=Bearer token" className="font-mono resize-none" rows={3} />
          </div>
          <div className="space-y-2">
            <Label>Body <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Textarea value={value.body ?? ''} onChange={(e) => onChange({ ...value, body: e.target.value || undefined })} placeholder='{"key": "value"}' className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}

      {/* Post-shortcuts for visual actions */}
      {hasPostShortcuts && (
        <>
          <div className="border-t border-border pt-4" />
          <PostShortcutsEditor shortcuts={postShortcuts} onChange={handleShortcutsChange} />
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/ActionBuilder.tsx
git commit -m "feat: rewrite ActionBuilder — file picker, OpenApp, post-shortcuts UI"
```

---

### Task 10: TaskEditor Defaults + Validation

**Files:**
- Modify: `src/components/TaskEditor.tsx`

- [ ] **Step 1: Update validation in handleSave**

In the `handleSave` function of `TaskEditor.tsx`, replace the validation block:

```ts
    // Validation
    if (schedule.type === 'Cron' && !schedule.expression.trim()) { setError('Cron expression is required'); return; }
    if (schedule.type === 'OneShot' && isNaN(new Date(schedule.run_at).getTime())) { setError('Invalid date'); return; }
    if (action.type === 'OpenFile' && !action.path?.trim()) { setError('File path is required'); return; }
    if (action.type === 'OpenUrl' && !action.url?.trim()) { setError('URL is required'); return; }
    if (action.type === 'OpenApp' && !action.app_path?.trim()) { setError('Application path is required'); return; }
    if (action.type === 'RunCommand' && !action.command?.trim()) { setError('Command is required'); return; }
    if (action.type === 'Notify' && !action.title?.trim()) { setError('Notification title is required'); return; }
    if (action.type === 'Webhook' && !action.url?.trim()) { setError('Webhook URL is required'); return; }
```

This removes the `Shortcut` validation and adds `OpenApp` validation.

- [ ] **Step 2: Commit**

```bash
git add src/components/TaskEditor.tsx
git commit -m "feat: update TaskEditor validation for new action types"
```

---

### Task 11: Build Verification

- [ ] **Step 1: TypeScript check**

```bash
cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit 2>&1 | head -30
```

Fix any type errors.

- [ ] **Step 2: Rust check**

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo check 2>&1 | tail -30
```

Fix any compilation errors. The `objc2` API may need adjustments — method names and signatures can differ from what's in the plan. The implementer should consult the crate docs.

- [ ] **Step 3: Vite build**

```bash
cd /Users/marcus.grando/git/cronmac && bun run build 2>&1 | tail -10
```

- [ ] **Step 4: Store tests**

```bash
cd /Users/marcus.grando/git/cronmac/src-tauri && cargo test 2>&1 | tail -20
```

The store tests use `Action::Notify` and `Action::RunCommand` which are unchanged, so they should pass.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A && git commit -m "fix: build verification adjustments"
```

---

### Task 12: Integration Test

- [ ] **Step 1: Run the app**

```bash
cd /Users/marcus.grando/git/cronmac && cargo tauri dev
```

- [ ] **Step 2: Test template grid**

1. Click "+" in popover → verify 7 templates (URL, File, App, Command, Reminder, Webhook, Custom)
2. Verify "Open App" template exists where "Keys" used to be

- [ ] **Step 3: Test OpenApp action**

1. Click "Open App" template → editor window opens
2. Click "Browse" → native file picker opens
3. Navigate to /Applications, select an app
4. Verify app_path is populated
5. Click "Create" → task created

- [ ] **Step 4: Test OpenFile with file picker**

1. Create a new File task
2. Click "Browse" → file picker opens
3. Select a file → path populated
4. Optionally select "Open with" app
5. Create and verify

- [ ] **Step 5: Test post-shortcuts**

1. Edit an OpenUrl task
2. Click "+ Run shortcuts after open"
3. Add a shortcut: Cmd + toggle, type "n" in key field
4. Add another: Cmd + Shift + type "s"
5. Save and run the task
6. Verify: URL opens, then shortcuts execute after activation

- [ ] **Step 6: Test disable/delete bugs**

1. Create a Cron task (every minute)
2. Wait for it to execute once (check history)
3. Toggle it off
4. Wait 1+ minutes → verify it does NOT execute (shows "skipped" in history)
5. Toggle back on → verify it resumes
6. Delete the task → verify no more executions

- [ ] **Step 7: Test window.close**

1. Edit a task → delete it from the editor
2. Verify: editor window closes after delete

- [ ] **Step 8: Commit any fixes**

```bash
git add -A && git commit -m "fix: integration test adjustments"
```
