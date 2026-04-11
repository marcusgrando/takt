# Calendar Trigger — Phase 1 Implementation Plan (Feature OFF)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the data model, migration, platform-bridge contract, and defensive plumbing for the Calendar trigger feature, keeping the feature **completely invisible and unreachable** from the UI. After this phase the app compiles, all existing tests pass, and users cannot create or see anything new — but the ground is prepared for Phases 2–5.

**Architecture:** New Rust enum variants (`Schedule::Calendar`, `Action::OpenEventLinks`), new model types (`CalendarEvent`, `CalendarInfo`, `CalendarAccessStatus`, `TaskHealth`), a migration adding a `calendar_dispatches` table, five new methods on the `PlatformBridge` trait (all stub-returning `Err("not_implemented")` on macOS for now), three new `TaktCore` API methods (also pass-throughs), defensive no-op arms in every exhaustive Rust match over `Schedule`/`Action`, fallback arms in every exhaustive Swift switch on the UniFFI-exported enums (but **no** new cases in `ScheduleTypeTag` or `ActionTypeTag`, so the tabs never appear), and belt-and-suspenders save guards in `TaskEditorViewModel`.

**Tech Stack:** Rust (`libtakt` crate), SQLite via `sqlx`, UniFFI for Rust↔Swift bindings, SwiftUI (`macos/Takt`), Xcode project generating `Info.plist` via `INFOPLIST_KEY_*` build settings.

**Spec reference:** `docs/superpowers/specs/2026-04-10-calendar-trigger-design.md` §10 "Step 1".

**Working directory:** `/Users/marcus.grando/git/cronmac`

**Rollout invariant for this phase:** at the end of Phase 1 a user launching the app MUST see zero change. No new tabs, no new actions, no new badges. If the invariant is violated, revert and re-read the spec §10 Step 1.

---

## Task 1: Create the `calendar_dispatches` migration

**Files:**
- Create: `libtakt/migrations/002_calendar.sql`

Only a SQL file — no Rust changes yet, so nothing to test besides "`sqlx` applies it on next boot".

- [ ] **Step 1: Write the migration**

Create `libtakt/migrations/002_calendar.sql` with this exact content:

```sql
-- Phase 1 of calendar-trigger feature: dedup + reconstitute table.
-- The table is populated only after Phase 3 lands the CalendarPoller.
-- In Phase 1 it stays empty; creating it now avoids a later schema churn.

CREATE TABLE calendar_dispatches (
    task_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_start TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('scheduled', 'dispatched')),
    trigger_at TEXT NOT NULL,
    reserved_at TEXT NOT NULL,
    dispatched_at TEXT,
    PRIMARY KEY (task_id, event_id, event_start)
);

CREATE INDEX idx_calendar_dispatches_status ON calendar_dispatches(status);
CREATE INDEX idx_calendar_dispatches_start ON calendar_dispatches(event_start);
```

- [ ] **Step 2: Verify the migration is discoverable**

Run: `ls -1 libtakt/migrations/`
Expected output: includes both `001_initial.sql` and `002_calendar.sql`.

- [ ] **Step 3: Smoke-test that `sqlx` accepts the migration**

Run: `cargo test -p libtakt --lib -- --nocapture db::` if any tests exist in `db.rs`; otherwise run the full suite:
Run: `cargo test -p libtakt`
Expected: all existing tests still pass. `sqlx` runs migrations during pool setup; if the SQL is malformed the first DB-backed test will fail with a clear error.

- [ ] **Step 4: Commit**

```bash
git add libtakt/migrations/002_calendar.sql
git commit -m "feat(calendar): add 002_calendar.sql migration with dispatches table"
```

---

## Task 2: Add calendar model types to `models.rs`

**Files:**
- Modify: `libtakt/src/models.rs` (add new types after line 73, before the `Action` enum)

- [ ] **Step 1: Add the new types**

Open `libtakt/src/models.rs`. Find the `KeyCombo` struct (around line 69–73). Insert the following block immediately after its closing `}`, before the line `#[derive(Debug, Clone, uniffi::Enum)]` that begins the `Action` enum:

```rust
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p libtakt`
Expected: builds cleanly. No new warnings beyond pre-existing ones.

- [ ] **Step 3: Commit**

```bash
git add libtakt/src/models.rs
git commit -m "feat(calendar): add CalendarEvent, CalendarInfo, TaskHealth, CalendarAccessStatus types"
```

---

## Task 3: Add `Schedule::Calendar` variant + defensive match arms

**Files:**
- Modify: `libtakt/src/models.rs` (the `Schedule` enum, line 48)
- Modify: `libtakt/src/scheduler.rs:123` (`catch_up_missed` match)
- Modify: `libtakt/src/scheduler.rs:170` (`schedule_task` match)

The variant and all its defensive match arms must land together, otherwise `cargo check` breaks between tasks.

- [ ] **Step 1: Add the `Calendar` variant to `Schedule`**

In `libtakt/src/models.rs`, replace the existing `Schedule` enum definition (currently lines 46–59):

