# SwiftUI Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate Takt from Tauri+React to a native SwiftUI app backed by a Rust core library (`libtakt`) exposed via UniFFI.

**Architecture:** Rust core extracted into `libtakt/` crate with `PlatformBridge` trait replacing Tauri dependencies. UniFFI (`with_foreign`) generates Swift bindings. SwiftUI app in `macos/` consumes bindings via thin ViewModels. Follows Ghostty pattern — ~90% Rust core, ~10% native shell.

**Tech Stack:** Rust (UniFFI 0.28, SQLx, tokio-cron-scheduler, objc2), Swift (SwiftUI, macOS 14.0+), Xcode

**Spec:** `docs/superpowers/specs/2026-04-06-swiftui-migration-design.md`

---

## File Map

### New files to create

| File | Responsibility |
|------|---------------|
| `Cargo.toml` (workspace root) | Workspace definition pointing to `libtakt` |
| `libtakt/Cargo.toml` | Crate config with UniFFI, staticlib |
| `libtakt/uniffi.toml` | UniFFI Swift configuration |
| `libtakt/src/lib.rs` | `TaktCore` UniFFI object + scaffolding |
| `libtakt/src/platform.rs` | `PlatformBridge` trait + callback registry |
| `libtakt/src/error.rs` | `TaktError` UniFFI-compatible error enum |
| `libtakt/src/bin/uniffi-bindgen-swift.rs` | Bindgen binary entry point |
| `scripts/build-rust.sh` | Build static lib + generate Swift bindings |
| `macos/Takt/TaktApp.swift` | SwiftUI @main, MenuBarExtra, Window |
| `macos/Takt/TaktCore+Bridge.swift` | MacOSPlatformBridge + TaktCore wrapper |
| `macos/Takt/Views/TaskListView.swift` | Task list in popover |
| `macos/Takt/Views/TaskItemView.swift` | Single task row |
| `macos/Takt/Views/TaskEditorView.swift` | Editor form |
| `macos/Takt/Views/ActionBuilderView.swift` | 7 action type builder |
| `macos/Takt/Views/ScheduleBuilderView.swift` | 3 schedule type builder |
| `macos/Takt/Views/HistoryView.swift` | Execution history |
| `macos/Takt/Views/TemplateGridView.swift` | Template picker |
| `macos/Takt/Views/Schedule/DayGridView.swift` | Day-of-month selector |
| `macos/Takt/Views/Schedule/WeekdayGridView.swift` | Weekday selector |
| `macos/Takt/Views/Schedule/TimeFieldView.swift` | Hour/minute picker |
| `macos/Takt/ViewModels/TaskListViewModel.swift` | List state + CRUD ops |
| `macos/Takt/ViewModels/TaskEditorViewModel.swift` | Editor state + dirty tracking |
| `macos/Takt/Helpers/CronUtils.swift` | Cron parse/build (port of cron-utils.ts) |
| `macos/Takt/Helpers/AutoName.swift` | Auto task name (port of AutoName.ts) |

### Files moved from `src-tauri/src/` to `libtakt/src/` (with modifications noted)

| Source | Destination | Changes |
|--------|------------|---------|
| `src-tauri/src/models.rs` | `libtakt/src/models.rs` | Add UniFFI derives |
| `src-tauri/src/store.rs` | `libtakt/src/store.rs` | None |
| `src-tauri/src/db.rs` | `libtakt/src/db.rs` | None |
| `src-tauri/src/scheduler.rs` | `libtakt/src/scheduler.rs` | `AppHandle` → `PlatformBridge` |
| `src-tauri/src/executor/mod.rs` | `libtakt/src/executor/mod.rs` | `AppHandle` → `PlatformBridge` |
| `src-tauri/src/executor/macos.rs` | `libtakt/src/executor/macos.rs` | Async `run_on_main`, bridge notifications |
| `src-tauri/src/executor/keymap.rs` | `libtakt/src/executor/keymap.rs` | None |
| `src-tauri/src/executor/tests.rs` | `libtakt/src/executor/tests.rs` | None |
| `src-tauri/src/launch_agent.rs` | `libtakt/src/launch_agent.rs` | None |
| `src-tauri/migrations/*` | `libtakt/migrations/*` | None |

### Files deleted at end

| File | Reason |
|------|--------|
| `src-tauri/` (entire directory) | Replaced by `libtakt/` |
| `src/` (React components) | Replaced by `macos/` SwiftUI |
| `package.json`, `bun.lock` | No more JS |
| `vite.config.ts`, `tsconfig.json` | No more Vite |
| `index.html`, `editor.html` | No more webviews |

---

## Phase 1 — Rust Core (libtakt)

### Task 1: Create workspace and libtakt crate skeleton

**Files:**
- Create: `Cargo.toml` (workspace root — replaces current if exists)
- Create: `libtakt/Cargo.toml`
- Create: `libtakt/src/lib.rs` (minimal)

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
# cronmac/Cargo.toml
[workspace]
members = ["libtakt"]
resolver = "2"
```

- [ ] **Step 2: Create libtakt/Cargo.toml**

```toml
[package]
name = "libtakt"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib", "lib"]
name = "libtakt"

[[bin]]
name = "uniffi-bindgen-swift"
path = "src/bin/uniffi-bindgen-swift.rs"

