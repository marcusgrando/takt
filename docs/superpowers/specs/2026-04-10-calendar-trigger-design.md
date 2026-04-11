# Calendar Trigger & Event Links Action — Design Spec

**Date:** 2026-04-10
**Status:** Draft — awaiting user review
**Scope:** Add a calendar-based schedule type and an action that opens links from calendar events.

## 1. Goal

Enable Takt to trigger tasks based on macOS system calendars (iCloud, Google Calendar synced to macOS, Local, etc.) via EventKit. Users can schedule tasks to fire a configurable number of minutes before events on a selected calendar, optionally filtered by a title substring. A new action opens the event's video conference link and any URLs found in the event notes.

## 2. Non-Goals (v1)

- Advanced filters (regex, organizer, recurrence type).
- Multiple offsets per task (e.g., 15 min and 0 min).
- Direct Google/Microsoft API integration (EventKit already aggregates these).
- Full Windows implementation (the design supports it; implementation is a separate spec).
- Editing or creating calendar events from Takt (read-only).
- Clickable chips to insert template variables in the editor (nice-to-have).

## 3. Architecture Overview

The design follows Takt's existing principle of **thin Swift UI + thick Rust core**, extended through the existing `PlatformBridge`:

- **Rust (`libtakt`)** owns the polling loop, event filtering, offset calculation, dedup logic, template variable substitution, and action dispatch. Shared across platforms.
- **Swift (`macos/Takt`)** implements a new `PlatformBridge` capability that talks to `EventKit` and returns events as serialized structs. Only the native calendar adapter is platform-specific.
- Windows portability is preserved: a future Windows UI would implement the same `PlatformBridge` calendar methods against `Windows.ApplicationModel.Appointments` or Microsoft Graph, reusing all the Rust logic unchanged.

## 4. Data Model

### 4.1 New `Schedule` variant

`libtakt/src/models.rs`:

```rust
pub enum Schedule {
    Cron { expression: String },
    OneShot { run_at: String },
    DailyFirstUse { delay_minutes: u64 },
    Calendar {
        calendar_id: String,
        title_contains: Option<String>,
        minutes_before: u32,
    },
}
```