```rust
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
```

Replace with:

```rust
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
```

(The `#[cfg_attr(test, derive(PartialEq))]` is added to match the style used on `Action`.)

- [ ] **Step 2: Run `cargo check` to see it fail**

Run: `cargo check -p libtakt`
Expected: compilation errors in `libtakt/src/scheduler.rs` about non-exhaustive matches over `Schedule`. This confirms the call sites we need to update.

- [ ] **Step 3: Add defensive arm in `catch_up_missed`**

In `libtakt/src/scheduler.rs`, find the match statement starting at line 123 inside `catch_up_missed`. It currently looks like:

```rust
let should_catch_up = match &task.schedule {
    Schedule::Cron { expression } => {
        missed_cron_run(expression, task.last_run_at.as_deref())
    }
    Schedule::OneShot { run_at } => {
        // ...
    }
    Schedule::DailyFirstUse { .. } => false, // has its own logic
};
```

Add a new arm just before the closing brace of the match, after `Schedule::DailyFirstUse`:

```rust
    Schedule::Calendar { .. } => false, // Phase 1 stub: never catches up; real logic lands in Phase 3
```

- [ ] **Step 4: Add defensive arm in `schedule_task`**

Still in `libtakt/src/scheduler.rs`, find the `match &task.schedule { ... }` block inside `schedule_task` (starts at line 170). Add this arm at the end, after the `Schedule::DailyFirstUse { .. }` arm that ends around line 268 with `});`:

```rust
    Schedule::Calendar { .. } => {
        // Phase 1 stub: calendar scheduling lands in Phase 3.
        // We intentionally do nothing so a rogue Schedule::Calendar row in the DB
        // (e.g. from a future build) cannot crash startup.
        tracing::warn!(
            task_id = %task_id,
            "Schedule::Calendar is not yet supported (Phase 1 stub); task will not fire"
        );
    }
```

(If `tracing` is not already imported at the top of `scheduler.rs`, verify its usage — the existing `daily_first_use_loop` at line 294+ almost certainly already imports it. If not, add `use tracing::warn;` at the top and use `warn!(...)`.)

- [ ] **Step 5: Run `cargo check` to verify green**

Run: `cargo check -p libtakt`
Expected: clean build.

- [ ] **Step 6: Run the existing test suite**

Run: `cargo test -p libtakt`
Expected: all existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add libtakt/src/models.rs libtakt/src/scheduler.rs
git commit -m "feat(calendar): add Schedule::Calendar variant with Phase-1 no-op arms"
```

---

## Task 4: Add `Action::OpenEventLinks` variant + update custom ser/de + defensive executor arm

**Files:**
- Modify: `libtakt/src/models.rs` (the `Action` enum at line 75, plus its custom `Serialize` at line 118 and `Deserialize` at line 182)
- Modify: `libtakt/src/executor/macos.rs:27` (`execute` match)

- [ ] **Step 1: Add the `OpenEventLinks` variant to `Action`**

In `libtakt/src/models.rs`, the `Action` enum ends around line 114 with `}`. Add a new variant as the last entry, after `Settings { pane_url: String }`:

```rust
    OpenEventLinks {
        open_conference: bool,
        open_notes_links: bool,
        browser: Option<String>,
    },
```

- [ ] **Step 2: Add the serialize branch**

In the `impl Serialize for Action` block (starts line 118), add a new match arm at the end of the inner match (after the `Action::Settings { pane_url }` arm that ends around line 177):

```rust
            Action::OpenEventLinks { open_conference, open_notes_links, browser } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "OpenEventLinks")?;
                map.serialize_entry("open_conference", open_conference)?;
                map.serialize_entry("open_notes_links", open_notes_links)?;
                map.serialize_entry("browser", browser)?;
                map.end()
            }
```

- [ ] **Step 3: Add the deserialize branch**

In the `impl<'de> Deserialize<'de> for Action` block (starts line 182), find the match inside `deserialize` that dispatches on `action_type` (around line 188). Add a new arm before the `other => Err(de::Error::unknown_variant(...))` arm:

```rust
            "OpenEventLinks" => Ok(Action::OpenEventLinks {
                open_conference: obj.get("open_conference").and_then(|v| v.as_bool()).unwrap_or(true),
                open_notes_links: obj.get("open_notes_links").and_then(|v| v.as_bool()).unwrap_or(true),
                browser: obj.get("browser").and_then(|v| v.as_str()).map(String::from),
            }),
```

- [ ] **Step 4: Update the `unknown_variant` list**

Still in `libtakt/src/models.rs`, update line 235 from:

```rust
            other => Err(de::Error::unknown_variant(other, &["OpenFile", "OpenUrl", "OpenApp", "RunCommand", "Notify", "Webhook", "Settings"])),
```

to:

```rust
            other => Err(de::Error::unknown_variant(other, &["OpenFile", "OpenUrl", "OpenApp", "RunCommand", "Notify", "Webhook", "Settings", "OpenEventLinks"])),
