# Takt: Migration from Tauri+React to SwiftUI+Rust FFI

**Date:** 2026-04-06
**Status:** Draft
**Author:** Marcus Grando + Claude

## Summary

Migrate Takt (macOS menu bar task scheduler) from Tauri+React to a native SwiftUI app backed by a Rust core library (`libtakt`) exposed via UniFFI. Follows the Ghostty pattern: ~90% of logic in Rust, thin native shell per platform.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| FFI strategy | UniFFI (Mozilla) | Generates Swift bindings now, C# for Windows later |
| Migration strategy | Clean cut | No parallel Tauri maintenance, simpler execution |
| Windows support | Later, macOS only now | YAGNI — UniFFI ensures portability when needed |
| macOS minimum | 14.0 (Sonoma) | Required for `@Observable`, `@Bindable` |

## Architecture

### Project Structure

```
cronmac/
├── libtakt/                    # Rust core (library crate)
│   ├── src/
│   │   ├── lib.rs              # UniFFI exports + TaktCore
│   │   ├── models.rs           # Task, Action, Schedule (reuse, add UniFFI derives)
│   │   ├── store.rs            # TaskStore — SQLx (reuse, no changes)
│   │   ├── db.rs               # SQLite connection (reuse, no changes)
│   │   ├── scheduler.rs        # AppScheduler (refactored: AppHandle → PlatformBridge)
│   │   ├── executor/
│   │   │   ├── mod.rs          # ActionExecutor trait (refactored: AppHandle → PlatformBridge)
│   │   │   ├── macos.rs        # macOS impl (refactored: 3 substitutions)
│   │   │   └── keymap.rs       # Key mapping (reuse, no changes)
│   │   ├── platform.rs         # NEW: PlatformBridge trait (UniFFI callback interface)
│   │   └── launch_agent.rs     # Reuse, no changes
│   ├── migrations/             # Copied from src-tauri/migrations/
│   ├── Cargo.toml
│   └── uniffi.toml
│
├── macos/                      # Xcode project (SwiftUI app)
│   ├── Takt/
│   │   ├── TaktApp.swift       # @main, MenuBarExtra, Window scenes
│   │   ├── TaktCore+Bridge.swift # PlatformBridge impl + TaktCore wrapper
│   │   ├── Views/
│   │   │   ├── TaskListView.swift
│   │   │   ├── TaskItemView.swift
│   │   │   ├── TaskEditorView.swift
│   │   │   ├── ActionBuilderView.swift
│   │   │   ├── ScheduleBuilderView.swift
│   │   │   ├── HistoryView.swift
│   │   │   ├── TemplateGridView.swift
│   │   │   └── Schedule/
│   │   │       ├── DayGridView.swift
│   │   │       ├── WeekdayGridView.swift
│   │   │       └── TimeFieldView.swift
│   │   ├── ViewModels/
│   │   │   ├── TaskListViewModel.swift
│   │   │   └── TaskEditorViewModel.swift
│   │   ├── Helpers/
│   │   │   ├── CronUtils.swift     # Port of cron-utils.ts
│   │   │   └── AutoName.swift      # Port of AutoName.ts
│   │   ├── Generated/              # UniFFI output (gitignored)
│   │   │   ├── libtakt.swift
│   │   │   ├── libtaktFFI.h
│   │   │   └── libtaktFFI.modulemap
│   │   └── Assets.xcassets
│   └── Takt.xcodeproj
│
├── scripts/
│   └── build-rust.sh           # Builds libtakt + generates UniFFI bindings
│
├── Cargo.toml                  # Workspace root
└── docs/
```

### Data Flow

```
SwiftUI View
    ↓ user action
ViewModel (@Observable)
    ↓ async call
UniFFI-generated Swift bindings
    ↓ FFI call
libtakt::TaktCore (Rust)
    ↓ business logic
Store / Scheduler / Executor
    ↓ platform callback (via PlatformBridge)
Swift PlatformBridge implementation
    ↓
macOS native APIs (UNNotification, DispatchQueue.main)
```

## Rust Core: libtakt

### PlatformBridge Trait

Replaces the two uses of `tauri::AppHandle`:

```rust
#[uniffi::export(callback_interface)]
pub trait PlatformBridge: Send + Sync {
    /// Send a native notification
    fn send_notification(&self, title: String, body: String, sound: bool);

    /// Execute a registered callback on the main thread (required for AppKit APIs).
    /// Rust registers the callback, Swift executes it by ID on DispatchQueue.main.
    fn run_on_main_sync(&self, callback_id: u64);
}
```

Note: UniFFI callback interfaces don't support generic closures. Main thread dispatch uses a callback registry pattern:

1. Rust stores the closure in a `static Mutex<HashMap<u64, Box<dyn FnOnce()>>>` registry with a numeric ID
2. Rust calls `bridge.run_on_main_sync(id)` — this crosses FFI into Swift
3. Swift calls `DispatchQueue.main.async { TaktCallbackRegistry.execute(id) }` — dispatches to main thread
4. `TaktCallbackRegistry.execute(id)` crosses FFI back into Rust, pops the closure from the registry, and runs it
5. Rust signals completion via a `oneshot` channel that the original caller is awaiting

**Deadlock prevention:** Swift MUST use `DispatchQueue.main.async` (not `.sync`) because the calling Rust thread may itself be the main thread in some scenarios. The Rust side awaits a `tokio::sync::oneshot` channel for completion, which does not block the OS thread.

### TaktCore (UniFFI exported object)

```rust
#[derive(uniffi::Object)]
pub struct TaktCore {
    store: Arc<TaskStore>,
    scheduler: Arc<AppScheduler>,
    executor: Arc<Box<dyn ActionExecutor>>,
}

#[uniffi::export]
impl TaktCore {
    #[uniffi::constructor]
    pub async fn new(bridge: Arc<dyn PlatformBridge>) -> Result<Self, TaktError>;

    // Task CRUD
    pub async fn list_tasks(&self) -> Result<Vec<TaskDto>, TaktError>;
    pub async fn get_task(&self, id: String) -> Result<Option<TaskDto>, TaktError>;
    pub async fn create_task(&self, params: CreateTaskParams) -> Result<TaskDto, TaktError>;
    pub async fn update_task(&self, params: UpdateTaskParams) -> Result<TaskDto, TaktError>;
    pub async fn delete_task(&self, id: String) -> Result<(), TaktError>;
    pub async fn run_task_now(&self, id: String) -> Result<(), TaktError>;

    // Logs
    pub async fn list_logs(&self, task_id: Option<String>, limit: Option<i64>) -> Result<Vec<ExecutionLog>, TaktError>;

    // System
    pub fn list_browsers(&self) -> Vec<String>;
    pub fn list_apps_for_file(&self, path: String) -> Vec<String>;
    pub fn validate_cron(&self, expression: String) -> Result<(), TaktError>;
}
```

### Module-by-Module Changes

| Module | Change | Details |
|--------|--------|---------|
| `models.rs` | Add derives | `#[derive(uniffi::Record)]` on structs, `#[derive(uniffi::Enum)]` on enums. Serde derives kept for DB JSON serialization. |
| `store.rs` | None | Pure SQLx, no Tauri dependency. |
| `db.rs` | None | Pure SQLx + dirs crate. Same DB path `~/Library/Application Support/takt/`. |
| `launch_agent.rs` | None | Uses `std::process::Command`. |
| `executor/mod.rs` | Small | `current_executor(app_handle)` → `current_executor(bridge: Arc<dyn PlatformBridge>)` |
| `executor/macos.rs` | Medium | 3 substitutions: `app.run_on_main_thread()` → `bridge.run_on_main_sync()`, `app.notification()` → `bridge.send_notification()`, `MacosExecutor { app_handle }` → `MacosExecutor { bridge }` |
| `executor/keymap.rs` | None | Pure key code mapping. |
| `scheduler.rs` | Medium | `app_handle: tauri::AppHandle` → `bridge: Arc<dyn PlatformBridge>`. `send_run_notification()` uses bridge. |
| `commands.rs` | **Delete** | Replaced by `TaktCore` UniFFI exports. |
| `lib.rs` | **Rewrite** | Was Tauri setup. Now `TaktCore` struct + UniFFI exports. |
| `platform.rs` | **New** | `PlatformBridge` trait definition (~15 lines). |

### Cargo.toml Changes

Removed:
- `tauri`, `tauri-build`, `tauri-plugin-notification`, `tauri-plugin-shell`, `tauri-plugin-dialog`

Added:
- `uniffi = { version = "0.28", features = ["cli"] }`

