# Action System Redesign

## Problem

1. **Shortcut is misplaced** — standalone action type that only makes sense chained after opening something (file, app, URL)
2. **No Application action** — can't schedule opening an app
3. **No file picker** — user types file/app paths manually
4. **osascript dependency** — notifications, shortcuts, and open commands use `osascript` / `open` CLI, which can have permission issues and feels non-native
5. **Bug: disable doesn't prevent execution** — scheduler checks `enabled` only at startup, not at execution time
6. **Bug: window.close not allowed** — missing `core:window:allow-close` permission in Tauri capabilities

## Design

### Data Model Changes

**Remove** `Shortcut` as standalone action type.

**Add** `OpenApp` action type.

**Add** `post_shortcuts` field to actions that open something visual.

**Add** `KeyCombo` type.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    OpenFile {
        path: String,
        app: Option<String>,        // open with specific app
        post_shortcuts: Vec<KeyCombo>,
    },
    OpenUrl {
        url: String,
        browser: Option<String>,
        post_shortcuts: Vec<KeyCombo>,
    },
    OpenApp {
        app_path: String,            // e.g. "/Applications/Slack.app"
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
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Modifier {
    Cmd,
    Shift,
    Opt,
    Ctrl,
}
```

**TypeScript equivalent** in `src/lib/api.ts`:

```ts
export type Modifier = 'Cmd' | 'Shift' | 'Opt' | 'Ctrl';

export interface KeyCombo {
  modifiers: Modifier[];
  key: string;
}

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

### Database

No migration. Drop and recreate tables. The SQL schema in `migrations/001_initial.sql` stays the same (tasks store `action_json` as TEXT). The JSON shape changes but the table structure doesn't.

The `db::connect()` already runs migrations via sqlx. To force recreation: either delete the DB file on startup if schema version is old, or simply delete the existing `cronmac.db` file manually for now. Since the app is pre-release (v0.1.0), data loss is acceptable.

Approach: add a `002_recreate.sql` migration that drops both tables and recreates them. This ensures sqlx migration tracking stays consistent.

### Native Executor (macOS)

Full migration from CLI/osascript to native macOS APIs.

**New Rust dependencies:**
- `objc2` — Objective-C runtime
- `objc2-foundation` — Foundation framework (NSString, NSURL, etc.)
- `objc2-app-kit` — AppKit framework (NSWorkspace, NSRunningApplication)
- `core-graphics` — CGEvent API for keyboard events

**Action execution mapping:**

| Action | Implementation |
|--------|---------------|
| OpenFile | `NSWorkspace.openURL(_:configuration:)` with optional `NSWorkspaceOpenConfiguration.setApplication()` |
| OpenUrl | `NSWorkspace.openURL(_:configuration:)` with optional browser app |
| OpenApp | `NSWorkspace.openApplicationAtURL(_:configuration:)` |
| RunCommand | `std::process::Command` (unchanged — no native API advantage) |
| Notify | `tauri-plugin-notification` (already native wrapper around UNUserNotificationCenter) |
| Webhook | `reqwest` (unchanged — already native Rust) |
| post_shortcuts (KeyCombo) | `CGEventCreateKeyboardEvent` + `CGEventPost` via `core-graphics` crate |

**Post-shortcuts execution flow:**

1. Execute primary action (open file/url/app) via NSWorkspace
2. Wait for target app to become frontmost:
   - Get the target app's bundle identifier or name from the open operation
   - Poll `NSRunningApplication.isActive` every 200ms
   - Timeout after 10 seconds (log warning, skip shortcuts)
3. Execute each KeyCombo in sequence:
   - Map `Modifier` enum to CGEvent flags
   - Map `key` string to virtual keycode
   - Create CGEvent with keycode + modifier flags
   - Post key-down then key-up event
   - 50ms delay between each KeyCombo

**Virtual keycode mapping:**
Maintain a lookup table mapping common key names (a-z, 0-9, return, tab, space, escape, arrow keys, F1-F12, etc.) to macOS virtual keycodes. This is a static `HashMap<&str, u16>`.

**Accessibility permission:**
`CGEventPost` requires Accessibility permission. The app should:
- Check `AXIsProcessTrusted()` before attempting to send key events
- If not trusted, return an error indicating the user needs to grant Accessibility permission
- The error surfaces in the execution log and in the editor UI

**Notification migration:**
Replace the osascript `display notification` with `tauri-plugin-notification`. The plugin is already installed and has `notification:default` permission. The `MacosExecutor` needs to hold an `AppHandle` to call the notification plugin's Rust API. Update the executor constructor: `MacosExecutor::new(app_handle: AppHandle)`. The `AppHandle` is available in `lib.rs` during setup and should be passed through to the executor.

### UI Changes

**tauri-plugin-dialog:** Add dependency for native file picker dialogs.

**ActionBuilder changes:**

1. **Remove** "Keys" tab (Shortcut is no longer standalone)
2. **Add** "App" tab for OpenApp
3. **OpenFile** — replace text input with: readonly input showing filename + "Browse" button that opens native file picker. Optional "Open with" app selector (same picker filtered to /Applications).
4. **OpenApp** — readonly input showing app name + "Browse" button filtered to `/Applications/*.app`
5. **OpenUrl** — unchanged (URL is typed, not picked)
6. **Post-shortcuts section** — collapsible section "Run shortcuts after open" inside OpenFile, OpenUrl, OpenApp:
   - Each KeyCombo row: toggle buttons for modifiers (Cmd/Shift/Opt/Ctrl) + text input for the key
   - "+" button to add a new KeyCombo
   - "×" button to remove a KeyCombo
   - Empty by default (collapsed)

**TemplateGrid changes:**
- Replace "Keys" template with "Open App" template
- OpenApp defaults: `{ app_path: '', post_shortcuts: [] }`, schedule: `Cron "0 9 * * *"`

**AutoName changes:**
- Add `OpenApp` handling: extract app name from path (e.g., `/Applications/Slack.app` → "Open Slack")
- Remove `Shortcut` handling

**TaskEditor defaults:**
- Update `defaultAction` for new types
- `OpenFile` default: `{ type: 'OpenFile', path: '', app: undefined, post_shortcuts: [] }`
- `OpenUrl` default: `{ type: 'OpenUrl', url: '', browser: undefined, post_shortcuts: [] }`
- `OpenApp` default: `{ type: 'OpenApp', app_path: '', post_shortcuts: [] }`

### Bug Fixes

**Bug 1: Disable doesn't prevent execution**

In `scheduler.rs`, inside the job closure that executes tasks, add an `enabled` check:
```rust
// Before executing, re-fetch the task to check if still enabled
let task = store.get_task(&task_id).await;
if let Some(task) = task {
    if !task.enabled {
        // Log as skipped
        store.log_execution(&task_id, "skipped", None, None, None).await;
        return;
    }
    // proceed with execution...
}
```

This ensures that even if a task was enabled when scheduled, disabling it in the UI prevents the next execution.

**Bug 2: window.close not allowed**

Add `core:window:allow-close` to `src-tauri/capabilities/default.json`:
```json
{
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

Also adds `dialog:default` for the new file picker functionality.

### Tauri Capabilities

Full updated permissions list:
- `core:default` — base IPC
- `core:window:allow-create` — editor window creation
- `core:window:allow-close` — editor window close after save/delete
- `core:webview:allow-create-webview-window` — WebviewWindow JS constructor
- `dialog:default` — native file/app picker dialogs
- `opener:default` — URL opener
- `notification:default` — native notifications
- `shell:default` — shell commands (RunCommand)

## Out of Scope

- Drag-to-reorder shortcuts
- Database migration framework (drop+recreate for now)
- Dark mode specific adjustments
- Recording keyboard shortcuts (capture mode)
- Cross-platform executor (Linux/Windows)