```

- [ ] **Step 5: Run `cargo check` to see the executor break**

Run: `cargo check -p libtakt`
Expected: compilation error in `libtakt/src/executor/macos.rs` about non-exhaustive match over `Action`.

- [ ] **Step 6: Add defensive arm in `execute`**

In `libtakt/src/executor/macos.rs`, find the `match action { ... }` block inside `execute` (line 27). Find the final `Action::Settings { pane_url } => { ... }` arm and add this arm right after it:

```rust
            Action::OpenEventLinks { .. } => Err(ExecutorError::Unsupported(
                "OpenEventLinks is not yet implemented (Phase 1 stub — lands in Phase 4)".into()
            )),
```

(Verify `ExecutorError::Unsupported(String)` already exists in the `ExecutorError` enum — it's mentioned in the spec §8. If it doesn't, add it to `libtakt/src/executor/mod.rs` or wherever `ExecutorError` is defined, as `Unsupported(String)` with a `#[error("unsupported action: {0}")]` line.)

- [ ] **Step 7: Run `cargo check` to verify green**

Run: `cargo check -p libtakt`
Expected: clean build.

- [ ] **Step 8: Run the existing test suite**

Run: `cargo test -p libtakt`
Expected: all existing tests pass.

- [ ] **Step 9: Commit**

```bash
git add libtakt/src/models.rs libtakt/src/executor/macos.rs libtakt/src/executor/mod.rs
git commit -m "feat(calendar): add Action::OpenEventLinks variant with Phase-1 stub executor arm"
```