- `calendar_id`: stable identifier from the system calendar store (EventKit's `calendarIdentifier`).
- `title_contains`: optional, case-insensitive substring filter on event title. `None` means "all events".
- `minutes_before`: offset from event start. `0` = fire at event start. Range 0–120.

### 4.2 `CalendarEvent` struct

Returned by the platform bridge and passed through the execution pipeline as context:

```rust
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,              // ISO 8601
    pub end: String,                // ISO 8601
    pub notes: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,        // event URL (EKEvent.url)
    pub conference_url: Option<String>,
    pub calendar_id: String,
}
```

### 4.3 `CalendarInfo` struct

Returned by `list_calendars()` for the editor dropdown:

```rust
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    pub source: String,             // "iCloud", "Google", "Local", ...
    pub color_hex: Option<String>,
}
```

### 4.4 Calendar access status & task health

Two new enums are added to `models.rs`:

```rust
pub enum CalendarAccessStatus {
    NotDetermined,   // user has not been asked yet
    Denied,          // user denied (or restricted by parental controls / MDM)
    Authorized,      // full access granted
}

pub enum TaskHealth {
    Healthy,
    CalendarNotFound,              // task references a calendar_id that no longer exists
    CalendarAccessDenied,          // permission revoked after task was created
    CalendarAccessNotDetermined,   // permission was never granted
}
```

`TaskDto` gains a derived field, populated at read time by `list_tasks()` / `get_task()`:

```rust
pub struct TaskDto {
    // ... existing fields ...
    pub health: TaskHealth,        // always `Healthy` for non-Calendar schedules
}
```

Health is **not persisted**. It is computed on every read by consulting the platform bridge once per `list_tasks()` call (see §6.4 caching rules). This keeps the UI declarative — `TaskItemView` just renders the badge from `task.health`.

### 4.5 New `Action` variant

```rust
Action::OpenEventLinks {
    open_conference: bool,
    open_notes_links: bool,
    browser: Option<String>,
}
```

- Requires event context. When invoked without one (e.g., "Run Now" on a non-calendar task), returns `ExecutorError::MissingEventContext` with a user-friendly message.

### 4.6 New table: `calendar_dispatches`

Serves two purposes: (a) deduplicates dispatches of the same event across consecutive polls, (b) persists the set of pending sleeps so they can be reconstituted after a process restart.

```sql
CREATE TABLE calendar_dispatches (
    task_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_start TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('scheduled', 'dispatched')),
    trigger_at TEXT NOT NULL,       -- ISO 8601, pre-computed event_start - minutes_before
    reserved_at TEXT NOT NULL,      -- ISO 8601, when the row was inserted
    dispatched_at TEXT,             -- ISO 8601, set when status transitions to 'dispatched'
    PRIMARY KEY (task_id, event_id, event_start)
);
CREATE INDEX idx_calendar_dispatches_status ON calendar_dispatches(status);
CREATE INDEX idx_calendar_dispatches_start ON calendar_dispatches(event_start);
```

State transitions:

- **Poller reserves a slot** (sleep about to start): insert row with `status = 'scheduled'`, `trigger_at` pre-computed.
- **Sleep fires, about to execute**: update row to `status = 'dispatched'`, set `dispatched_at = now()`. This happens **after** the re-check of task enabled state (see §5.2) but **before** calling `execute_and_log`.
- **Catch-up slot consumed without running** (`run_if_missed = false`): insert row directly with `status = 'dispatched'`. This prevents reconsideration and pairs with a `skipped` entry in `execution_logs` (the real audit trail).
- **Task deleted or disabled**: rows with `status = 'scheduled'` for that `task_id` are **deleted** (not kept as "cancelled"). Rows with `status = 'dispatched'` are retained until pruning so we remember the slot was consumed.

Pruning: each poller tick deletes rows with `event_start < now - 24h`.

Restart recovery: see §5.2 "Reconstitute pending timers on startup".

Migration: `libtakt/migrations/002_calendar.sql`. No changes to `tasks` table — `schedule_json` is text and serializes the new variant via serde.

## 5. Execution Flow

### 5.1 `CalendarPoller`

New file: `libtakt/src/calendar.rs`.

A tokio task managed by `AppScheduler`. Runs only while at least one enabled task has `Schedule::Calendar`.

Constants:

- `CALENDAR_POLL_INTERVAL_SECS = 300` (5 minutes).
- `CALENDAR_LOOKAHEAD_MARGIN_SECS = 60`.
- `CALENDAR_LOOKBACK_MARGIN_SECS = 60` (slack for wake-from-sleep).

Each tick:

1. Load all enabled tasks with `Schedule::Calendar` from the store.
2. Group by `calendar_id` to minimize bridge calls.
3. Compute the event window:
   - `lookahead_minutes = max(task.minutes_before over this calendar) + (poll_interval + lookahead_margin) / 60`
   - `lookback_minutes = (poll_interval + lookback_margin) / 60` — ensures events whose `start` was just before `now` (e.g., because the app was asleep) still appear in the bridge response.
4. For each calendar, call `bridge.fetch_events_in_window(calendar_id, lookback_minutes, lookahead_minutes)`. (The bridge method is renamed from `fetch_upcoming_events` — see §6.1.)
5. For each `(task, event)` pair:
   - Skip if `title_contains` set and does not match (case-insensitive).
   - Compute `trigger_at = event.start - minutes_before`.
   - Skip if `trigger_at > now + poll_interval` (will be handled by a future tick).
   - Skip if a row exists in `calendar_dispatches` for `(task.id, event.id, event.start)` (regardless of status — we do not dispatch twice).
   - Three sub-cases for inserting the reservation:
     - **Future slot** (`trigger_at > now`): insert row with `status = 'scheduled'`, `trigger_at` pre-computed. Reserve a `CancellationToken` (see §5.2) and spawn a sleep that fires at `trigger_at` then runs the dispatch routine.
     - **Missed slot, `run_if_missed = true`**: insert row with `status = 'scheduled'` and immediately spawn the dispatch routine (no sleep). The dispatch routine will transition the row to `'dispatched'` just before executing.
     - **Missed slot, `run_if_missed = false`**: insert row directly with `status = 'dispatched'` and log a `skipped` entry in `execution_logs`. Slot is consumed without executing.
6. Prune `calendar_dispatches` rows with `event_start < now - 24h`.

The dispatch routine (spawned task, whether sleeping first or running immediately):

1. Wait for the sleep (if any), racing a `CancellationToken` from `cancel_tokens` — same pattern as `OneShot` in `scheduler.rs:217-242`.
2. Re-read the task from the store via `store.get_task(task_id)`:
   - If not found → delete the `scheduled` row, log nothing, return.
   - If `!enabled` → delete the `scheduled` row, log a `skipped` execution, return.
   - If the `Schedule::Calendar` fields changed (e.g., user edited `calendar_id` or `title_contains`) → delete the `scheduled` row and return; the next poll tick will re-evaluate.
3. **Re-fetch the event from the platform bridge** using `bridge.fetch_events_in_window(task.calendar_id, lookback, lookahead)` with a tight window around the original `trigger_at`, and find the event whose `id` matches the one stored in the dedup row:
   - If the event is no longer returned (cancelled, deleted, moved out of window, access revoked): transition the dedup row to `dispatched`, log a `skipped` execution with reason `"event no longer exists"`, return. We do not fall back to the stale snapshot — stale data is a user-visible bug (wrong Zoom link, wrong title in notifications).
   - If the event is still present but its `start` shifted, and the new `start - minutes_before` is now more than `CALENDAR_POLL_INTERVAL_SECS` in the future: delete the `scheduled` row and return. The next poll tick will create a fresh reservation at the new `trigger_at`.
   - If the event is still present and the new `start` is close enough to `now`: proceed with the **freshly fetched** `CalendarEvent` (not the snapshot captured at reservation time).
4. Transition the dedup row from `scheduled` → `dispatched` (update `status`, set `dispatched_at`).
5. Call `execute_and_log(task, Some(fresh_event))` — always the fresh copy.
6. Remove the entry from `cancel_tokens`.

**Fire-time data rule**: the `CalendarEvent` captured at reservation time is used **only** as a persistence key (via `event_id` + `event_start` in `calendar_dispatches`). Every actual execution — normal path, immediate catch-up, and restart reconstitute — reads the event fresh from the bridge just before dispatching. This is a single rule across all code paths; there is no snapshot fallback.

### 5.2 Integration with `AppScheduler`

- New method `ensure_calendar_poller(&mut self)`: starts the poller if any calendar-scheduled task exists and not already running; stops it otherwise.
- Called from `load_all_tasks()`, `schedule_task()`, `unschedule_task()`, and `update_task()` paths.
- `schedule_task()` for `Schedule::Calendar` does not create a `tokio-cron-scheduler` job. The poller is the sole dispatcher for calendar tasks.
- Each pending sleep (§5.1 future slot) registers a `CancellationToken` in the existing `cancel_tokens: HashMap<String, CancellationToken>` field, keyed as `"calendar:{task_id}:{event_id}:{event_start}"` to allow per-slot cancellation without colliding with `OneShot`/`DailyFirstUse` entries keyed by plain `task_id`.
- `remove_task(task_id)` (see `scheduler.rs:273`) is extended to: (a) cancel all tokens whose key starts with `"calendar:{task_id}:"`, (b) delete all `calendar_dispatches` rows with `status = 'scheduled'` for that `task_id`. Rows with `status = 'dispatched'` are kept (historical slot-consumption marker) until pruned.
- `update_task()` follows the same cancel-and-rebuild pattern: cancel all `"calendar:{task_id}:*"` tokens and delete `scheduled` rows, then let the next poll tick rebuild reservations from the new schedule.

### 5.2.1 Reconstitute pending timers on startup

On `AppScheduler::start()`, after loading all tasks but before starting the poller's periodic ticks:

1. Query `calendar_dispatches WHERE status = 'scheduled' AND trigger_at >= now`.
2. For each row, look up the corresponding task:
   - If the task no longer exists, is disabled, or no longer has `Schedule::Calendar` → delete the row.
   - Otherwise, register a `CancellationToken` and spawn a sleep until `trigger_at`. **No event data is re-fetched at reconstitute time** — the dispatch routine itself is responsible for re-fetching the event just before executing (§5.1 "Fire-time data rule"). `calendar_dispatches` only persists `(task_id, event_id, event_start, trigger_at)` — never the event body.
3. Additionally query `calendar_dispatches WHERE status = 'scheduled' AND trigger_at < now` — these are missed reservations (app was down past the trigger time). For each:
   - If the task's `run_if_missed = true` → spawn the dispatch routine immediately. The routine re-fetches the event per §5.1; if the event is gone, it logs a `skipped` execution.
   - Otherwise → mark row as `dispatched`, log `skipped` in `execution_logs`.

This closes the gap where a restart between reservation and sleep-fire would otherwise cause the event to be lost (the bug flagged in review #1), and it uses the same single fire-time data rule as every other dispatch path (no snapshot vs. re-fetch inconsistency).

### 5.3 Executor signature change

The `ActionExecutor` trait gains an event context parameter:

```rust
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(
        &self,
        action: &Action,
        event: Option<&CalendarEvent>,
    ) -> Result<ExecutionResult, ExecutorError>;
}
```

Existing callers (all non-calendar schedules) pass `None`. This is a breaking change local to the crate — all call sites are updated in the same commit.

`execute_and_log()` in `scheduler.rs` also takes `Option<CalendarEvent>` and, on success, prepends `stdout` with a traceability line:

```
Triggered by event: {title} @ {start}
```

### 5.4 Template variable substitution

New module: `libtakt/src/template.rs`.

```rust
pub fn substitute_event_vars(text: &str, event: Option<&CalendarEvent>) -> String;
```

Supported variables:

- `{{event.title}}`
- `{{event.start}}`
- `{{event.end}}`
- `{{event.notes}}`
- `{{event.location}}`
- `{{event.url}}`
- `{{event.conference_url}}`
- `{{event.calendar_id}}`

Behavior:

- If `event` is `None`, all `{{event.*}}` tokens are replaced with empty strings and a debug log is emitted (permissive — the variables only make sense with calendar triggers, and empty-substitution is safer than failing).
- Unknown `{{event.xxx}}` tokens are left as-is (typo visibility).
- Applied by the executor to these fields before dispatch:
  - `OpenUrl.urls` (each URL)
  - `RunCommand.command`, `RunCommand.args`
  - `Notify.title`, `Notify.body`
  - `Webhook.url`, `Webhook.body`, `Webhook.headers` (values)

### 5.5 `OpenEventLinks` execution

New module: `libtakt/src/url_extract.rs` — portable, unit-tested Rust.

```rust
pub fn extract_urls_from_text(text: &str) -> Vec<String>;
```

Uses a regex for HTTP(S) URLs. Deduplicates against `conference_url`. Order preserved.

Executor path (`macos.rs`):

1. Require `event` context. If `None`, return `ExecutorError::MissingEventContext`.
2. Collect URLs:
   - If `open_conference` and `event.conference_url.is_some()`, add it.
   - If `open_notes_links`, extract from `event.notes` and `event.location`, dedup against conference URL.
3. Open each URL via the existing `OpenUrl` code path (NSWorkspace, respecting `browser`).
4. Return success with `stdout` listing opened URLs.

## 6. Platform Bridge

### 6.1 Trait extensions

`libtakt/src/platform.rs`:

```rust
pub trait PlatformBridge: Send + Sync {
    // ... existing methods ...

    fn get_calendar_access_status(&self) -> Result<CalendarAccessStatus, String>;
    fn request_calendar_access(&self) -> Result<CalendarAccessStatus, String>;
    fn list_calendars(&self) -> Result<Vec<CalendarInfo>, String>;
    fn fetch_events_in_window(
        &self,
        calendar_id: String,
        lookback_minutes: u32,
        lookahead_minutes: u32,
    ) -> Result<Vec<CalendarEvent>, String>;
}
```

- `get_calendar_access_status()` is the **only** way callers should determine permission state. `list_calendars()` returning an empty vec MUST NOT be interpreted as "permission denied" — an authorized user may genuinely have zero visible calendars. `list_calendars()` returns `Err("calendar_access_denied")` when called without authorization, not an empty vec.
- `fetch_events_in_window` replaces the earlier `fetch_upcoming_events` to support retroactive lookup for catch-up (review #3).
- `request_calendar_access()` returns the resulting status (not a bool), so the caller can distinguish `Authorized` from `Denied` from `NotDetermined` (the latter happens if the user dismisses the prompt without choosing).

Follows the existing synchronous `runOnMainSync` + callback-registry pattern used by `is_user_active` and `send_notification`.

### 6.2 macOS implementation

New file: `macos/Takt/TaktCore+Calendar.swift`.

- Lazy `EKEventStore` instance held by `MacOSPlatformBridge`.
- `getCalendarAccessStatus()`: maps `EKEventStore.authorizationStatus(for: .event)` to `CalendarAccessStatus` (`.notDetermined → NotDetermined`, `.fullAccess → Authorized`, everything else → `Denied`).
- `requestCalendarAccess()`: calls `requestFullAccessToEvents` (macOS 14+). After the call, re-reads `authorizationStatus` and returns the mapped `CalendarAccessStatus`.
- `listCalendars()`:
  - If status != `Authorized`, returns `Err("calendar_access_denied")`.
  - Otherwise returns `store.calendars(for: .event)` mapped to `CalendarInfo { id: calendarIdentifier, title: title, source: source.title, color_hex: cgColor → hex }`. May be an empty vec (user is authorized but has zero visible calendars — legitimate state).
- `fetchEventsInWindow(calendarId, lookbackMinutes, lookaheadMinutes)`:
  - If status != `Authorized`, returns `Err("calendar_access_denied")`.
  - Finds `EKCalendar` by `calendarIdentifier`. If missing, returns `Err("calendar_not_found:{calendarId}")` so the caller can distinguish this from "authorized, no events" (review #6 requires this signal for `TaskHealth::CalendarNotFound`).
  - `predicateForEvents(withStart: now - lookback*60, end: now + lookahead*60, calendars: [cal])`.
  - For each `EKEvent`, build `CalendarEvent`:
    - `id = eventIdentifier`
    - `title = title ?? ""`
    - `start/end` = ISO 8601
    - `notes = notes`
    - `location = location`
    - `url = url?.absoluteString`
    - `conference_url`: prefer `EKEvent.conferenceURL` (macOS 12+); fallback to regex over `notes` + `location` looking for `meet.google.com`, `zoom.us`, `teams.microsoft.com`, `webex.com`.
    - `calendar_id = calendarIdentifier`

### 6.3 Permissions

- The Takt target generates its `Info.plist` at build time via `GENERATE_INFOPLIST_FILE = YES` and `INFOPLIST_KEY_*` build settings (see `macos/Takt.xcodeproj/project.pbxproj:283`, `:327`). There is **no** `Info.plist` file in the source tree.
- Action: add `INFOPLIST_KEY_NSCalendarsFullAccessUsageDescription` to both the Debug and Release build settings of the `Takt` target, value: `"Takt reads your calendars to trigger automations based on upcoming events. Data stays on your device."`.
- **Permission prompt is always user-gated, never automatic.** When the user selects the `Calendar` schedule tab in the editor, `CalendarScheduleBuilder` calls `core.getCalendarAccessStatus()`. If the status is `NotDetermined`, the view renders a visible **"Grant Calendar Access"** button (and no calendar dropdown). Only clicking that button calls `core.requestCalendarAccess()`, which in turn triggers EventKit's native prompt. Selecting the tab alone never shows a prompt. See §7.2 for the full UX.

### 6.4 New `TaktCore` APIs

The Swift UI does not call `PlatformBridge` directly — it goes through `TaktCore`. Three new public methods are added to `libtakt/src/lib.rs` (alongside `list_tasks`, `get_task`, etc.):

```rust
impl TaktCore {
    pub async fn get_calendar_access_status(&self) -> Result<CalendarAccessStatus, TaktError>;
    pub async fn request_calendar_access(&self) -> Result<CalendarAccessStatus, TaktError>;
    pub async fn list_calendars(&self) -> Result<Vec<CalendarInfo>, TaktError>;
}
```

Each wraps the corresponding `PlatformBridge` method via `tokio_runtime().spawn` and maps bridge errors to `TaktError::CalendarAccessDenied` or a generic variant, following the same pattern as existing methods.

**Task health population caching.** `list_tasks()` and `get_task()` populate the new `TaskDto.health` field:

1. If the task schedule is not `Calendar`, set `health = Healthy` unconditionally (no bridge call).
2. Otherwise, within a single `list_tasks()` call:
   - Call `bridge.get_calendar_access_status()` **once** per call.
   - If `NotDetermined` → all calendar tasks get `CalendarAccessNotDetermined`. Do not call `list_calendars()`.
   - If `Denied` → all calendar tasks get `CalendarAccessDenied`.
   - If `Authorized` → call `bridge.list_calendars()` **once**, build a `HashSet<calendar_id>`, then for each calendar task check if its `calendar_id` is in the set. In-set → `Healthy`. Not-in-set → `CalendarNotFound`.
3. For `get_task()` (single-task path), apply the same rules but without the set (directly check the specific `calendar_id` against `list_calendars()` output).

This keeps health cost bounded: **at most two bridge calls per `list_tasks()`**, regardless of how many calendar tasks exist. Health is never persisted — it reflects the current system state every time the UI refreshes.

## 7. UI (SwiftUI)

### 7.1 `ScheduleBuilderView`

Adds a fourth option to the schedule type selector: `Cron | OneShot | DailyFirstUse | Calendar`. When `Calendar` is selected, renders `CalendarScheduleBuilder`.

### 7.2 `CalendarScheduleBuilder` (new)

File: `macos/Takt/Views/CalendarScheduleBuilder.swift`.

Layout (illustrative):

```
Calendar:       [ Work (Google) ▼ ]
Title contains: [ standup           ]   (placeholder: "any event")
Trigger:        [ 5 ] minutes before
```

Behavior:

- On appear, calls `core.getCalendarAccessStatus()` first. Based on the result:
  - `NotDetermined`: shows a "Grant Calendar Access" button that calls `core.requestCalendarAccess()` and refreshes the view on return.
  - `Denied`: shows a yellow banner with a link that opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars` and hides the rest of the form (selecting a calendar is impossible without access).
  - `Authorized`: calls `core.listCalendars()` and renders the dropdown.
- Calendar dropdown shows title + source (e.g., "Work (Google)"). Persists `calendar_id`. Empty list (legitimate "no calendars") shows a neutral info text, not a permission banner.
- Title contains: plain `TextField`, optional.
- Minutes before: `Stepper` 0–120, default 5.

### 7.3 `ActionBuilderView`

Adds an eighth action type button (icon `calendar.badge.clock`, label "Open Event Links"). When selected, renders:

- Toggle "Open video conference link" (default on).
- Toggle "Open links from event notes" (default on).
- Browser picker (reuses the existing one from `OpenUrl`).
- Help line in italic: *"Only fires with Calendar schedule. Run Now will fail without event context."*

### 7.4 Template variable hints

When the current task has `Schedule::Calendar` and the action is `OpenUrl`, `Notify`, `RunCommand`, or `Webhook`, show a subtle help line below the relevant text fields:

> *Use `{{event.title}}`, `{{event.start}}`, `{{event.conference_url}}`, etc.*

### 7.5 `AutoName`

New cases in `macos/Takt/Helpers/AutoName.swift`:

- `OpenEventLinks` + `Calendar` → `"Open links — 5 min before standup"` (uses `title_contains` and `minutes_before`).
- `Calendar` alone → `"before any Work event"` when `title_contains` is empty; `"when Work events start"` when `minutes_before == 0`.

### 7.6 Task list badges (`TaskItemView`)

`TaskItemView` (`macos/Takt/Views/TaskItemView.swift:134`) gains a health badge derived from `task.health`. The badge sits next to the action label:

| `TaskHealth`                      | Badge text                 | Badge color | Tooltip                                               |
|-----------------------------------|----------------------------|-------------|-------------------------------------------------------|
| `Healthy`                         | (none — no badge rendered) | —           | —                                                     |
| `CalendarNotFound`                | "Calendar missing"         | red         | "The calendar referenced by this task no longer exists." |
| `CalendarAccessDenied`            | "Access denied"            | orange      | "Grant calendar access in System Settings."           |
| `CalendarAccessNotDetermined`     | "Grant access"             | yellow      | "Calendar access has not been granted yet."           |

Clicking the badge opens the task in the editor (same as the existing edit button) so the user can fix or delete.

### 7.7 `TaskEditorViewModel`

- Lazy-loads the calendar list only when the user selects `Calendar` as schedule type.
- Validation: saving with `Calendar` schedule requires a non-empty `calendar_id`. Otherwise, toasts an error.
- If the user picks `OpenEventLinks` but no `Calendar` schedule, shows an inline warning ("This action only runs with a Calendar schedule") — non-blocking; user can still save.

### 7.8 Quick-start template

New case added to the existing `ActionTemplate` enum in `macos/Takt/TaktApp.swift:81`: **`openMeetingLinks`**, rendered in `TemplateGridView` with label "Open meeting links" and icon `calendar.badge.clock`.

**Resolution strategy.** `ActionTemplate` is a synchronous enum today (`defaultAction`/`defaultSchedule` return pure values — `TaktApp.swift:108, :127`). We do **not** change that architecture. The new template emits:

- `defaultSchedule`: `Schedule::Calendar { calendar_id: "", title_contains: None, minutes_before: 5 }` — deliberately empty `calendar_id`.
- `defaultAction`: `Action::OpenEventLinks { open_conference: true, open_notes_links: true, browser: None }`.

When the user taps the template, `EditorWindowContent` opens the editor with those defaults. `CalendarScheduleBuilder` (§7.2) handles every downstream case without any new logic:

- **Status `NotDetermined`** → builder shows the "Grant Calendar Access" button. Save is blocked by the existing empty-`calendar_id` validation.
- **Status `Denied`** → builder shows the yellow banner with the System Settings deep link. Save blocked identically.
- **Status `Authorized`, non-empty calendar list** → the dropdown renders with the first entry visually highlighted (via an existing default-to-first-item pattern in the dropdown component), but the underlying `schedule.calendar_id` stays `""` until the user explicitly picks. This forces a conscious choice and avoids silently binding to whichever calendar EventKit happened to list first.
- **Status `Authorized`, empty calendar list** → builder shows the neutral "No calendars found" info text. Save blocked by validation.

`TaskEditorViewModel.save()` (§7.7) already rejects a `Calendar` schedule with empty `calendar_id` and surfaces the error inline. No new validation is needed for the template path — it reuses the editor's existing gate.

This keeps `ActionTemplate` synchronous (no bridge calls, no async `defaultSchedule`) and pushes all the permission/empty-list handling into the single place that already owns it: `CalendarScheduleBuilder`.

## 8. Error Handling

- New `TaktError` variants: `CalendarAccessDenied`, `CalendarNotFound { id: String }`.
- New `ExecutorError` variant: `MissingEventContext` (with user-friendly message for the "Run Now" case).
- Access states are **not** errors: `NotDetermined` and `Denied` flow through `TaskDto.health` and are handled by the UI (see §7.6). The error variants are reserved for unexpected failures at dispatch time.
- `CalendarPoller` is resilient: an error fetching one calendar logs and continues with the others. Never panics.
- If a calendar referenced by a task was deleted from the system, the bridge returns `calendar_not_found:{id}`; the poller logs a warning and skips the task for that tick. `TaskDto.health` surfaces this as `CalendarNotFound` on the next `list_tasks()` call so the user can fix or delete the task (badge rendered by §7.6).
- `substitute_event_vars` with `None` event emits debug log, never errors.
- `request_calendar_access` returning `Denied` surfaces as a banner in `CalendarScheduleBuilder` with a "Open System Settings" deep link.

## 9. Testing

### 9.1 Rust unit/integration tests

- `libtakt/tests/calendar_poller.rs` — uses a `MockPlatformBridge` that returns controlled event lists:
  - Event inside the window dispatches.
  - Event outside the window does not dispatch.
  - `title_contains` filter is case-insensitive.
  - Two consecutive poll ticks for the same event produce exactly one dispatch (dedup via `status = 'scheduled' → 'dispatched'`).
  - Disabled task is ignored at poll time.
  - Task disabled **between** reservation and sleep-fire is re-checked and skipped (logs `skipped` in `execution_logs`, deletes the `scheduled` row).
  - Task deleted between reservation and sleep-fire cancels the token cleanly without panic.
  - `update_task()` that changes `calendar_id` cancels all `scheduled` rows for the old slot, letting the next tick rebuild reservations.
  - Catch-up from retroactive window: event whose `start` is slightly before `now` (simulating wake-from-sleep) still appears in `fetch_events_in_window` and dispatches when `run_if_missed = true`; is marked `dispatched` + logged `skipped` when false.
  - **Restart reconstitute (§5.2.1)**: seed `calendar_dispatches` with a `scheduled` row whose `trigger_at` is in the future, restart the scheduler, verify the sleep is re-created and fires at the correct time.
  - **Restart reconstitute — past slot**: seed a `scheduled` row with `trigger_at < now`, verify it is dispatched immediately (if `run_if_missed = true`) or marked `dispatched` + skipped logged (if false).
  - **Restart reconstitute — orphan row**: seed a `scheduled` row whose task no longer exists; restart cleans it up without error.
  - Calendar removed from system: bridge returns `Err("calendar_not_found:{id}")`; poller logs, does not dispatch, `list_tasks()` surfaces `TaskHealth::CalendarNotFound`.
  - **Fire-time re-fetch — event still present, unchanged**: reservation at t=0 captures event E with Zoom link A; at t=trigger, mock bridge returns E with link A; dispatch uses the freshly-fetched E (assert the executor receives the fresh copy, not the snapshot).
  - **Fire-time re-fetch — event edited**: reservation captures E with link A; mock bridge at dispatch time returns E with link B (title or notes changed); assert the executor receives the version with link B, confirming no stale snapshot fallback.
  - **Fire-time re-fetch — event cancelled**: reservation captures E; mock bridge at dispatch time no longer returns E by id; assert the dedup row transitions to `dispatched`, an `execution_logs` row with status `skipped` and reason `"event no longer exists"` is written, and no action runs.
  - **Fire-time re-fetch — event moved forward**: reservation captures E starting at 10:00 (trigger_at 09:55); before dispatch, event is moved to 11:00. Assert the dedup row is deleted (not marked dispatched), no action runs, and the next poll tick creates a fresh reservation for the new trigger_at.
- `libtakt/tests/task_health.rs` — unit tests for the health population logic in `list_tasks()`:
  - Non-calendar task always `Healthy`.
  - Calendar task with status `Authorized` + calendar present → `Healthy`.
  - Calendar task with status `Authorized` + calendar missing → `CalendarNotFound`.
  - Calendar task with status `Denied` → `CalendarAccessDenied` (no `list_calendars` call).
  - Calendar task with status `NotDetermined` → `CalendarAccessNotDetermined` (no `list_calendars` call).
  - Multiple calendar tasks → bridge is called **at most twice** total (`get_calendar_access_status` + `list_calendars`).
- `libtakt/src/url_extract.rs` unit tests: single URL, multiple URLs, dedup against conference URL, malformed strings ignored, empty notes, URLs in location.
- `libtakt/src/template.rs` unit tests: each supported variable, missing event context → empty substitution, unknown `{{event.xxx}}` preserved.

### 9.2 Swift

Manual QA for permission flows (first-time prompt, denial recovery, re-grant), since EventKit authorization is not easily mockable. Listed in a test plan section of the README.

## 10. Migration & Rollout

Each step below must build, pass tests, and ship a coherent user experience where the calendar feature is either **genuinely off** (no way to produce or observe it) or **fully on**. Half-on states that persist unusable tasks are explicitly forbidden.

The critical constraint: adding variants to `Schedule` and `Action` in `libtakt/src/models.rs` regenerates the UniFFI-exported Swift enums **and** breaks exhaustive matches on both sides. The rollout below lists every known call site that must be updated in the same commit as the enum extension.

### Step 1 — Enum variants, Rust stubs, schema, bridge contract (feature OFF)

**Guiding principle**: after this step, a user running the app cannot produce a `Schedule::Calendar` or `Action::OpenEventLinks` through any UI path. The enums exist purely so subsequent steps can fill them in without churning every file again. If a rogue row somehow exists in the DB (dev, migration from a future build, manual edit), the Rust arms handle it defensively without panic.

- Add migration `002_calendar.sql` (with the full `status`/`trigger_at`/`dispatched_at` schema).
- Add `Schedule::Calendar`, `Action::OpenEventLinks`, `CalendarEvent`, `CalendarInfo`, `CalendarAccessStatus`, `TaskHealth` to `libtakt/src/models.rs`.
- **Update `models.rs` custom deserializers** so the new variants round-trip:
  - `Action` custom deserialize (`models.rs:235` — `unknown_variant` list and branching): add `"OpenEventLinks"` case that constructs `Action::OpenEventLinks { open_conference: false, open_notes_links: false, browser: None }`.
  - `Schedule` deserializer: if custom, add the `Calendar` branch; if derive-based, no change needed (verify during implementation).
- Add `TaskDto.health` field (populated with `Healthy` for all tasks in this step — no bridge call yet).
- Extend `PlatformBridge` trait with the new methods. Provide a macOS implementation that returns stubbed values (`Err("not_implemented")` is fine) — no EventKit wiring yet. Keeps existing `PlatformBridge` consumers compiling.
- Add the three new `TaktCore` methods (`get_calendar_access_status`, `request_calendar_access`, `list_calendars`) as thin pass-throughs. They return errors in step 1; UI will not call them yet.
- **Add defensive no-op arms in every exhaustive Rust match over `Schedule` or `Action`.** Each arm logs a warning and returns early without side effects. The known call sites:
  - `libtakt/src/scheduler.rs:123` (`catch_up_missed` match on `task.schedule`) — `Schedule::Calendar { .. } => false` (never catches up in step 1).
  - `libtakt/src/scheduler.rs:170` (`schedule_task` match on `task.schedule`) — `Schedule::Calendar { .. } => { warn!("Schedule::Calendar not yet supported, task {id} will not run"); Ok(()) }`.
  - `libtakt/src/executor/macos.rs:27` (`execute` match on `action`) — `Action::OpenEventLinks { .. } => Err(ExecutorError::Unsupported("OpenEventLinks not yet supported".into()))`.
  - Run `cargo build` and address any other exhaustive matches surfaced by the compiler (this list is authoritative at spec time; plan time should re-verify).
- Regenerate UniFFI bindings.
- **Swift: hide the feature in the UI, don't just placeholder it.** The regenerated Swift enums force updates to every exhaustive switch, but we must also prevent the user from *producing* the new variants:
  - `ScheduleTypeTag` enum: **do not add a `.calendar` case in step 1.** Leaving it out means the type selector has no Calendar tab, the `handleTypeChange` switch in `ScheduleBuilderView.swift:299` remains three-cased and compiles, and there is no UI path to write `.calendar` into `schedule`. The Rust `Schedule::Calendar` variant is only reachable via the serde layer, which the UI never constructs.
  - `ActionTypeTag` enum: same approach — **do not add `.openEventLinks` in step 1.** No button, no `handleTypeChange` case (`ActionBuilderView.swift:661`), no way for the editor to write the variant.
  - The remaining exhaustive switches (on the Swift enum mirror itself, not on the tag enums) **must** be updated because the compiler forces them:
    - `ScheduleBuilderView.swift:10` (`scheduleType` computed property matching on `schedule`): add `case .calendar: return .cron` (falls back to an existing tag — the UI will never actually render this because the tag isn't in `allCases`, but it must compile). Include a `// TODO: step 4 — real Calendar tag` comment.
    - `ActionBuilderView.swift:14` (`actionType` computed): same pattern — `case .openEventLinks: return .openUrl` with the same comment.
    - `AutoName.swift` `describeAction` and `describeSchedule`: add cases returning `"Calendar event"` / `"Open event links"` so a serde-only task is still nameable in the list.
    - `TaskItemView.swift:134` (`actionLabel`, `actionColor`): add `.openEventLinks` returning `("event", .teal)` or similar.
    - Any other exhaustive switches the compiler surfaces.
  - `TaskEditorViewModel.save()` (`TaskEditorViewModel.swift:198`): add a guard at the top — `if case .calendar = schedule { throw …; }; if case .openEventLinks = action { throw …; }`. The editor UI can't produce these in step 1, but this guard is a belt-and-suspenders defense against hypothetical future paths (template import, etc.) reaching save before step 4.
  - Badge rendering (§7.6) is **not** added in step 1 — `TaskDto.health` is always `Healthy` for existing tasks, and Calendar tasks can't exist, so no badge code is needed.
- **User-visible effect**: zero. The app looks and behaves identically. Internally, the enums and schema are in place.
- **Tests**: all existing tests must still pass. No new tests yet. Add one Rust test that round-trips a `Schedule::Calendar` through serde to catch future regressions in the custom deserializer.

### Step 2 — EventKit adapter + permissions plumbing (feature OFF)

- Implement the stubbed `PlatformBridge` methods against `EKEventStore` in `macos/Takt/TaktCore+Calendar.swift`.
- Add `INFOPLIST_KEY_NSCalendarsFullAccessUsageDescription` to Debug/Release build settings of the Takt target.
- Populate `TaskDto.health` using the caching rules from §6.4. No calendar tasks exist yet, so health is always `Healthy`, but the code path is exercised on every `list_tasks()` call.
- UI remains unchanged from step 1 — still no way to produce `Schedule::Calendar` / `Action::OpenEventLinks`. The save-time guards from step 1 stay in place.
- **User-visible effect**: zero. The app can now query calendars and access state internally.
- **Tests**: Swift manual test plan for the permission prompt (documented but not automated, since EventKit auth isn't mockable). Rust tests for the health-population code path using a mock bridge.

### Step 3 — `CalendarPoller` + executor signature change + template vars (feature OFF)

- Implement `CalendarPoller`, `ensure_calendar_poller`, the `cancel_tokens` extension with `"calendar:{task}:{event}:{start}"` keys, startup reconstitute (§5.2.1), the executor trait signature change (`Option<&CalendarEvent>`), template substitution, and the `url_extract` module.
- Replace the defensive `warn!` arm in `scheduler.rs:170` for `Schedule::Calendar` with real logic that delegates to `ensure_calendar_poller`.
- Replace the `ExecutorError::Unsupported` arm for `Action::OpenEventLinks` with the real implementation (§5.5). This will never be reached in step 3 because the UI still can't produce the variant, but the code is ready for step 4 to flip on.
- Every existing call site of `ActionExecutor::execute` now passes `None` for the event context. Existing Cron/OneShot/DailyFirstUse tasks behave identically.
- UI still unchanged — save guards still in place.
- **User-visible effect**: zero. The engine is fully ready.
- **Tests**: the full §9.1 suite for the poller, template vars, URL extraction, and health population. Existing tests must still pass.

### Step 4 — Turn the feature on (UI + badges)

This is the step that exposes the feature to users. It is atomic: nothing about Calendar is visible before this step; everything is visible after.

- Add `.calendar` to `ScheduleTypeTag` and wire `handleTypeChange(.calendar)` in `ScheduleBuilderView.swift:299` to emit a default `Schedule::Calendar { calendar_id: "", title_contains: None, minutes_before: 5 }`.
- Add `.openEventLinks` to `ActionTypeTag` and wire `handleTypeChange` in `ActionBuilderView.swift:661` to emit the default `Action::OpenEventLinks { open_conference: true, open_notes_links: true, browser: None }`.
- Remove the `// TODO: step 4` fallback arms from `scheduleType` / `actionType` computed properties and return the real tag cases.
- Remove the save-time guards in `TaskEditorViewModel.save()` (the UI can now legitimately produce these variants).
- Implement the real `CalendarScheduleBuilder.swift` view and the `OpenEventLinks` action form in `ActionBuilderView`.
- Add validation: saving a `Calendar` schedule requires non-empty `calendar_id`; otherwise toast an error. Non-blocking inline warning if the user picks `OpenEventLinks` without `Calendar` schedule.
- Implement the health badges in `TaskItemView.swift:134` per §7.6. Replace the minimal `actionLabel`/`actionColor` entries for `.openEventLinks` with the real "event" label and teal color.
- Implement the real `AutoName` cases (§7.5), replacing the minimal strings from step 1.
- **User-visible effect**: calendar triggers and event-links actions are now fully usable end-to-end. The first time a user picks `Calendar`, they see a "Grant Calendar Access" button; clicking it triggers EventKit's native prompt (see §6.3 and §7.2). The app never prompts without an explicit user click.
- **Tests**: manual smoke test of the golden path — create a task, trigger fires on a real EventKit event, links open in the correct browser.

### Step 5 — Polish: template variable hints, quick-start template, README

- Add the variable hint lines below text fields when `Schedule::Calendar` is active (§7.4).
- Add the "Open meeting links" template to `TemplateGridView` (§7.8).
- Document the feature in the README.
- **User-visible effect**: discoverability improvements.

**Rollout invariant**: at the end of steps 1, 2, and 3, the feature is genuinely off — no UI path constructs the new variants, and both Rust and Swift save-time guards reject them defensively. Step 4 is the single atomic flip. This matches how the existing `DailyFirstUse` feature was shipped: the engine, the persistence, and the UI all landed together, avoiding half-on states where users could persist unusable tasks.

## 11. Open Questions

None at design time. Any unknowns surfaced during implementation should be raised as plan-stage decisions.