Kept:
- `tokio`, `sqlx`, `chrono`, `serde`, `serde_json`, `uuid`, `croner`, `tokio-cron-scheduler`, `tokio-util`, `reqwest`, `anyhow`, `thiserror`, `async-trait`, `dirs`, `objc2`, `objc2-foundation`, `objc2-app-kit`, `block2`, `core-graphics`

## SwiftUI App

### React → SwiftUI Component Mapping

| React Component | SwiftUI Equivalent | Notes |
|---|---|---|
| `App.tsx` (popover hack) | `MenuBarExtra(.window)` | Native popover, no positioning code |
| `TaskList.tsx` + React Query | `TaskListView` + `@Observable` VM | `async/await` replaces React Query |
| `TaskItem.tsx` | `TaskItemView` | `Toggle`, contextMenu |
| Editor (dynamic webview) | `Window(id:for:)` + `openWindow` | Native multi-window |
| `TaskEditor.tsx` | `TaskEditorView` + VM | `@Observable` + dirty tracking |
| `ActionBuilder.tsx` (7 tabs) | `ActionBuilderView` + segmented Picker | Each type is a ViewBuilder section |
| `ScheduleBuilder.tsx` | `ScheduleBuilderView` + segmented Picker | 3 schedule types |
| `DayGrid.tsx` | `LazyVGrid(columns: 7)` | Toggle buttons |
| `WeekdayGrid.tsx` | `HStack` of 7 toggles | Trivial |
| `TimeField.tsx` | `DatePicker(.hourAndMinute)` | Native, 1 line |
| `HistoryView.tsx` | `HistoryView` + `DisclosureGroup` | Native expand/collapse |
| `TemplateGrid.tsx` | `LazyVGrid(columns: 2)` | Card buttons |
| Theme (CSS) | Automatic | SwiftUI follows system |
| `set_activation_policy` (IPC) | `NSApp.setActivationPolicy()` | Direct call, no IPC |
| Unsaved changes dialog | `.confirmationDialog` | Native |
| File picker (Tauri plugin) | `.fileImporter()` | Native, 1 modifier |

### Key SwiftUI Components

**TaktApp.swift** — Entry point:
- `MenuBarExtra` with `.menuBarExtraStyle(.window)` — replaces ~100 lines of tray/popover setup
- `Window("Editor", id: "editor", for: EditorParams.self)` — replaces dynamic webview creation
- Activation policy management: accessor mode by default, regular when editor opens

**TaskListViewModel** — Replaces React Query:
- `@Observable` class with `tasks: [TaskDto]`, `isLoading`, `error`
- `refresh()`, `toggle()`, `delete()`, `runNow()` as async methods
- Holds `TaktCore` instance (shared with editor via environment)

**TaskEditorViewModel** — Form state:
- All form fields as `@Observable` properties
- Dirty detection via snapshot comparison
- Auto-name generation from action + schedule
- Unsaved changes confirmation on dismiss

**MacOSPlatformBridge** — Implements UniFFI callback interface:
- `sendNotification()` → `UNUserNotificationCenter`
- `runOnMainSync(callbackId:)` → `DispatchQueue.main.sync` + callback registry

### Complexity Removed

The following no longer needed:
- Vite config + 2 HTML entry points
- `node_modules/` (~200MB)
- Tailwind CSS + all CSS
- React Query cache management
- shadcn/Radix UI component library
- `theme.ts` + CSS media queries
- `show_near_tray()` positioning hack
- `kill_previous_instance()` in Rust (moves to Swift)

## Build System

### Cargo Workspace

```toml
# cronmac/Cargo.toml
[workspace]
members = ["libtakt"]
```

### Build Script (`scripts/build-rust.sh`)

1. `cargo build --release --target aarch64-apple-darwin` → produces `liblibtakt.a`
2. `uniffi-bindgen generate` → produces `libtakt.swift`, `libtaktFFI.h`, `libtaktFFI.modulemap`
3. Output copied to `macos/Takt/Generated/`

### Xcode Integration

- **Build Phase** (Run Script, before Compile Sources): calls `build-rust.sh`
- **Header Search Paths**: `$(SRCROOT)/Takt/Generated`
- **Library Search Paths**: `$(SRCROOT)/../libtakt/target/aarch64-apple-darwin/release`
- **Other Linker Flags**: `-lliblibtakt -lsqlite3 -framework Security -framework SystemConfiguration`
- **Module Map**: points to `libtaktFFI.modulemap`