(Drop `libtakt/src/executor/mod.rs` from the `git add` if you didn't need to touch it.)

---

## Task 5: Extend `TaskDto` with `health` field + update `to_dto`

**Files:**
- Modify: `libtakt/src/models.rs` (the `TaskDto` struct around line 30, and the `Task::to_dto` method around line 294)

- [ ] **Step 1: Add the `health` field to `TaskDto`**

In `libtakt/src/models.rs`, the `TaskDto` struct ends around line 44. Add a new field immediately before the closing `}`:

```rust
    pub health: TaskHealth,
```

The full struct now reads (for reference — do not duplicate):

```rust
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
    pub health: TaskHealth,
}
```

- [ ] **Step 2: Populate `health` in `Task::to_dto`**

In `libtakt/src/models.rs`, find the `to_dto` method (around line 294). Currently it builds `TaskDto { ... }` with every field except `health`. Add `health: TaskHealth::Healthy,` as the last field inside the struct literal, just before `})`:

```rust
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
            health: TaskHealth::Healthy, // Phase 1: always Healthy. Phase 2 populates based on platform bridge.
        })
    }
```

- [ ] **Step 3: Run `cargo check`**

Run: `cargo check -p libtakt`
Expected: clean build. (`TaskDto` is constructed only in this one place — the store returns DTOs by calling `to_dto`. Nothing else should break.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p libtakt`
Expected: all existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add libtakt/src/models.rs
git commit -m "feat(calendar): add TaskDto.health field, defaulted to Healthy in Phase 1"
```

---

## Task 6: Serde round-trip tests for the new variants

**Files:**
- Modify: `libtakt/src/models.rs` (append to the existing `#[cfg(test)] mod tests` block at the bottom, or create one if none exists)

- [ ] **Step 1: Find or create the test module**

Open `libtakt/src/models.rs`. Scroll to the very bottom. If there is already a `#[cfg(test)] mod tests { ... }` block, append to it. If not, add this at the end of the file:

```rust
#[cfg(test)]
mod phase1_calendar_tests {
    use super::*;

    #[test]
    fn schedule_calendar_round_trips_through_serde() {
        let schedule = Schedule::Calendar {
            calendar_id: "cal-abc-123".to_string(),
            title_contains: Some("standup".to_string()),
            minutes_before: 5,
        };
        let json = serde_json::to_string(&schedule).expect("serialize");
        let parsed: Schedule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(schedule, parsed);
    }

    #[test]
    fn schedule_calendar_title_contains_none_round_trips() {
        let schedule = Schedule::Calendar {
            calendar_id: "cal-x".to_string(),
            title_contains: None,
            minutes_before: 0,
        };
        let json = serde_json::to_string(&schedule).expect("serialize");
        let parsed: Schedule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(schedule, parsed);
    }

    #[test]
    fn action_open_event_links_round_trips_through_serde() {
        let action = Action::OpenEventLinks {
            open_conference: true,
            open_notes_links: false,
            browser: Some("Safari".to_string()),
        };
        let json = serde_json::to_string(&action).expect("serialize");
        let parsed: Action = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(action, parsed);
    }

    #[test]
    fn action_open_event_links_defaults_when_fields_missing() {
        // Simulates an older build or a hand-edited JSON with only the type tag.
        let json = r#"{"type":"OpenEventLinks"}"#;
        let parsed: Action = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            parsed,
            Action::OpenEventLinks {
                open_conference: true,
                open_notes_links: true,
                browser: None,
            }
        );
    }

    #[test]
    fn calendar_event_round_trips_through_serde() {
        let event = CalendarEvent {
            id: "evt-1".to_string(),
            title: "Team standup".to_string(),
            start: "2026-04-12T09:00:00Z".to_string(),
            end: "2026-04-12T09:30:00Z".to_string(),
            notes: Some("Agenda: https://example.com/doc".to_string()),
            location: None,
            url: None,
            conference_url: Some("https://meet.google.com/abc-defg-hij".to_string()),
            calendar_id: "cal-abc-123".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let parsed: CalendarEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, parsed);
    }
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test -p libtakt phase1_calendar_tests`
Expected: 5 tests pass.

- [ ] **Step 3: Run the full suite to catch regressions**

Run: `cargo test -p libtakt`
Expected: everything passes.

- [ ] **Step 4: Commit**

```bash
git add libtakt/src/models.rs
git commit -m "test(calendar): add Phase 1 serde round-trip tests for new variants"
```

---

## Task 7: Extend `PlatformBridge` trait + add Swift stub implementations

**Files:**
- Modify: `libtakt/src/platform.rs` (the `PlatformBridge` trait at line 8)
- Modify: `macos/Takt/TaktCore+Bridge.swift` (the `MacOSPlatformBridge` class at line 8)

The Rust trait and the Swift impl MUST land in the same commit because UniFFI's `with_foreign` breaks Swift compilation the moment a new trait method appears.

- [ ] **Step 1: Extend the Rust trait**

In `libtakt/src/platform.rs`, replace the trait block (currently lines 8–21):

```rust
#[uniffi::export(with_foreign)]
pub trait PlatformBridge: Send + Sync {
    /// Send a native notification.
    fn send_notification(&self, title: String, body: String, sound: bool);

    /// Execute a registered callback on the main thread.
    /// Swift MUST use DispatchQueue.main.async (not .sync) to avoid deadlock,
    /// then call `execute_callback(callbackId)` to run the closure.
    fn run_on_main_sync(&self, callback_id: u64);

    /// Returns true when the user is actively present: screen is unlocked
    /// AND there has been HID input (keyboard/mouse) within `idle_threshold_secs`.
    fn is_user_active(&self, idle_threshold_secs: u64) -> bool;
}
```

With:

```rust
#[uniffi::export(with_foreign)]
pub trait PlatformBridge: Send + Sync {
    /// Send a native notification.
    fn send_notification(&self, title: String, body: String, sound: bool);

    /// Execute a registered callback on the main thread.
    /// Swift MUST use DispatchQueue.main.async (not .sync) to avoid deadlock,
    /// then call `execute_callback(callbackId)` to run the closure.
    fn run_on_main_sync(&self, callback_id: u64);

    /// Returns true when the user is actively present: screen is unlocked
    /// AND there has been HID input (keyboard/mouse) within `idle_threshold_secs`.
    fn is_user_active(&self, idle_threshold_secs: u64) -> bool;

    // ── Calendar methods (Phase 1: stubs; Phase 2: real EventKit impls) ──

    /// Returns the current user permission state for EventKit access.
    /// Callers MUST use this instead of inferring from `list_calendars()` output.
    fn get_calendar_access_status(&self) -> Result<crate::models::CalendarAccessStatus, String>;

    /// Triggers the native permission prompt (user-gated via an explicit button click).
    /// Returns the resulting status after the prompt closes.
    fn request_calendar_access(&self) -> Result<crate::models::CalendarAccessStatus, String>;

    /// Lists calendars the user can select as trigger sources.
    /// Returns `Err("calendar_access_denied")` if access is not granted.
    /// An empty vec is a legitimate "authorized user with zero calendars" state.
    fn list_calendars(&self) -> Result<Vec<crate::models::CalendarInfo>, String>;

    /// Fetches events in a time window around `now`, used by the poller to
    /// discover upcoming and recently-past events for reservation and catch-up.
    fn fetch_events_in_window(
        &self,
        calendar_id: String,
        lookback_minutes: u32,
        lookahead_minutes: u32,
    ) -> Result<Vec<crate::models::CalendarEvent>, String>;

    /// Recurrence-safe lookup of a specific event occurrence at dispatch time.
    /// On macOS, uses `predicateForEvents` over ±6h around `event_start`
    /// filtered by `eventIdentifier`, not `EKEventStore.event(withIdentifier:)`
    /// which would return only the first occurrence of recurring events.
    fn fetch_event_instance(
        &self,
        calendar_id: String,
        event_id: String,
        event_start: String,
    ) -> Result<Option<crate::models::CalendarEvent>, String>;
}
```

- [ ] **Step 2: Run `cargo check` to confirm Rust compiles**

Run: `cargo check -p libtakt`
Expected: clean build. (Rust has no consumers of these methods yet; Swift is what needs updating, and Swift doesn't compile until UniFFI regenerates.)

- [ ] **Step 3: Regenerate UniFFI bindings**

Run: `make build-rust`
Expected: Rust compiles, UniFFI-generated Swift file (`macos/Takt/libtakt.swift` or similar path — the Makefile knows where) is updated to include the new protocol methods.

- [ ] **Step 4: Add stub implementations in `MacOSPlatformBridge`**

In `macos/Takt/TaktCore+Bridge.swift`, find the `MacOSPlatformBridge` class (line 8). After the existing `isUserActive` method (ends around line 47), inside the same class, add these stubs before the closing `}` of the class:

```swift
    // MARK: - Calendar (Phase 1 stubs — real EventKit impls land in Phase 2)

    func getCalendarAccessStatus() throws -> CalendarAccessStatus {
        throw NSError(
            domain: "MacOSPlatformBridge",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "not_implemented"]
        )
    }

    func requestCalendarAccess() throws -> CalendarAccessStatus {
        throw NSError(
            domain: "MacOSPlatformBridge",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "not_implemented"]
        )
    }

    func listCalendars() throws -> [CalendarInfo] {
        throw NSError(
            domain: "MacOSPlatformBridge",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "not_implemented"]
        )
    }

    func fetchEventsInWindow(
        calendarId: String,
        lookbackMinutes: UInt32,
        lookaheadMinutes: UInt32
    ) throws -> [CalendarEvent] {
        throw NSError(
            domain: "MacOSPlatformBridge",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "not_implemented"]
        )
    }

    func fetchEventInstance(
        calendarId: String,
        eventId: String,
        eventStart: String
    ) throws -> CalendarEvent? {
        throw NSError(
            domain: "MacOSPlatformBridge",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "not_implemented"]
        )
    }
```

**Note on UniFFI error mapping**: the Rust trait methods return `Result<T, String>`. UniFFI with `with_foreign` converts this to a Swift `throws` function where the thrown error's `localizedDescription` becomes the Rust `String`. The `NSError` above uses `NSLocalizedDescriptionKey: "not_implemented"` so the Rust side receives `Err("not_implemented")`. Verify the exact Swift signature by reading the regenerated binding file — UniFFI may prefer a different error type (e.g., a generated enum). If the types don't match what's above, match the generated signature exactly and throw in the way the generated protocol expects.

- [ ] **Step 5: Build the full app**

Run: `make build`
Expected: Xcode Release build succeeds. If the UniFFI-generated types differ from the Swift stubs above, Xcode will error — adjust the stub signatures to match the generated protocol.

- [ ] **Step 6: Commit**

```bash
git add libtakt/src/platform.rs macos/Takt/TaktCore+Bridge.swift
# Also stage any regenerated UniFFI binding files if the build regenerates them:
git add macos/Takt/libtakt.swift 2>/dev/null || true
git commit -m "feat(calendar): extend PlatformBridge with stub calendar methods"
```

---

## Task 8: Add pass-through `TaktCore` methods

**Files:**
- Modify: `libtakt/src/lib.rs` (in the `impl TaktCore` block, near the `list_tasks` method around line 125)

- [ ] **Step 1: Locate the TaktCore impl**

Open `libtakt/src/lib.rs`. `TaktCore` owns `bridge: Arc<dyn PlatformBridge>` directly (line 44) — no helper needed, use `self.bridge.clone()` like `start()` does at line 85. The `#[uniffi::export] impl TaktCore { ... }` block at line 67 auto-exports every method inside it, so new methods added there are automatically reachable from Swift. Find `pub async fn list_tasks(&self)` (around line 125); the three new methods land right after it.

- [ ] **Step 2: Add the three pass-through methods**

Insert these methods inside the `#[uniffi::export] impl TaktCore { ... }` block, after `list_tasks()` and before `get_task()`:

```rust
    // ── Calendar APIs ─────────────────────────────────────────────────
    // Phase 1: thin pass-throughs to a stub bridge (returns "not_implemented").
    // Phase 2 replaces the bridge with a real EventKit-backed implementation;
    // these method bodies do not change.

    pub async fn get_calendar_access_status(&self) -> Result<models::CalendarAccessStatus, TaktError> {
        let bridge = self.bridge.clone();
        tokio_runtime()
            .spawn(async move {
                bridge
                    .get_calendar_access_status()
                    .map_err(|e| TaktError::CalendarBridge { msg: e })
            })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }

    pub async fn request_calendar_access(&self) -> Result<models::CalendarAccessStatus, TaktError> {
        let bridge = self.bridge.clone();
        tokio_runtime()
            .spawn(async move {
                bridge
                    .request_calendar_access()
                    .map_err(|e| TaktError::CalendarBridge { msg: e })
            })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }

    pub async fn list_calendars(&self) -> Result<Vec<models::CalendarInfo>, TaktError> {
        let bridge = self.bridge.clone();
        tokio_runtime()
            .spawn(async move {
                bridge
                    .list_calendars()
                    .map_err(|e| TaktError::CalendarBridge { msg: e })
            })
            .await
            .map_err(|e| TaktError::Database { msg: e.to_string() })?
    }
```

Verify the `models::` path resolves from `lib.rs` — the file already uses `use crate::models::{...}` or similar near the top. Either reference the types through whatever path is already in scope, or add explicit `use crate::models::{CalendarAccessStatus, CalendarInfo};`.

- [ ] **Step 3: Add the `CalendarBridge` error variant**

Open `libtakt/src/error.rs` (or wherever `TaktError` is defined). Find the enum and add a new variant:

```rust
    #[error("calendar bridge error: {msg}")]
    CalendarBridge { msg: String },
```

Also add `CalendarAccessDenied` and `CalendarNotFound` for Phase 2/3 consumers:

```rust
    #[error("calendar access denied")]
    CalendarAccessDenied,

    #[error("calendar not found: {id}")]
    CalendarNotFound { id: String },
```

- [ ] **Step 4: Confirm UniFFI export**

The `#[uniffi::export] impl TaktCore` block at `lib.rs:67` exports every method inside it automatically. No extra attribute is needed on the three new methods. Just double-check they are *inside* that impl block, not a separate plain `impl TaktCore { ... }` block (if they are in a separate block, move them into the exported one).

- [ ] **Step 5: Run `cargo check`**

Run: `cargo check -p libtakt`
Expected: clean build.

- [ ] **Step 6: Run the test suite**

Run: `cargo test -p libtakt`
Expected: all existing tests pass.

- [ ] **Step 7: Regenerate bindings and build the app**

Run: `make build-rust && make build`
Expected: Xcode build succeeds. The Swift side now sees three new `core.getCalendarAccessStatus() async throws -> CalendarAccessStatus` etc. methods, but no Swift code calls them yet — so the build should still pass.

- [ ] **Step 8: Commit**

```bash
git add libtakt/src/lib.rs libtakt/src/error.rs
git add macos/Takt/libtakt.swift 2>/dev/null || true
git commit -m "feat(calendar): add TaktCore pass-through methods for calendar bridge"
```

---

## Task 9: Fix Swift exhaustive switches on `Schedule` and `Action`

**Files:**
- Modify: `macos/Takt/Views/ScheduleBuilderView.swift:10`
- Modify: `macos/Takt/Views/ActionBuilderView.swift:14`
- Modify: `macos/Takt/Helpers/AutoName.swift` (`describeAction` and `describeSchedule`)
- Modify: `macos/Takt/Views/TaskItemView.swift:134` (`actionLabel` and `actionColor`)

**Critical**: do NOT add `.calendar` to `ScheduleTypeTag` or `.openEventLinks` to `ActionTypeTag`. Those enums stay three-cased / seven-cased respectively. The switches below are over the UniFFI-exported `Schedule` / `Action` enums, which the compiler now forces us to update — we return a fallback existing tag with a `// TODO: Phase 4` marker so the compiler is happy and no new tab ever appears in the UI.

- [ ] **Step 1a: Update the `scheduleType` computed property in `ScheduleBuilderView.swift`**

In `macos/Takt/Views/ScheduleBuilderView.swift`, replace the `scheduleType` computed property (lines 9–15):

```swift
    private var scheduleType: ScheduleTypeTag {
        switch schedule {
        case .cron: return .cron
        case .oneShot: return .oneShot
        case .dailyFirstUse: return .dailyFirstUse
        }
    }
```

With:

```swift
    private var scheduleType: ScheduleTypeTag {
        switch schedule {
        case .cron: return .cron
        case .oneShot: return .oneShot
        case .dailyFirstUse: return .dailyFirstUse
        // TODO: Phase 4 — add `.calendar` to ScheduleTypeTag and return it here.
        // Phase 1 keeps Calendar schedules unreachable from the UI by falling
        // back to an existing tag. This branch should never execute because
        // no Phase-1 code path produces a Schedule::Calendar value.
        case .calendar: return .cron
        }
    }
```

- [ ] **Step 1b: Update the body switch in `ScheduleBuilderView.swift`**

Still in `macos/Takt/Views/ScheduleBuilderView.swift`, find the second exhaustive switch around line 47 that renders the type-specific sub-content:

```swift
            // Type-specific content
            switch schedule {
            case .cron:
                cronContent
            case .oneShot:
                oneShotContent
            case .dailyFirstUse:
                dailyFirstUseContent
            }
```

Replace with:

```swift
            // Type-specific content
            switch schedule {
            case .cron:
                cronContent
            case .oneShot:
                oneShotContent
            case .dailyFirstUse:
                dailyFirstUseContent
            // TODO: Phase 4 — render CalendarScheduleBuilder here. Phase 1
            // shows the same content as the Cron tab because no UI path
            // produces a Schedule::Calendar value (the tag is not in allCases).
            case .calendar:
                cronContent
            }
```

- [ ] **Step 2a: Update the `actionType` computed property in `ActionBuilderView.swift`**

In `macos/Takt/Views/ActionBuilderView.swift`, replace the `actionType` computed property (lines 13–23):

```swift
    private var actionType: ActionTypeTag {
        switch action {
        case .openUrl: return .openUrl
        case .openFile: return .openFile
        case .openApp: return .openApp
        case .runCommand: return .runCommand
        case .notify: return .notify
        case .webhook: return .webhook
        case .settings: return .settings
        }
    }
```

With:

```swift
    private var actionType: ActionTypeTag {
        switch action {
        case .openUrl: return .openUrl
        case .openFile: return .openFile
        case .openApp: return .openApp
        case .runCommand: return .runCommand
        case .notify: return .notify
        case .webhook: return .webhook
        case .settings: return .settings
        // TODO: Phase 4 — add `.openEventLinks` to ActionTypeTag and return it here.
        // Phase 1 keeps this action unreachable from the UI by falling back to
        // `.openUrl`. Never reached in Phase 1 because no UI path produces it.
        case .openEventLinks: return .openUrl
        }
    }
```

- [ ] **Step 2b: Update the body switch in `ActionBuilderView.swift`**

Still in `macos/Takt/Views/ActionBuilderView.swift`, find the second exhaustive switch around line 49 that renders the type-specific fields:

```swift
            // Type-specific fields
            switch action {
            case .openFile:
                openFileFields
            case .openUrl:
                openUrlFields
            case .openApp:
                openAppFields
            case .runCommand:
                runCommandFields
            case .notify:
                notifyFields
            case .webhook:
                webhookFields
            case .settings:
                settingsFields
            }
```

Replace with:

```swift
            // Type-specific fields
            switch action {
            case .openFile:
                openFileFields
            case .openUrl:
                openUrlFields
            case .openApp:
                openAppFields
            case .runCommand:
                runCommandFields
            case .notify:
                notifyFields
            case .webhook:
                webhookFields
            case .settings:
                settingsFields
            // TODO: Phase 4 — render the OpenEventLinks form here. Phase 1
            // renders the OpenUrl fields as a harmless fallback; unreachable
            // because no UI path produces an Action::OpenEventLinks value.
            case .openEventLinks:
                openUrlFields
            }
```

- [ ] **Step 3: Update `AutoName.swift` — `describeAction`**

In `macos/Takt/Helpers/AutoName.swift`, find the `describeAction(_ action: Action)` function (around line 11–?). The existing switch ends with `case .settings:` returning some string. Add a new case before the closing `}`:

```swift
        case .openEventLinks:
            return "Open event links"
```

- [ ] **Step 4: Update `AutoName.swift` — `describeSchedule`**

In the same file, find `describeSchedule(_ schedule: Schedule)` and add a new case at the end:

```swift
        case .calendar:
            return "Calendar event"
```

- [ ] **Step 5: Update `TaskItemView.swift` — `actionLabel`**

In `macos/Takt/Views/TaskItemView.swift:134`, the `actionLabel` computed property ends around line 143 with `case .settings: return "settings"`. Add a new case before the closing `}`:

```swift
        case .openEventLinks: return "event"
```

- [ ] **Step 6: Update `TaskItemView.swift` — `actionColor`**

In the same file, find `actionColor` (around line 146). Add a new case at the end of its switch:

```swift
        case .openEventLinks: return .teal
```

- [ ] **Step 7: Build the full app**

Run: `make build`
Expected: Xcode build succeeds. No visible changes in the UI because the tag enums were never extended.

- [ ] **Step 8: Smoke-test the app manually**

Run: `open /Applications/Takt.app` (or wherever the Release build lands — check `make build` output).
Click the menu bar icon. Open the editor via "+ New task". Verify:
- Schedule tabs: exactly three (Cron, One-shot, Daily first use). **No Calendar tab.**
- Action tabs: exactly seven (Open URL, Open File, Open App, Run Command, Notify, Webhook, Settings). **No Open Event Links button.**
- Task list: no badges, no new columns.

If anything unexpected appears, stop and investigate — the Phase 1 invariant is broken.

- [ ] **Step 9: Commit**

```bash
git add macos/Takt/Views/ScheduleBuilderView.swift \
        macos/Takt/Views/ActionBuilderView.swift \
        macos/Takt/Helpers/AutoName.swift \
        macos/Takt/Views/TaskItemView.swift
git commit -m "feat(calendar): fallback Swift switches for Phase 1 (feature stays off in UI)"
```

---

## Task 10: Add save guards in `TaskEditorViewModel`

**Files:**
- Modify: `macos/Takt/ViewModels/TaskEditorViewModel.swift` (the `save()` method around line 134)

- [ ] **Step 1: Locate the save method**

Open `macos/Takt/ViewModels/TaskEditorViewModel.swift`. Find:

```swift
    @MainActor
    func save() async -> Bool {
        error = nil
```

(around line 134).

- [ ] **Step 2: Add the guards immediately after `error = nil`**

Insert this block right after `error = nil` and before `let finalName = ...` (or wherever the existing validation starts):

```swift
        // Phase 1 belt-and-suspenders: the editor UI cannot produce these
        // variants (ScheduleTypeTag / ActionTypeTag do not expose them), but
        // guard against template imports, future dev paths, or rogue
        // deserialization reaching save() before Phase 4 turns the feature on.
        if case .calendar = schedule {
            self.error = "Calendar schedules are not yet supported"
            return false
        }
        if case .openEventLinks = action {
            self.error = "Open Event Links action is not yet supported"
            return false
        }
```

- [ ] **Step 3: Build the full app**

Run: `make build`
Expected: Xcode build succeeds.

- [ ] **Step 4: Smoke-test normal task creation**

Open the app. Create a new task with Schedule=Cron, Action=OpenUrl pointing at `https://example.com`. Save it. Verify:
- Task appears in the list.
- Task runs at the scheduled time (or use "Run Now" to trigger it immediately).
- Nothing about calendar appears anywhere.

- [ ] **Step 5: Commit**

```bash
git add macos/Takt/ViewModels/TaskEditorViewModel.swift
git commit -m "feat(calendar): Phase 1 save-time guards against Calendar/OpenEventLinks"
```

---

## Task 11: Full Phase 1 verification and rollout commit

**Files:**
- None (verification-only task)

- [ ] **Step 1: Run the full Rust suite**

Run: `cargo test -p libtakt`
Expected: all tests pass, including the new `phase1_calendar_tests` from Task 6.

- [ ] **Step 2: Run `cargo clippy` to catch lints**

Run: `cargo clippy -p libtakt --all-targets -- -D warnings`
Expected: no new clippy warnings introduced by Phase 1.

- [ ] **Step 3: Build the full app in Release mode**

Run: `make build`
Expected: Xcode Release build succeeds without warnings related to the new code.

- [ ] **Step 4: Manual smoke test — Phase 1 invariant check**

Launch the built app. Walk through the invariant checklist:

- [ ] Menu bar icon appears as before.
- [ ] Task list shows existing tasks (if any) unchanged. No new columns, no new badges.
- [ ] Opening the editor shows exactly 3 schedule tabs: Cron, One-shot, Daily first use.
- [ ] Opening the editor shows exactly 7 action tabs: Open URL, Open File, Open App, Run Command, Notify, Webhook, Settings.
- [ ] Creating a task with Cron + OpenUrl still works and runs.
- [ ] Existing tasks still fire on schedule (spot-check the nearest one or use "Run Now").
- [ ] App does not prompt for calendar access on launch.
- [ ] No new dialogs, badges, banners, or UI elements anywhere.

If ANY of these checklist items fails, stop, diagnose, and fix before the final commit. The Phase 1 contract is "zero user-visible change."

- [ ] **Step 5: Verify the database migration applied cleanly**

Run: `sqlite3 ~/.local/share/takt/takt-dev.db ".schema calendar_dispatches"` (use `takt.db` instead of `takt-dev.db` for Release builds).
Expected: the table schema is printed, matching the migration from Task 1.

- [ ] **Step 6: Create a Phase 1 completion commit (empty or summary)**

```bash
git commit --allow-empty -m "chore(calendar): Phase 1 complete — feature scaffolded, UI off

All enum variants, schema, platform-bridge contract, and defensive
stubs are in place. No user-visible change. Phases 2–5 turn the
feature on incrementally."
```

- [ ] **Step 7: Tag the Phase 1 commit for easy rollback**

```bash
git tag calendar-phase-1-complete
```

---

## Summary

At the end of Phase 1:

- **Database**: `calendar_dispatches` table exists and is empty.
- **Rust models**: `Schedule::Calendar`, `Action::OpenEventLinks`, `CalendarEvent`, `CalendarInfo`, `CalendarAccessStatus`, `TaskHealth` all defined and serde round-trippable.
- **Rust scheduler/executor**: defensive no-op arms for the new variants. A rogue DB row with `Schedule::Calendar` logs a warning and is skipped without crash; a rogue `Action::OpenEventLinks` returns `ExecutorError::Unsupported`.
- **Platform bridge**: trait extended with five new calendar methods; macOS impl returns `"not_implemented"` for all of them.
- **TaktCore**: three new async methods (`get_calendar_access_status`, `request_calendar_access`, `list_calendars`) exposed via UniFFI, backed by the stub bridge.
- **Swift UI**: exhaustive switches over the regenerated `Schedule` / `Action` enums compile via fallback arms with `// TODO: Phase 4` markers. `ScheduleTypeTag` and `ActionTypeTag` are **unchanged** — no Calendar tab, no OpenEventLinks button, no save path can produce the new variants.
- **Save guards**: `TaskEditorViewModel.save()` refuses `Schedule::Calendar` and `Action::OpenEventLinks` with inline errors, even though the UI can't produce them.
- **Verification**: existing tests pass, new serde round-trip tests pass, manual smoke test confirms zero user-visible change.

The feature is fully scaffolded and genuinely off. Phase 2 fills in the EventKit adapter and populates `TaskDto.health` for real. Phase 3 lands the poller and executor. Phase 4 flips the UI on. Phase 5 polishes.
