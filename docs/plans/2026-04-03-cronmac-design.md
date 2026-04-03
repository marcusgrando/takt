# cronmac — Design Document

**Date:** 2026-04-03  
**Status:** Approved  
**Stack:** Tauri 2 + React + TypeScript + Rust

---

## Overview

**cronmac** is a macOS menu bar app for scheduling and automating actions — opening files/apps, running scripts, opening URLs, sending notifications, simulating keyboard shortcuts, and firing webhooks. It lives exclusively in the macOS menu bar (no Dock icon) and starts automatically via Launch Agent on login.

The architecture is designed to be cross-platform from the start: macOS is the primary target, but a Windows port should require only adding a `WindowsExecutor` implementation without touching the core scheduler or UI.

---

## Target Audience

All user types: power users, developers/DevOps, and non-technical end users. The UI is visual and WYSIWYG (no cron syntax exposed), while supporting the full power of cron-style recurrence internally.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  cronmac — Tauri 2 App (Menu Bar Only, no Dock)             │
│                                                             │
│  ┌─────────────┐    IPC (invoke/events)    ┌─────────────┐  │
│  │  React UI   │◀─────────────────────────▶│  Rust Core  │  │
│  │  (Webview)  │                           │             │  │
│  └─────────────┘                           │ ┌─────────┐ │  │
│                                            │ │Scheduler│ │  │
│  Menu Bar Icon ──▶ Popover Window          │ │ (tokio- │ │  │
│  (click opens floating window)             │ │  cron)  │ │  │
│                                            │ └────┬────┘ │  │
│                                            │      │      │  │
│                                            │ ┌────▼────┐ │  │
│                                            │ │Executor │ │  │
│                                            │ │  Trait  │ │  │
│                                            │ └────┬────┘ │  │
│                                            │      │      │  │
│                                            │ ┌────▼──────┐  │
│                                            │ │MacosExecut│  │
│                                            │ │(open/shell│  │
│                                            │ │AppleScript│  │
│                                            │ └───────────┘  │
│                                            └─────────────┘  │
│                                                             │
│  Storage: SQLite (sqlx) in ~/Library/Application Support    │
│  Launch Agent: ~/.config/launchd plist auto-registered      │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

| Component | Technology | Responsibility |
|---|---|---|
| UI | React + TypeScript + Vite | Task management, schedule builder, history view |
| State | @tanstack/query | Data fetching/caching via Tauri IPC |
| Backend | Rust (Tauri 2) | IPC router, scheduler, executor dispatch |
| Scheduler | tokio-cron-scheduler | Job management (cron, one-shot, event-triggered) |
| Executor | `ActionExecutor` trait | Platform-specific action dispatch |
| MacosExecutor | Rust + `open`, `osascript` | macOS implementation of all action types |
| Storage | SQLite via sqlx | Task persistence, execution history, logs |

---

## Platform Abstraction Layer

The key architectural decision for future cross-platform support is a `ActionExecutor` trait:

```rust
pub trait ActionExecutor: Send + Sync {
    fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError>;
    fn platform(&self) -> Platform;
}

pub struct MacosExecutor;
// Future: pub struct WindowsExecutor;

impl ActionExecutor for MacosExecutor {
    fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile { path }    => macos::open_file(path),
            Action::OpenUrl { url, .. }  => macos::open_url(url),
            Action::RunCommand { .. }    => macos::run_shell(..),
            Action::Notify { .. }        => macos::notify(..),
            Action::Shortcut { keys }    => macos::send_keys(keys),
            Action::Webhook { .. }       => http::send_request(..),  // cross-platform
            Action::SystemControl { c }  => macos::system_control(c),
        }
    }
}
```

macOS-specific implementations:
- `OpenFile` → `open <path>`
- `OpenUrl` → `open <url>` or `open -a <browser> <url>`
- `RunCommand` → `sh -c <cmd>` or `osascript -e <script>`
- `Notify` → macOS Notification Center via `osascript` or `tauri-plugin-notification`
- `Shortcut` → `osascript` with keystroke commands
- `Webhook` → native `reqwest` HTTP client (works on all platforms)

---

## Data Model

```rust
struct Task {
    id: Uuid,
    name: String,
    description: Option<String>,
    enabled: bool,
    schedule: Schedule,
    action: Action,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_run_at: Option<DateTime<Utc>>,
    next_run_at: Option<DateTime<Utc>>,
}

enum Schedule {
    Cron { expression: String },          // internal cron expr, built by visual UI
    OneShot { run_at: DateTime<Utc> },
    OnLogin,
    OnWake,                               // system wake from sleep
}

enum Action {
    OpenFile   { path: String },
    OpenUrl    { url: String, browser: Option<String> },
    RunCommand { command: String, args: Vec<String>, shell: Shell },
    Notify     { title: String, body: String, sound: bool },
    Shortcut   { keys: Vec<Key> },
    Webhook    { url: String, method: HttpMethod, headers: HashMap<String, String>, body: Option<String> },
    SystemControl { control: SystemControlKind },
}

enum Shell { Sh, Bash, Zsh, Python, AppleScript }
enum SystemControlKind { SetVolume(u8), Mute, Unmute, SetBrightness(u8), DoNotDisturb(bool) }

struct ExecutionLog {
    id: Uuid,
    task_id: Uuid,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    status: ExecutionStatus,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

enum ExecutionStatus { Success, Failure, Skipped }
```

---

## UI / UX

### Menu Bar Popover (280x400px)

```
[⏱] ← menu bar icon
      ▾ click opens popover
      ┌──────────────────────────────┐
      │  cronmac             [+ New] │
      │ ─────────────────────────── │
      │ ● Abrir Spotify     08:00   │
      │   Every weekday             │
      │ ● Daily standup     09:30   │
      │   Every weekday             │
      │ ○ Backup (disabled)         │
      │ ─────────────────────────── │
      │ [History]       [Settings]  │
      └──────────────────────────────┘
```

### Create/Edit Task — 3-Step Wizard (600x500px modal)

**Step 1 — Schedule**
- Task name
- Recurrence: visual day-of-week picker + time selector (no cron syntax exposed)
- Or: one-shot date/time picker
- Or: event trigger (on login, on wake)

**Step 2 — Action**
- Action type selector (icons + labels)
- Dynamic form for action-specific fields
- Test button: run the action immediately

**Step 3 — Review**
- Summary of task name + schedule + action
- Preview: "Next run: Monday Apr 6 at 08:00"
- Save button

### History View (full-size window, 700x500px)

- Filterable list of past executions per task
- Status indicators (success/failure)
- Expandable rows with stdout/stderr output
- Clear history option

---

## Scheduling

- `tokio-cron-scheduler` handles recurrent cron jobs in async Rust
- One-shot tasks use `tokio::time::sleep_until`
- `OnLogin` / `OnWake` tasks are dispatched via macOS Launch Agent callbacks or NSWorkspace notifications via Tauri plugin
- All active jobs are loaded from SQLite on app startup and re-registered with the scheduler
- Enabled/disabled state is respected — disabled tasks are stored but not scheduled

---

## Persistence

- SQLite file at `~/Library/Application Support/cronmac/cronmac.db`
- Schema migrations via `sqlx migrate`
- Tables: `tasks`, `execution_logs`
- Tauri's `sqlx` integration with async access

---

## Launch Agent (Auto-start)

On first launch, cronmac registers itself as a macOS Launch Agent:
- Writes `~/Library/LaunchAgents/com.cronmac.app.plist`
- Runs `launchctl load` to activate it
- Settings toggle to disable auto-start (unloads and removes plist)

---

## Error Handling

- Each task execution is wrapped in a `catch_unwind`-style handler
- Failed tasks log the error to `execution_logs`, never crash the app
- Notification shown for failures (configurable per-task)
- IPC errors returned as structured `Result<T, AppError>` with typed error variants

---

## Testing Strategy

- **Rust unit tests**: `ActionExecutor` implementations tested with mocks
- **Integration tests**: Scheduler dispatch tested with in-memory SQLite
- **UI tests**: Vitest + Testing Library for React components
- `MacosExecutor` and `WindowsExecutor` are conditionally compiled and tested on their respective CI runners

---

## Future — Windows Port

To add Windows support:
1. Implement `WindowsExecutor` mapping the same `Action` enum to Windows-native APIs:
   - `OpenFile` → `start <path>`
   - `OpenUrl` → `start <url>`
   - `RunCommand` → PowerShell or cmd
   - `Shortcut` → `SendInput` via winapi crate
   - `SystemControl` → Windows API calls
2. Register the correct executor at runtime via `#[cfg(target_os)]`
3. Replace Launch Agent registration with Windows Task Scheduler or registry Run key
4. No changes needed to: scheduler, data model, React UI, SQLite, IPC layer

---

## Out of Scope (v1)

- Cloud sync of tasks across machines
- Import/export of task configurations
- CLI interface
- iOS/Android
- Plugin system for custom action types