[dependencies]
uniffi = { version = "0.28", features = ["cli", "bindgen-swift"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
tokio-cron-scheduler = "0.13"
croner = "2"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "macros", "migrate", "uuid", "chrono"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json"] }
anyhow = "1"
thiserror = "1"
async-trait = "0.1"
dirs = "5"
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSURL", "NSString", "NSArray", "NSObject", "NSBundle"] }
objc2-app-kit = { version = "0.3", features = ["NSWorkspace", "NSRunningApplication"] }
block2 = "0.6"
core-graphics = "0.24"

[build-dependencies]
uniffi = { version = "0.28", features = ["build"] }
```

- [ ] **Step 3: Create minimal lib.rs**

```rust
// libtakt/src/lib.rs
uniffi::setup_scaffolding!();
```

- [ ] **Step 4: Create uniffi-bindgen-swift binary**

```rust
// libtakt/src/bin/uniffi-bindgen-swift.rs
fn main() {
    uniffi::uniffi_bindgen_swift()
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS (empty crate with scaffolding)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml libtakt/
git commit -m "feat: create libtakt crate skeleton with UniFFI scaffolding"
```

---

### Task 2: Copy unchanged modules into libtakt

**Files:**
- Copy: `src-tauri/src/models.rs` → `libtakt/src/models.rs`
- Copy: `src-tauri/src/store.rs` → `libtakt/src/store.rs`
- Copy: `src-tauri/src/db.rs` → `libtakt/src/db.rs`
- Copy: `src-tauri/src/launch_agent.rs` → `libtakt/src/launch_agent.rs`
- Copy: `src-tauri/src/executor/keymap.rs` → `libtakt/src/executor/keymap.rs`
- Copy: `src-tauri/src/executor/tests.rs` → `libtakt/src/executor/tests.rs`
- Copy: `src-tauri/migrations/` → `libtakt/migrations/`

- [ ] **Step 1: Copy files**

```bash
cp src-tauri/src/models.rs libtakt/src/models.rs
cp src-tauri/src/store.rs libtakt/src/store.rs
cp src-tauri/src/db.rs libtakt/src/db.rs
cp src-tauri/src/launch_agent.rs libtakt/src/launch_agent.rs
mkdir -p libtakt/src/executor
cp src-tauri/src/executor/keymap.rs libtakt/src/executor/keymap.rs
cp src-tauri/src/executor/tests.rs libtakt/src/executor/tests.rs
cp -r src-tauri/migrations libtakt/migrations
```

- [ ] **Step 2: Register modules in lib.rs**

```rust
// libtakt/src/lib.rs
uniffi::setup_scaffolding!();

mod db;
mod executor;
mod launch_agent;
mod models;
mod store;
```

Note: `executor/mod.rs` and `scheduler.rs` are NOT copied yet — they need refactoring (Tasks 4-5).

- [ ] **Step 3: Create a stub executor/mod.rs so the crate compiles**

```rust
// libtakt/src/executor/mod.rs
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

// current_executor() will be added in Task 4 after PlatformBridge exists

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Verify store and db tests pass**

Run: `cargo test --package libtakt -- store::tests db::tests`
Expected: All 5 tests pass (test_create_and_list_tasks, test_update_task, test_delete_task, test_log_execution, test_db_connects_and_migrates)

- [ ] **Step 5: Commit**

```bash
git add libtakt/src/ libtakt/migrations/
git commit -m "feat: copy unchanged modules into libtakt (models, store, db, keymap, launch_agent)"
```

---

### Task 3: Create PlatformBridge trait and callback registry

**Files:**
- Create: `libtakt/src/platform.rs`

- [ ] **Step 1: Write platform.rs**

```rust
// libtakt/src/platform.rs
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::oneshot;

/// Trait implemented by the host platform (Swift on macOS, C# on Windows).
/// Uses UniFFI `with_foreign` — gives `Arc<dyn PlatformBridge>` on Rust side.
#[uniffi::export(with_foreign)]
pub trait PlatformBridge: Send + Sync {
    /// Send a native notification.
    fn send_notification(&self, title: String, body: String, sound: bool);

    /// Execute a registered callback on the main thread.
    /// Swift MUST use DispatchQueue.main.async (not .sync) to avoid deadlock,
    /// then call `TaktCallbackRegistry.execute(callbackId)` to run the closure.
    fn run_on_main_sync(&self, callback_id: u64);
}

// ── Callback Registry ────────────────────────────────────────────────

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

type BoxedCallback = Box<dyn FnOnce() + Send + 'static>;

static REGISTRY: Mutex<Option<HashMap<u64, BoxedCallback>>> = Mutex::new(None);

fn registry() -> std::sync::MutexGuard<'static, Option<HashMap<u64, BoxedCallback>>> {
    let mut guard = REGISTRY.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

/// Register a callback and return its ID.
fn register_callback(f: BoxedCallback) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry().as_mut().unwrap().insert(id, f);
    id
}

/// Execute and remove a callback by ID. Called from Swift via FFI.
#[uniffi::export]
pub fn execute_callback(callback_id: u64) {
    let cb = registry().as_mut().and_then(|m| m.remove(&callback_id));
    if let Some(f) = cb {
        f();
    }
}

/// Run a closure on the main thread via PlatformBridge and wait for completion.
/// Returns the closure's result. Uses oneshot channel so the calling tokio task
/// yields instead of blocking an OS thread.
pub async fn run_on_main<F, R>(bridge: &dyn PlatformBridge, f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let callback = Box::new(move || {
        let result = f();
        let _ = tx.send(result);
    });
    let id = register_callback(callback);
    bridge.run_on_main_sync(id);
    // Await completion — does NOT block the OS thread
    rx.await.expect("Main thread callback was dropped without executing")
}
```

- [ ] **Step 2: Register module in lib.rs**

Add `pub mod platform;` to `libtakt/src/lib.rs` after the other modules.

```rust
// libtakt/src/lib.rs
uniffi::setup_scaffolding!();

mod db;
mod executor;
mod launch_agent;
mod models;
pub mod platform;
mod store;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add libtakt/src/platform.rs libtakt/src/lib.rs
git commit -m "feat: add PlatformBridge trait with callback registry for main thread dispatch"
```

---

### Task 4: Refactor executor to use PlatformBridge

**Files:**
- Modify: `libtakt/src/executor/mod.rs`
- Modify: `libtakt/src/executor/macos.rs` (was copied but needs refactoring)

- [ ] **Step 1: Copy macos.rs from src-tauri and refactor**

Copy `src-tauri/src/executor/macos.rs` to `libtakt/src/executor/macos.rs`, then apply these changes:

1. Replace `tauri::AppHandle` with `Arc<dyn PlatformBridge>`
2. Replace `run_on_main` to use async `platform::run_on_main`
3. Replace `send_notification` to use `bridge.send_notification`
4. Make `execute` method properly async through the `run_on_main` calls

The full refactored file:

```rust
// libtakt/src/executor/macos.rs
use super::keymap::resolve_keycode;
use super::{ActionExecutor, ExecutionResult, ExecutorError};
use crate::models::{Action, HttpMethod, KeyCombo, Modifier, Shell};
use crate::platform::{self, PlatformBridge};
use async_trait::async_trait;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSString, NSURL};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

pub struct MacosExecutor {
    bridge: Arc<dyn PlatformBridge>,
}

impl MacosExecutor {
    pub fn new(bridge: Arc<dyn PlatformBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile {
                path,
                app,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let path = path.clone();
                let app = app.clone();
                platform::run_on_main(&*self.bridge, move || open_file(&path, app.as_deref()))
                    .await
                    .map_err(|e| e)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::OpenUrl {
                url,
                browser,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let url = url.clone();
                let browser = browser.clone();
                platform::run_on_main(&*self.bridge, move || {
                    open_url(&url, browser.as_deref())
                })
                .await
                .map_err(|e| e)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::OpenApp {
                app_path,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let app_path = app_path.clone();
                platform::run_on_main(&*self.bridge, move || open_app(&app_path))
                    .await
                    .map_err(|e| e)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::Settings { pane_url } => {
                let pane_url = pane_url.clone();
                platform::run_on_main(&*self.bridge, move || open_url(&pane_url, None))
                    .await
                    .map_err(|e| e)?;
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::RunCommand {
                command,
                args,
                shell,
            } => run_command(command, args, shell),
            Action::Notify { title, body, sound } => {
                self.bridge
                    .send_notification(title.clone(), body.clone(), *sound);
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::Webhook {
                url,
                method,
                headers,
                body,
            } => send_webhook(url, method, headers, body.as_deref()).await,
        }
    }
}

// ── Open via NSWorkspace (main thread) ────────────────────────────────
// These functions are unchanged from the original — they run INSIDE
// the main thread callback dispatched by platform::run_on_main.

fn open_file(path: &str, app: Option<&str>) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let file_url = NSURL::fileURLWithPath(&NSString::from_str(path));

    if let Some(app_name) = app {
        let config = NSWorkspaceOpenConfiguration::configuration();
        let app_url = resolve_app_url(app_name);
        let urls = NSArray::from_retained_slice(&[file_url]);
        workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &urls, &app_url, &config, None,
        );
    } else {
        let opened = workspace.openURL(&file_url);
        if !opened {
            return Err(ExecutorError::CommandFailed(format!(
                "Failed to open file: {}",
                path
            )));
        }
    }
    Ok(())
}

fn open_url(url: &str, browser: Option<&str>) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let has_scheme = url.contains("://")
        || url.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '+')
        });
    let normalized = if has_scheme {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    let ns_url = NSURL::URLWithString(&NSString::from_str(&normalized))
        .ok_or_else(|| ExecutorError::CommandFailed(format!("Invalid URL: {}", url)))?;

    if let Some(browser_name) = browser {
        let config = NSWorkspaceOpenConfiguration::configuration();
        let app_url = resolve_app_url(browser_name);
        let urls = NSArray::from_retained_slice(&[ns_url]);
        workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &urls, &app_url, &config, None,
        );
    } else {
        let opened = workspace.openURL(&ns_url);
        if !opened {
            return Err(ExecutorError::CommandFailed(format!(
                "Failed to open URL: {}",
                url
            )));
        }
    }
    Ok(())
}

fn open_app(app_path: &str) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app_url = NSURL::fileURLWithPath(&NSString::from_str(app_path));
    let config = NSWorkspaceOpenConfiguration::configuration();
    workspace.openApplicationAtURL_configuration_completionHandler(&app_url, &config, None);
    Ok(())
}

fn resolve_app_url(app_name: &str) -> objc2::rc::Retained<NSURL> {
    let path = if app_name.ends_with(".app") || app_name.starts_with('/') {
        app_name.to_string()
    } else {
        format!("/Applications/{}.app", app_name)
    };
    NSURL::fileURLWithPath(&NSString::from_str(&path))
}

// ── Post-shortcuts ──────────────────────────────────────────────────
// Unchanged from original.

async fn wait_and_send_shortcuts(
    shortcuts: &[KeyCombo],
    delay_secs: u64,
) -> Result<(), ExecutorError> {
    if !accessibility_is_trusted() {
        return Err(ExecutorError::AccessibilityRequired(
            "Grant Accessibility permission in System Settings -> Privacy & Security -> Accessibility"
                .into(),
        ));
    }
    let delay = Duration::from_secs(delay_secs);
    for combo in shortcuts {
        tokio::time::sleep(delay).await;
        send_key_combo(combo)?;
    }
    Ok(())
}

fn send_key_combo(combo: &KeyCombo) -> Result<(), ExecutorError> {
    let keycode = resolve_keycode(&combo.key)
        .ok_or_else(|| ExecutorError::CommandFailed(format!("Unknown key: {}", combo.key)))?;
    let mut flags = CGEventFlags::CGEventFlagNull;
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
    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode as CGKeyCode, true)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-down event".into()))?;
    key_down.set_flags(flags);
    key_down.post(CGEventTapLocation::HID);
    let key_up = CGEvent::new_keyboard_event(source, keycode as CGKeyCode, false)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-up event".into()))?;
    key_up.set_flags(flags);
    key_up.post(CGEventTapLocation::HID);
    Ok(())
}

fn accessibility_is_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

// ── RunCommand ──────────────────────────────────────────────────────
// Unchanged from original.

fn run_command(
    command: &str,
    args: &[String],
    shell: &Shell,
) -> Result<ExecutionResult, ExecutorError> {
    let (shell_bin, shell_flag) = match shell {
        Shell::Sh => ("/bin/sh", "-c"),
        Shell::Bash => ("/bin/bash", "-c"),
        Shell::Zsh => ("/bin/zsh", "-c"),
        Shell::Python => ("/usr/bin/python3", "-c"),
        Shell::AppleScript => ("/usr/bin/osascript", "-e"),
    };
    let mut cmd = Command::new(shell_bin);
    match shell {
        Shell::Sh | Shell::Bash | Shell::Zsh => {
            let full = if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            cmd.arg(shell_flag).arg(&full);
        }
        Shell::Python => {
            cmd.arg(shell_flag).arg(command);
            for arg in args {
                cmd.arg(arg);
            }
        }
        Shell::AppleScript => {
            cmd.arg(shell_flag).arg(command);
            if !args.is_empty() {
                cmd.arg("--");
                for arg in args {
                    cmd.arg(arg);
                }
            }
        }
    }
    let output = cmd.output()?;
    let stdout = if output.stdout.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    };
    let stderr = if output.stderr.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&output.stderr).to_string())
    };
    if output.status.success() {
        Ok(ExecutionResult { stdout, stderr })
    } else {
        Err(ExecutorError::CommandFailed(format!(
            "Exit code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.as_deref().unwrap_or("")
        )))
    }
}

// ── Webhook ──────────────────────────────────────────────────────────
// Unchanged from original.

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
    let response = req
        .send()
        .await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;
    let status = response.status();
    let response_body = response
        .text()
        .await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;
    if status.is_success() {
        Ok(ExecutionResult {
            stdout: Some(response_body),
            stderr: None,
        })
    } else {
        Err(ExecutorError::CommandFailed(format!(
            "HTTP {} - {}",
            status, response_body
        )))
    }
}
```

- [ ] **Step 2: Update executor/mod.rs with current_executor using PlatformBridge**

Add the `current_executor` function and `MacosExecutor` re-export to `libtakt/src/executor/mod.rs`:

```rust
// Add at the end, before #[cfg(test)]

#[cfg(target_os = "macos")]
pub use macos::MacosExecutor;

pub fn current_executor(bridge: std::sync::Arc<dyn crate::platform::PlatformBridge>) -> Box<dyn ActionExecutor> {
    #[cfg(target_os = "macos")]
    return Box::new(MacosExecutor::new(bridge));

    #[cfg(not(target_os = "macos"))]
    panic!("No executor available for this platform");
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add libtakt/src/executor/
git commit -m "feat: refactor executor to use PlatformBridge instead of Tauri AppHandle"
```

---

### Task 5: Refactor scheduler to use PlatformBridge

**Files:**
- Create: `libtakt/src/scheduler.rs` (refactored copy)

- [ ] **Step 1: Copy and refactor scheduler.rs**

Copy `src-tauri/src/scheduler.rs` to `libtakt/src/scheduler.rs` and apply these changes:

1. Remove `use tauri_plugin_notification::NotificationExt;`
2. Replace `app_handle: tauri::AppHandle` with `bridge: Arc<dyn PlatformBridge>` in `AppScheduler` struct
3. Replace `app_handle: tauri::AppHandle` param in `AppScheduler::new()`
4. Replace all `app_handle.clone()` with `bridge.clone()` in `schedule_task()` and `catch_up_missed()`
5. Replace `tauri::AppHandle` with `Arc<dyn PlatformBridge>` in `daily_first_use_loop()` and `execute_and_log()` signatures
6. Replace `send_run_notification` to use `bridge.send_notification()` directly

Key changes (showing only the lines that differ — everything else is identical to `src-tauri/src/scheduler.rs`):

```rust
// libtakt/src/scheduler.rs — top imports
use crate::executor::ActionExecutor;
use crate::models::{Action, Schedule, TaskDto};
use crate::platform::PlatformBridge;
use crate::store::TaskStore;
// ... (chrono, HashMap, Arc, Mutex, Job, JobScheduler, CancellationToken, Uuid unchanged)

pub struct AppScheduler {
    inner: JobScheduler,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,  // was: app_handle: tauri::AppHandle
    job_ids: Mutex<HashMap<String, Uuid>>,
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AppScheduler {
    pub async fn new(
        store: Arc<TaskStore>,
        executor: Arc<Box<dyn ActionExecutor>>,
        bridge: Arc<dyn PlatformBridge>,  // was: app_handle: tauri::AppHandle
    ) -> anyhow::Result<Self> {
        let inner = JobScheduler::new().await?;
        Ok(Self {
            inner, executor, store, bridge,  // was: app_handle
            job_ids: Mutex::new(HashMap::new()),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    // ...
}
```

In `schedule_task()`, `catch_up_missed()`: replace every `self.app_handle.clone()` and `app_handle.clone()` with `self.bridge.clone()` / `bridge.clone()`.

In `execute_and_log()`:
```rust
pub(crate) async fn execute_and_log(
    executor: &dyn ActionExecutor,
    store: &TaskStore,
    bridge: &dyn PlatformBridge,  // was: app_handle: &tauri::AppHandle
    task_id: &str,
    task_name: &str,
    notify: bool,
    action: &Action,
) -> Result<(), String> {
    let result = executor.execute(action).await;
    let (status, stdout, stderr, error) = match &result {
        Ok(r) => ("success", r.stdout.clone(), r.stderr.clone(), None),
        Err(e) => ("failure", None, None, Some(e.to_string())),
    };
    if notify {
        send_run_notification(bridge, task_name, status == "success");
    }
    let _ = store.log_execution(task_id, status, stdout, stderr, error.clone()).await;
    let _ = store.update_last_run(task_id, None).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn send_run_notification(bridge: &dyn PlatformBridge, task_name: &str, success: bool) {
    let body = if success {
        format!("Executed: {}", task_name)
    } else {
        format!("Failed: {}", task_name)
    };
    bridge.send_notification("Takt".to_string(), body, false);
}
```

In `daily_first_use_loop()`:
```rust
async fn daily_first_use_loop(
    cancel: CancellationToken,
    required_secs: u64,
    executor: Arc<Box<dyn ActionExecutor>>,
    store: Arc<TaskStore>,
    bridge: Arc<dyn PlatformBridge>,  // was: app_handle: tauri::AppHandle
    task_id: String,
    task_name: String,
    notify: bool,
    action: Action,
) {
    // ... body identical, just s/app_handle/bridge/ in the execute_and_log call
}
```

- [ ] **Step 2: Register scheduler module in lib.rs**

Add `mod scheduler;` to `libtakt/src/lib.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS

- [ ] **Step 4: Run existing tests**

Run: `cargo test --package libtakt`
Expected: All tests pass (store, db, executor serialization)

- [ ] **Step 5: Commit**

```bash
git add libtakt/src/scheduler.rs libtakt/src/lib.rs
git commit -m "feat: refactor scheduler to use PlatformBridge instead of Tauri AppHandle"
```

---

### Task 6: Add UniFFI derives to models

**Files:**
- Modify: `libtakt/src/models.rs`

- [ ] **Step 1: Add UniFFI derives**

UniFFI needs derives on all types that cross the FFI boundary. Add `uniffi::Record` to structs and `uniffi::Enum` to enums. Keep existing `serde` and `sqlx` derives.

The `Task` struct (DB row) does NOT need UniFFI derives — it never crosses FFI. Only `TaskDto` and the types it references need them.

Changes to `libtakt/src/models.rs`:

```rust
// TaskDto — add uniffi::Record
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct TaskDto { /* fields unchanged */ }

// Schedule — add uniffi::Enum
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type")]
pub enum Schedule { /* variants unchanged */ }

// Modifier — add uniffi::Enum
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum Modifier { /* variants unchanged */ }

// KeyCombo — add uniffi::Record
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Record)]
pub struct KeyCombo { /* fields unchanged */ }

// Action — add uniffi::Enum
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type")]
pub enum Action { /* variants unchanged */ }

// Shell — add uniffi::Enum
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
pub enum Shell { /* variants unchanged */ }

// HttpMethod — add uniffi::Enum
#[derive(Debug, Clone, Serialize, Deserialize, uniffi::Enum)]
#[allow(clippy::upper_case_acronyms)]
pub enum HttpMethod { /* variants unchanged */ }

// ExecutionLog — add uniffi::Record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, uniffi::Record)]
pub struct ExecutionLog { /* fields unchanged */ }
```

Note: `Task` (DB row with `i64` booleans) keeps its existing derives only — it does not cross FFI.

Note: UniFFI does not support `HashMap` in records/enums by default. The `headers` field in `Action::Webhook` uses `HashMap<String, String>`. Check if UniFFI 0.28 supports this — if not, change to `Vec<(String, String)>` and convert in store. UniFFI 0.28+ supports `HashMap<String, String>` natively, so this should work.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS. If UniFFI rejects any type (e.g., HashMap or serde defaults), fix them here.

- [ ] **Step 3: Run tests**

Run: `cargo test --package libtakt`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add libtakt/src/models.rs
git commit -m "feat: add UniFFI derives to all FFI-crossing model types"
```

---

### Task 7: Create TaktError and TaktCore with full CRUD + rollback logic

**Files:**
- Create: `libtakt/src/error.rs`
- Modify: `libtakt/src/lib.rs` (add TaktCore)

- [ ] **Step 1: Create error.rs**

```rust
// libtakt/src/error.rs

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
}

impl From<anyhow::Error> for TaktError {
    fn from(e: anyhow::Error) -> Self {
        TaktError::Database { msg: e.to_string() }
    }
}
```

- [ ] **Step 2: Create CreateTaskParams and UpdateTaskParams**

Add to `libtakt/src/models.rs`:

```rust
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
```

- [ ] **Step 3: Write TaktCore in lib.rs**

This is the main UniFFI object. It carries the rollback/consistency logic from `commands.rs`.

```rust
// libtakt/src/lib.rs
uniffi::setup_scaffolding!();

mod db;
mod error;
mod executor;
mod launch_agent;
mod models;
pub mod platform;
mod scheduler;
mod store;

use std::sync::{Arc, OnceLock};

use error::TaktError;
use executor::{current_executor, ActionExecutor};
use models::{CreateTaskParams, ExecutionLog, TaskDto, UpdateTaskParams};
use platform::PlatformBridge;
use scheduler::AppScheduler;
use store::TaskStore;

#[derive(uniffi::Object)]
pub struct TaktCore {
    bridge: Arc<dyn PlatformBridge>,
    store: OnceLock<Arc<TaskStore>>,
    scheduler: OnceLock<Arc<AppScheduler>>,
    executor: OnceLock<Arc<Box<dyn ActionExecutor>>>,
}

#[uniffi::export]
impl TaktCore {
    #[uniffi::constructor]
    pub fn new(bridge: Arc<dyn PlatformBridge>) -> Self {
        Self {
            bridge,
            store: OnceLock::new(),
            scheduler: OnceLock::new(),
            executor: OnceLock::new(),
        }
    }

    /// Async initialization. Idempotent — second call returns Ok immediately.
    /// Resources are only created if OnceLock is empty, preventing duplicate
    /// schedulers or DB pools from a repeated call.
    pub async fn start(&self) -> Result<(), TaktError> {
        // Guard: if already initialized, return immediately
        if self.store.get().is_some() {
            return Ok(());
        }

        let pool = db::connect().await?;
        let store = Arc::new(TaskStore::new(pool));
        let executor = Arc::new(current_executor(self.bridge.clone()));
        let scheduler = Arc::new(
            AppScheduler::new(
                Arc::clone(&store),
                Arc::clone(&executor),
                Arc::clone(&self.bridge),
            )
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?,
        );

        // Set OnceLock BEFORE starting scheduler — if set() returns Err,
        // another thread won the race; drop our resources and return Ok.
        if self.store.set(store).is_err() {
            return Ok(()); // another call won the race
        }
        let _ = self.executor.set(executor);
        let _ = self.scheduler.set(scheduler);

        // Now start the scheduler that is stored in the OnceLock
        let scheduler = self.scheduler.get().unwrap();
        scheduler
            .start()
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;
        if let Err(e) = scheduler.load_all_tasks().await {
            eprintln!("Warning: failed to load tasks: {}", e);
        }

        launch_agent::ensure_registered();

        Ok(())
    }

    // ── Helpers ──────────────────────────────────────────────────────

    fn store(&self) -> Result<&Arc<TaskStore>, TaktError> {
        self.store.get().ok_or(TaktError::NotInitialized)
    }

    fn scheduler(&self) -> Result<&Arc<AppScheduler>, TaktError> {
        self.scheduler.get().ok_or(TaktError::NotInitialized)
    }

    fn executor(&self) -> Result<&Arc<Box<dyn ActionExecutor>>, TaktError> {
        self.executor.get().ok_or(TaktError::NotInitialized)
    }

    // ── Task CRUD ───────────────────────────────────────────────────
    // Rollback logic preserved verbatim from src-tauri/src/commands.rs

    pub async fn list_tasks(&self) -> Result<Vec<TaskDto>, TaktError> {
        Ok(self.store()?.list_tasks().await?)
    }

    pub async fn get_task(&self, id: String) -> Result<Option<TaskDto>, TaktError> {
        Ok(self.store()?.get_task(&id).await?)
    }

    /// Create task with rollback on scheduler failure.
    /// Preserves logic from commands.rs:17-57.
    pub async fn create_task(&self, params: CreateTaskParams) -> Result<TaskDto, TaktError> {
        let store = self.store()?;
        let scheduler = self.scheduler()?;

        let task = store
            .create_task(
                params.name,
                params.description,
                params.run_if_missed.unwrap_or(true),
                params.notify_on_run.unwrap_or(false),
                params.schedule,
                params.action,
            )
            .await?;

        if task.enabled {
            if let Err(e) = scheduler.schedule_task(&task).await {
                let mut errors = vec![e.to_string()];
                if let Err(rb_err) = store.delete_task(&task.id).await {
                    errors.push(format!("rollback delete failed: {}", rb_err));
                    if let Err(dis_err) = store
                        .update_task(&task.id, None, None, Some(false), None, None, None, None)
                        .await
                    {
                        errors.push(format!("disable failed: {} - restart app to fix", dis_err));
                    }
                }
                return Err(TaktError::Scheduler {
                    msg: errors.join("; "),
                });
            }
        }
        Ok(task)
    }

    /// Update task with full rollback chain.
    /// Preserves logic from commands.rs:59-169.
    pub async fn update_task(&self, params: UpdateTaskParams) -> Result<TaskDto, TaktError> {
        let store = self.store()?;
        let scheduler = self.scheduler()?;

        // Snapshot old state for rollback
        let old_task = store
            .get_task(&params.id)
            .await?
            .ok_or_else(|| TaktError::NotFound {
                msg: "Task not found".to_string(),
            })?;

        // Remove old schedule
        scheduler
            .remove_task(&params.id)
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;

        // Update DB
        let task = match store
            .update_task(
                &params.id,
                params.name,
                params.description,
                params.enabled,
                params.run_if_missed,
                params.notify_on_run,
                params.schedule,
                params.action,
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                // DB failed - restore old schedule
                let mut errors = vec![e.to_string()];
                if old_task.enabled {
                    if let Err(rb_err) = scheduler.schedule_task(&old_task).await {
                        errors.push(format!("rollback re-schedule failed: {}", rb_err));
                    }
                }
                return Err(TaktError::Database {
                    msg: errors.join("; "),
                });
            }
        };

        // Re-schedule with new config
        if task.enabled {
            if let Err(e) = scheduler.schedule_task(&task).await {
                let mut errors = vec![e.to_string()];
                let db_reverted = store
                    .update_task(
                        &params.id,
                        Some(old_task.name.clone()),
                        Some(old_task.description.clone()),
                        Some(old_task.enabled),
                        Some(old_task.run_if_missed),
                        Some(old_task.notify_on_run),
                        Some(old_task.schedule.clone()),
                        Some(old_task.action.clone()),
                    )
                    .await;
                match db_reverted {
                    Ok(_) => {
                        if old_task.enabled {
                            if let Err(rb_err) = scheduler.schedule_task(&old_task).await {
                                errors.push(format!("rollback re-schedule failed: {}", rb_err));
                                if let Err(dis_err) = store
                                    .update_task(
                                        &params.id,
                                        None,
                                        None,
                                        Some(false),
                                        None,
                                        None,
                                        None,
                                        None,
                                    )
                                    .await
                                {
                                    errors.push(format!(
                                        "disable failed: {} - restart app to fix",
                                        dis_err
                                    ));
                                }
                            }
                        }
                    }
                    Err(rb_err) => {
                        errors.push(format!("rollback failed: {}", rb_err));
                        if let Err(dis_err) = store
                            .update_task(
                                &params.id,
                                None,
                                None,
                                Some(false),
                                None,
                                None,
                                None,
                                None,
                            )
                            .await
                        {
                            errors.push(format!(
                                "disable failed: {} - restart app to fix",
                                dis_err
                            ));
                        }
                    }
                }
                return Err(TaktError::Scheduler {
                    msg: errors.join("; "),
                });
            }
        }
        Ok(task)
    }

    pub async fn delete_task(&self, id: String) -> Result<(), TaktError> {
        let scheduler = self.scheduler()?;
        let store = self.store()?;
        scheduler
            .remove_task(&id)
            .await
            .map_err(|e| TaktError::Scheduler { msg: e.to_string() })?;
        store.delete_task(&id).await?;
        Ok(())
    }

    pub async fn run_task_now(&self, id: String) -> Result<(), TaktError> {
        let store = self.store()?;
        let executor = self.executor()?;
        let task = store
            .get_task(&id)
            .await?
            .ok_or_else(|| TaktError::NotFound {
                msg: "Task not found".to_string(),
            })?;
        scheduler::execute_and_log(
            &***executor,
            store,
            &*self.bridge,
            &id,
            &task.name,
            task.notify_on_run,
            &task.action,
        )
        .await
        .map_err(|msg| TaktError::Execution { msg })
    }

    // ── Logs ────────────────────────────────────────────────────────

    pub async fn list_logs(
        &self,
        task_id: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<ExecutionLog>, TaktError> {
        Ok(self
            .store()?
            .list_logs(task_id.as_deref(), limit.unwrap_or(50).min(500))
            .await?)
    }

    // ── System ──────────────────────────────────────────────────────

    pub fn validate_cron(&self, expression: String) -> Result<(), TaktError> {
        croner::Cron::new(expression.trim())
            .with_seconds_optional()
            .parse()
            .map(|_| ())
            .map_err(|e| TaktError::Validation {
                msg: format!("Invalid cron expression: {}", e),
            })
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build --package libtakt`
Expected: BUILD SUCCESS

- [ ] **Step 5: Write tests for start() idempotency and rollback**

Create `libtakt/src/tests.rs` with:

```rust
#[cfg(test)]
mod tests {
    use crate::error::TaktError;
    use crate::models::{Action, CreateTaskParams, Schedule, Shell};
    use crate::platform::PlatformBridge;
    use crate::TaktCore;
    use std::sync::Arc;

    /// Mock bridge that records calls but does nothing.
    struct MockBridge;

    impl PlatformBridge for MockBridge {
        fn send_notification(&self, _title: String, _body: String, _sound: bool) {}
        fn run_on_main_sync(&self, callback_id: u64) {
            // Execute immediately on current thread (OK for tests)
            crate::platform::execute_callback(callback_id);
        }
    }

    #[tokio::test]
    async fn test_start_is_idempotent() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);

        // First start should succeed
        core.start().await.unwrap();

        // Second start should succeed without creating duplicate resources
        core.start().await.unwrap();

        // Core should be functional
        let tasks = core.list_tasks().await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_methods_fail_before_start() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);

        // Should get NotInitialized error
        let result = core.list_tasks().await;
        assert!(matches!(result, Err(TaktError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);
        core.start().await.unwrap();

        let params = CreateTaskParams {
            name: "Test Task".to_string(),
            description: None,
            run_if_missed: None,
            notify_on_run: None,
            schedule: Schedule::Cron {
                expression: "0 9 * * 1-5".to_string(),
            },
            action: Action::RunCommand {
                command: "echo test".to_string(),
                args: vec![],
                shell: Shell::Sh,
            },
        };

        let task = core.create_task(params).await.unwrap();
        assert_eq!(task.name, "Test Task");
        assert!(task.enabled);

        let tasks = core.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_task() {
        let bridge = Arc::new(MockBridge);
        let core = TaktCore::new(bridge);
        core.start().await.unwrap();

        let params = CreateTaskParams {
            name: "Delete Me".to_string(),
            description: None,
            run_if_missed: None,
            notify_on_run: None,
            schedule: Schedule::DailyFirstUse { delay_minutes: 5 },
            action: Action::Notify {
                title: "Hi".to_string(),
                body: "World".to_string(),
                sound: false,
            },
        };

        let task = core.create_task(params).await.unwrap();
        core.delete_task(task.id).await.unwrap();

        let tasks = core.list_tasks().await.unwrap();
        assert!(tasks.is_empty());
    }
}
```

Register in lib.rs: add `#[cfg(test)] mod tests;` at the bottom.

- [ ] **Step 6: Run all tests including new ones**

Run: `cargo test --package libtakt`
Expected: All tests pass (store, db, executor serialization, AND the new TaktCore tests)

- [ ] **Step 7: Verify static lib is produced**

Run: `cargo build --package libtakt --release --target aarch64-apple-darwin`
Run: `ls -la target/aarch64-apple-darwin/release/liblibtakt.a`
Expected: File exists, several MB in size

- [ ] **Step 8: Commit**

```bash
git add libtakt/src/error.rs libtakt/src/lib.rs libtakt/src/models.rs
git commit -m "feat: add TaktCore with UniFFI exports and full rollback logic from commands.rs"
```

---

## Phase 2 — UniFFI Bindings + Xcode Project

### Task 8: Configure UniFFI and build script

**Files:**
- Create: `libtakt/uniffi.toml`
- Create: `scripts/build-rust.sh`

- [ ] **Step 1: Create uniffi.toml**

```toml
# libtakt/uniffi.toml
[bindings.swift]
module_name = "LibTakt"
ffi_module_name = "LibTaktFFI"
generate_immutable_records = false
```

- [ ] **Step 2: Create build-rust.sh**

```bash
#!/bin/bash
# scripts/build-rust.sh — Build libtakt and generate Swift bindings
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="aarch64-apple-darwin"
PROFILE="${1:-release}"
LIB="$REPO_ROOT/target/$TARGET/$PROFILE/liblibtakt.a"
OUT="$REPO_ROOT/macos/Takt/Generated"

mkdir -p "$OUT" "$OUT/Headers" "$OUT/Modules"

echo "==> Building libtakt ($PROFILE, $TARGET)..."
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt "--$PROFILE" --target "$TARGET"

echo "==> Generating Swift sources..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT" --swift-sources

echo "==> Generating C headers..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT/Headers" --headers

echo "==> Generating modulemap..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT/Modules" --modulemap --modulemap-filename LibTaktFFI.modulemap

echo "==> Done. Output in $OUT"
ls -la "$OUT" "$OUT/Headers" "$OUT/Modules"
```

- [ ] **Step 3: Make script executable**

Run: `chmod +x scripts/build-rust.sh`

- [ ] **Step 4: Run the build script**

Run: `./scripts/build-rust.sh release`
Expected: Script completes, output shows:
- `macos/Takt/Generated/LibTakt.swift` (or similar name)
- `macos/Takt/Generated/Headers/LibTaktFFI.h`
- `macos/Takt/Generated/Modules/LibTaktFFI.modulemap`

If UniFFI rejects any types, fix them in the Rust code and re-run.

- [ ] **Step 5: Commit**

```bash
git add libtakt/uniffi.toml scripts/build-rust.sh
git commit -m "feat: add UniFFI config and build script for Swift binding generation"
```

---

### Task 9: Create Xcode project with minimal app

**Files:**
- Create: `macos/Takt.xcodeproj` (via Xcode)
- Create: `macos/Takt/TaktApp.swift`
- Create: `macos/Takt/TaktCore+Bridge.swift`

This task requires Xcode. Steps:

- [ ] **Step 1: Create Xcode project**

Open Xcode → File → New → Project → macOS → App:
- Product Name: `Takt`
- Organization Identifier: `com.marcusgrando`
- Interface: SwiftUI
- Language: Swift
- Save location: `cronmac/macos/`

Set deployment target to macOS 14.0 in project settings.

- [ ] **Step 2: Configure build settings**

In Xcode project settings → Build Settings:
- **Header Search Paths**: `$(SRCROOT)/Takt/Generated/Headers`
- **Library Search Paths**: `$(SRCROOT)/../target/aarch64-apple-darwin/release`
- **Other Linker Flags**: `-llibtakt -lsqlite3 -framework Security -framework SystemConfiguration -framework CoreGraphics`
- **Swift Compiler - Search Paths → Import Paths**: `$(SRCROOT)/Takt/Generated/Modules`

- [ ] **Step 3: Add Build Phase for Rust**

Build Phases → Add Run Script Phase (before Compile Sources):
- Shell: `/bin/bash`
- Script: `"${SRCROOT}/../scripts/build-rust.sh" release`
- Uncheck "Based on dependency analysis" (always run)

- [ ] **Step 4: Add generated files to project**

Add the generated Swift file to the project:
- Drag `macos/Takt/Generated/LibTakt.swift` (or whatever UniFFI names it) into the Xcode project navigator as a **source file** in the app target (Create groups, NOT folder references). This compiles the bindings as part of the app — no `import LibTakt` needed, all types are available directly.
- Do NOT add the Headers/ or Modules/ directories as source — those are consumed by the build settings (Header Search Paths, Import Paths) configured in Step 2.
- The C header and modulemap enable the Swift compiler to find the FFI symbols in `liblibtakt.a`; the `.swift` file provides the Swift wrappers that call those symbols.

- [ ] **Step 5: Write MacOSPlatformBridge**

```swift
// macos/Takt/TaktCore+Bridge.swift
import Foundation
import UserNotifications
// No `// UniFFI types available directly — generated sources compiled in app target` — generated bindings are compiled as app target sources.
// UniFFI types (TaktCore, PlatformBridgeProtocol, etc.) are available directly.

final class MacOSPlatformBridge: PlatformBridgeProtocol, @unchecked Sendable {
    func sendNotification(title: String, body: String, sound: Bool) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        if sound { content.sound = .default }
        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    func runOnMainSync(callbackId: UInt64) {
        DispatchQueue.main.async {
            executeCallback(callbackId: callbackId)
        }
    }
}
```

- [ ] **Step 6: Write minimal TaktApp.swift**

```swift
// macos/Takt/TaktApp.swift
import SwiftUI

// No `// UniFFI types available directly — generated sources compiled in app target` needed — generated bindings are compiled as app target sources.

@main
struct TaktApp: App {
    @State private var initialized = false

    var body: some Scene {
        MenuBarExtra("Takt", systemImage: "clock") {
            if initialized {
                Text("Takt is running")
            } else {
                ProgressView("Starting...")
            }
        }
        .menuBarExtraStyle(.window)
    }

    init() {
        // Quick smoke test: init core and list tasks
        Task {
            let bridge = MacOSPlatformBridge()
            let core = TaktCore(bridge: bridge)
            try await core.start()
            let tasks = try await core.listTasks()
            print("Takt initialized. \(tasks.count) tasks loaded.")
            await MainActor.run { initialized = true }
        }
    }
}
```

- [ ] **Step 7: Build and run in Xcode**

Press Cmd+R in Xcode.
Expected: App compiles, menu bar icon appears, console prints "Takt initialized. N tasks loaded."

If it crashes: check linker errors (missing frameworks/libraries), or UniFFI protocol mismatches.

- [ ] **Step 8: Commit**

```bash
git add macos/ scripts/
git commit -m "feat: create Xcode project with MacOSPlatformBridge and minimal smoke test"
```

---

## Phase 3 — UI: Popover + Task List

### Task 10: TaskListViewModel and TaskListView

**Files:**
- Create: `macos/Takt/ViewModels/TaskListViewModel.swift`
- Create: `macos/Takt/Views/TaskListView.swift`
- Create: `macos/Takt/Views/TaskItemView.swift`
- Modify: `macos/Takt/TaktApp.swift`

- [ ] **Step 1: Create TaskListViewModel**

```swift
// macos/Takt/ViewModels/TaskListViewModel.swift
import Foundation
// UniFFI types available directly — generated sources compiled in app target

@Observable
final class TaskListViewModel {
    let core: TaktCore
    var tasks: [TaskDto] = []
    var isLoading = false
    var error: String?

    init(core: TaktCore) {
        self.core = core
    }

    func refresh() async {
        isLoading = true
        defer { isLoading = false }
        do {
            tasks = try await core.listTasks()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    func toggleEnabled(_ task: TaskDto) async {
        do {
            let params = UpdateTaskParams(
                id: task.id,
                name: nil,
                description: nil,
                enabled: !task.enabled,
                runIfMissed: nil,
                notifyOnRun: nil,
                schedule: nil,
                action: nil
            )
            _ = try await core.updateTask(params: params)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func deleteTask(_ id: String) async {
        do {
            try await core.deleteTask(id: id)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func runNow(_ id: String) async {
        do {
            try await core.runTaskNow(id: id)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }
}
```

- [ ] **Step 2: Create TaskItemView**

```swift
// macos/Takt/Views/TaskItemView.swift
import SwiftUI
// UniFFI types available directly — generated sources compiled in app target

struct TaskItemView: View {
    let task: TaskDto
    let onToggle: () -> Void
    let onRun: () -> Void
    let onEdit: () -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Toggle("", isOn: Binding(
                get: { task.enabled },
                set: { _ in onToggle() }
            ))
            .toggleStyle(.switch)
            .controlSize(.small)

            VStack(alignment: .leading, spacing: 2) {
                Text(task.name)
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)
                if let next = task.nextRunAt {
                    Text(next)
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()

            Button(action: onRun) {
                Image(systemName: "play.fill")
                    .font(.system(size: 10))
            }
            .buttonStyle(.plain)
            .help("Run now")

            Button(action: onEdit) {
                Image(systemName: "pencil")
                    .font(.system(size: 10))
            }
            .buttonStyle(.plain)
            .help("Edit")
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .contextMenu {
            Button("Run Now", action: onRun)
            Button("Edit", action: onEdit)
            Divider()
            Button("Delete", role: .destructive, action: onDelete)
        }
    }
}
```

- [ ] **Step 3: Create TaskListView**

```swift
// macos/Takt/Views/TaskListView.swift
import SwiftUI
// UniFFI types available directly — generated sources compiled in app target

struct TaskListView: View {
    @Bindable var vm: TaskListViewModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Tasks")
                    .font(.headline)
                Spacer()
                Button(action: { /* open template grid or editor */ }) {
                    Image(systemName: "plus")
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider()

            // Task list
            if vm.isLoading && vm.tasks.isEmpty {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if vm.tasks.isEmpty {
                ContentUnavailableView("No Tasks", systemImage: "clock")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(vm.tasks, id: \.id) { task in
                            TaskItemView(
                                task: task,
                                onToggle: { Task { await vm.toggleEnabled(task) } },
                                onRun: { Task { await vm.runNow(task.id) } },
                                onEdit: { /* TODO: open editor window - Task 12 */ },
                                onDelete: { Task { await vm.deleteTask(task.id) } }
                            )
                            Divider()
                        }
                    }
                }
            }

            if let error = vm.error {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(8)
            }
        }
        .frame(width: 280, height: 400)
        .task {
            await vm.refresh()
        }
    }
}
```

- [ ] **Step 4: Update TaktApp.swift**

Replace the minimal version with the real app structure:

```swift
// macos/Takt/TaktApp.swift
import SwiftUI

// NOTE: Do NOT use `// UniFFI types available directly — generated sources compiled in app target` — the generated Swift bindings are compiled
// as part of the app target (added as source files), not as a separate module.
// All UniFFI-generated types (TaktCore, TaskDto, etc.) are available directly.

@main
struct TaktApp: App {
    // Core is initialized eagerly at app launch via AppDelegate, NOT lazily
    // inside the MenuBarExtra body. This ensures the scheduler, catch-up logic,
    // and auto-start are active immediately — not deferred to first popover open.
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        MenuBarExtra("Takt", systemImage: "clock") {
            if let vm = appDelegate.vm {
                TaskListView(vm: vm)
            } else {
                ProgressView("Starting...")
                    .frame(width: 280, height: 400)
            }
        }
        .menuBarExtraStyle(.window)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var vm: TaskListViewModel?
    private var core: TaktCore?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Hide from dock immediately
        NSApp.setActivationPolicy(.accessory)

        // Initialize core eagerly — scheduler starts at launch, not on first popover open
        Task {
            let bridge = MacOSPlatformBridge()
            let core = TaktCore(bridge: bridge)
            do {
                try await core.start()
                self.core = core
                await MainActor.run {
                    self.vm = TaskListViewModel(core: core)
                }
            } catch {
                print("Failed to initialize: \(error)")
            }
        }
    }
}
```

- [ ] **Step 5: Build and run**

Press Cmd+R in Xcode.
Expected: Menu bar icon appears, clicking shows popover with task list (or empty state).

- [ ] **Step 6: Commit**

```bash
git add macos/Takt/
git commit -m "feat: add TaskListView with toggle, run, delete functionality"
```

---

## Phase 4 — UI: Editor

### Task 11: Port CronUtils and AutoName to Swift

**Files:**
- Create: `macos/Takt/Helpers/CronUtils.swift`
- Create: `macos/Takt/Helpers/AutoName.swift`

These are mechanical ports of `src/components/schedule/cron-utils.ts` (219 lines) and `src/components/AutoName.ts` (93 lines). The logic is pure — no UI, no platform dependencies.

- [ ] **Step 1: Port cron-utils.ts to CronUtils.swift**

Port the types (`FrequencyType`, `IntervalUnit`, `MonthlyMode`, `OrdinalPosition`, `RecurringState`) and functions (`buildCron`, `parseCron`) from `src/components/schedule/cron-utils.ts`. Use Swift enums and structs. Keep the same logic and edge case handling.

Reference: `src/components/schedule/cron-utils.ts` (full source read earlier in this plan).

- [ ] **Step 2: Port AutoName.ts to AutoName.swift**

Port `describeAction`, `describeSchedule`, and `generateAutoName` from `src/components/AutoName.ts`. These use the UniFFI-generated `Action` and `Schedule` types. The Swift switch exhaustiveness checking ensures no variant is missed.

Reference: `src/components/AutoName.ts` (full source read earlier in this plan).

- [ ] **Step 3: Verify they compile**

Build in Xcode. Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add macos/Takt/Helpers/
git commit -m "feat: port CronUtils and AutoName helpers from TypeScript to Swift"
```

---

### Task 12: Editor views and ViewModel

**Files:**
- Create: `macos/Takt/ViewModels/TaskEditorViewModel.swift`
- Create: `macos/Takt/Views/TaskEditorView.swift`
- Create: `macos/Takt/Views/ActionBuilderView.swift`
- Create: `macos/Takt/Views/ScheduleBuilderView.swift`
- Create: `macos/Takt/Views/Schedule/DayGridView.swift`
- Create: `macos/Takt/Views/Schedule/WeekdayGridView.swift`
- Create: `macos/Takt/Views/Schedule/TimeFieldView.swift`
- Create: `macos/Takt/Views/TemplateGridView.swift`
- Modify: `macos/Takt/TaktApp.swift` (add Window scene)

This is the largest task — mechanical port of the React editor to SwiftUI. Reference the React components for exact field layouts and behavior:

- `src/components/TaskEditor.tsx` — form structure, dirty detection
- `src/components/ActionBuilder.tsx` — 7 action types with their fields
- `src/components/ScheduleBuilder.tsx` — 3 schedule types, cron visual builder
- `src/components/schedule/DayGrid.tsx` — 7x5 grid of day buttons
- `src/components/schedule/WeekdayGrid.tsx` — 7 weekday toggle buttons
- `src/components/schedule/TimeField.tsx` — hour/minute inputs
- `src/components/TemplateGrid.tsx` — 7 template cards

- [ ] **Step 1: Create TaskEditorViewModel**

Key responsibilities:
- Hold all form fields as `@Observable` properties
- `isDirty` computed from snapshot comparison
- `autoNameEnabled` flag, `displayName` uses `AutoName.generateAutoName()`
- `save()` calls `core.createTask()` or `core.updateTask()`
- `listBrowsers()` and `listAppsForFile()` via NSWorkspace on `@MainActor`

- [ ] **Step 2: Create ActionBuilderView**

Segmented Picker for 7 action types. Each type shows its fields:
- OpenUrl: url TextField, optional browser Picker
- OpenFile: path TextField with `.fileImporter`, optional app Picker
- OpenApp: app_path TextField with `.fileImporter`
- RunCommand: command TextEditor, args, shell Picker
- Notify: title, body, sound Toggle
- Webhook: url, method Picker, headers, body
- Settings: pane_url TextField

- [ ] **Step 3: Create schedule sub-views**

- `DayGridView`: `LazyVGrid(columns: 7)` of toggle buttons for days 1-31
- `WeekdayGridView`: `HStack` of 7 toggle buttons (Sun-Sat)
- `TimeFieldView`: `DatePicker(selection:, displayedComponents: .hourAndMinute)`

- [ ] **Step 4: Create ScheduleBuilderView**

Segmented Picker for 3 schedule types:
- Cron: frequency Picker (interval/daily/weekly/monthly/custom), sub-views based on frequency
- OneShot: DatePicker for run_at
- DailyFirstUse: delay_minutes Stepper

- [ ] **Step 5: Create TaskEditorView**

Form with:
- Name TextField (disabled when autoNameEnabled)
- Auto-name Toggle
- Description TextField
- ActionBuilderView
- ScheduleBuilderView
- run_if_missed Toggle
- notify_on_run Toggle
- Save/Cancel buttons
- `.confirmationDialog` for unsaved changes on dismiss

- [ ] **Step 6: Create TemplateGridView**

`LazyVGrid(columns: 2)` with 7 template cards matching `src/components/TemplateGrid.tsx`.

- [ ] **Step 7: Add Window scene and editor opening to TaktApp.swift**

Add `Window("Editor", id: "editor", for: EditorParams.self)` scene. Wire the "+" button and edit button to `openWindow(id: "editor", value: params)`.

Handle activation policy: set to `.regular` when editor opens, `.accessory` when last editor closes.

- [ ] **Step 8: Build and test**

Build in Xcode. Create a task with each of the 7 action types and 3 schedule types. Verify save/load round-trips correctly.

- [ ] **Step 9: Commit**

```bash
git add macos/Takt/
git commit -m "feat: add complete task editor with all action and schedule types"
```

---

## Phase 5 — History + Polish

### Task 13: History view

**Files:**
- Create: `macos/Takt/Views/HistoryView.swift`
- Modify: `macos/Takt/Views/TaskListView.swift` (add view switching)

- [ ] **Step 1: Create HistoryView**

Port from `src/components/HistoryView.tsx`. Use `DisclosureGroup` for expand/collapse. Show status badge, timestamps, stdout/stderr/error.

- [ ] **Step 2: Add view switching to TaskListView**

Add a segmented control or tab bar to switch between Tasks and History views, matching the current React `App.tsx` behavior.

- [ ] **Step 3: Build and test**

- [ ] **Step 4: Commit**

```bash
git add macos/Takt/Views/
git commit -m "feat: add execution history view with expandable entries"
```

---

### Task 14: System integration (permissions, auto-start, single instance)

**Files:**
- Modify: `macos/Takt/TaktApp.swift`

- [ ] **Step 1: Add accessibility permission request**

```swift
// In TaktApp initialization
func requestAccessibility() {
    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true] as CFDictionary
    AXIsProcessTrustedWithOptions(options)
}
```

- [ ] **Step 2: Add notification permission request**

```swift
UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
```

- [ ] **Step 3: Add single instance enforcement**

```swift
// In TaktApp.init or an AppDelegate
func killPreviousInstances() {
    let bundleId = Bundle.main.bundleIdentifier ?? ""
    let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId)
    let myPID = ProcessInfo.processInfo.processIdentifier
    for app in running where app.processIdentifier != myPID {
        app.forceTerminate()
    }
}
```

- [ ] **Step 4: Add keyboard shortcuts**

Add `.keyboardShortcut("n", modifiers: .command)` to the new task button, `.keyboardShortcut("w", modifiers: .command)` to close editor.

- [ ] **Step 5: Add app icon**

Add icon assets to `Assets.xcassets`. Create a tray icon as a template image.

- [ ] **Step 6: Build and test full feature parity**

Test all features:
- Menu bar icon appears
- Popover shows task list
- Create/edit/delete tasks
- All 7 action types work
- All 3 schedule types work
- History shows execution logs
- Notifications fire
- Auto-start on login
- Single instance
- Keyboard shortcuts

- [ ] **Step 7: Commit**

```bash
git add macos/
git commit -m "feat: add system integration (permissions, auto-start, shortcuts, app icon)"
```

---

## Phase 6 — Cleanup

### Task 15: Remove Tauri and React code

**Files:**
- Delete: `src-tauri/` (entire directory)
- Delete: `src/` (entire directory)
- Delete: `package.json`, `bun.lock`, `vite.config.ts`, `tsconfig.json`
- Delete: `index.html`, `editor.html`
- Delete: `tailwind.config.*`, `postcss.config.*` (if present)
- Delete: `components.json` (shadcn config, if present)
- Modify: `.gitignore`

- [ ] **Step 1: Delete old frontend and Tauri**

```bash
rm -rf src-tauri/ src/
rm -f package.json bun.lock vite.config.ts tsconfig.json
rm -f index.html editor.html
rm -f components.json tailwind.config.* postcss.config.*
```

- [ ] **Step 2: Update .gitignore**

Add:
```
# UniFFI generated bindings
macos/Takt/Generated/

# Xcode
macos/*.xcodeproj/xcuserdata/
macos/*.xcodeproj/project.xcworkspace/xcuserdata/
```

Remove any node_modules, dist, or Tauri-specific entries.

- [ ] **Step 3: Verify clean build**

Run: `cargo build --package libtakt --release --target aarch64-apple-darwin`
Run: Xcode Cmd+Shift+K (clean), then Cmd+B (build)
Expected: Both succeed

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove Tauri+React code, migration complete"
```

---

## Summary

| Phase | Tasks | Key deliverable |
|-------|-------|-----------------|
| Phase 1 | Tasks 1-7 | `libtakt` crate compiles, all tests pass, produces `liblibtakt.a` |
| Phase 2 | Tasks 8-9 | Swift bindings generated, Xcode project builds, smoke test passes |
| Phase 3 | Task 10 | Menu bar popover with functional task list (toggle/run/delete) |
| Phase 4 | Tasks 11-12 | Full editor with 7 action types, 3 schedule types, cron visual builder |
| Phase 5 | Tasks 13-14 | History view, permissions, auto-start, keyboard shortcuts, app icon |
| Phase 6 | Task 15 | Old code removed, clean build |