### Signing & Distribution

- Developer ID: `Developer ID Application: Marcus Nestor Alves Grando (MY427949GW)`
- Bundle ID: `com.marcusgrando.takt` (unchanged)
- Notarization: `xcrun notarytool`
- DB path: `~/Library/Application Support/takt/takt.db` (backward compatible)

## Migration Phases

### Phase 1 — Create libtakt (Rust core)

Extract Rust code from `src-tauri/src/` to `libtakt/src/`, remove all Tauri dependencies, add UniFFI.

Deliverables:
1. New crate `libtakt/` with workspace config
2. `platform.rs` — `PlatformBridge` trait with UniFFI callback interface
3. `lib.rs` — `TaktCore` struct with all UniFFI exports
4. `executor/macos.rs` refactored (AppHandle → PlatformBridge)
5. `scheduler.rs` refactored (AppHandle → PlatformBridge)
6. `commands.rs` deleted (replaced by TaktCore methods)
7. `models.rs` with UniFFI derives added
8. Existing tests passing (store, db)
9. New test: TaktCore with mock PlatformBridge

Validation: `cargo build --release` produces `liblibtakt.a`, `cargo test` passes.

### Phase 2 — UniFFI Bindings + Xcode Project

Configure UniFFI, generate Swift bindings, create minimal Xcode project.

Deliverables:
1. `uniffi.toml` configured
2. `scripts/build-rust.sh` functional
3. Xcode project `macos/Takt.xcodeproj`
4. Build phase linking `liblibtakt.a`
5. `TaktCore+Bridge.swift` — PlatformBridge implementation
6. Minimal app that initializes TaktCore and prints task list

Validation: Xcode build succeeds, app launches, TaktCore initializes without crash.

### Phase 3 — UI: Popover + Task List

Deliverables:
1. `TaktApp.swift` with `MenuBarExtra`
2. `TaskListView.swift` + `TaskListViewModel.swift`
3. `TaskItemView.swift` with toggle, run, delete
4. Tray icon (template image asset)
5. Activation policy management

Validation: App in menu bar, lists tasks, toggle/run/delete work.

### Phase 4 — UI: Editor

Deliverables:
1. `TaskEditorView.swift` + `TaskEditorViewModel.swift`
2. `ActionBuilderView.swift` — all 7 action types
3. `ScheduleBuilderView.swift` — all 3 schedule types
4. `Schedule/DayGridView.swift`, `WeekdayGridView.swift`, `TimeFieldView.swift`
5. `CronUtils.swift` (port of cron-utils.ts)
6. `AutoName.swift` (port of AutoName.ts)
7. `TemplateGridView.swift`
8. Dirty detection + unsaved changes confirmation
9. Multi-window support (multiple editors)

Validation: Create, edit, save tasks with all 7 action types and 3 schedule types.

### Phase 5 — History + Polish

Deliverables:
1. `HistoryView.swift` with expand/collapse
2. LaunchAgent registration (reuse Rust code)
3. Accessibility permission request (Swift)
4. Notification permission request (Swift)
5. Single instance enforcement (Swift)
6. Keyboard shortcuts (Cmd+N, Cmd+W)
7. App icon (Assets.xcassets)

Validation: Full feature parity with Tauri+React version.

### Phase 6 — Cleanup

Deliverables:
1. Delete `src-tauri/` (migrations already copied to libtakt)
2. Delete `src/` (React components)
3. Delete `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `editor.html`
4. Update `.gitignore` (add `macos/Takt/Generated/`, remove node_modules)
5. Update README

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| UniFFI async support for callback interfaces | High | Use callback registry pattern with sync dispatch + channel; validate in Phase 2 |
| `MenuBarExtra(.window)` customization limits | Medium | Fallback: manual `NSPopover` via NSApplicationDelegate |
| Cron builder complexity in SwiftUI | Medium | Mechanical port of `cron-utils.ts` — pure logic, no UI dependency |
| SQLx migrations path change | Low | Copy `migrations/` directory; same schema, no new migrations |
| Existing user DB must keep working | High | Same path, same schema — no breaking changes |
| `run_on_main_sync` deadlock risk | Medium | Use `DispatchQueue.main.async` with semaphore instead of `.sync` if main thread is calling into Rust |
