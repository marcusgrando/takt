# Calendar Trigger — Phase 2 Implementation Plan (EventKit Adapter)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Phase 1 stub implementations of the calendar `PlatformBridge` methods with real EventKit-backed logic, add the required `Info.plist` key, and start populating `TaskDto.health` based on live calendar state. The feature stays **off** from the user's perspective — no new UI, no new tabs — but the engine can now talk to the system calendar store.

**Architecture:** Add a new Swift file `macos/Takt/TaktCore+Calendar.swift` that extends `MacOSPlatformBridge` with real EventKit implementations. Add `INFOPLIST_KEY_NSCalendarsFullAccessUsageDescription` to the Xcode build settings. Rewrite `TaskStore::list_tasks` and `get_task` in Rust to populate `TaskDto.health` for Calendar-scheduled tasks using at most two bridge calls per `list_tasks()` call.

**Tech Stack:** Swift (`EventKit`), Xcode `INFOPLIST_KEY_*` build settings, Rust (`libtakt` → `store.rs`), UniFFI.

**Spec reference:** `docs/superpowers/specs/2026-04-10-calendar-trigger-design.md` §6.2, §6.3, §6.4, §10 "Step 2".

**Working directory:** `/Users/marcus.grando/git/cronmac`

**Prerequisite:** Phase 1 complete (`calendar-phase-1-complete` tag or equivalent).

**Rollout invariant for this phase:** the user still sees zero UI change. No Calendar tab, no OpenEventLinks button, no badges. Internally, the bridge now returns real data and `list_tasks` has the health-population code path exercised (even though no task has a Calendar schedule yet).

---

## Task 1: Add the `NSCalendarsFullAccessUsageDescription` build setting

**Files:**
- Modify: `macos/Takt.xcodeproj/project.pbxproj` (Debug and Release build settings for the `Takt` target)

Xcode generates `Info.plist` from `INFOPLIST_KEY_*` build settings. There is no `Info.plist` file in the tree — we add a build setting, not a file.

- [ ] **Step 1: Locate the existing INFOPLIST_KEY lines**

Run: `grep -n "INFOPLIST_KEY_NSHumanReadableCopyright" macos/Takt.xcodeproj/project.pbxproj`
Expected: two hits, one for Debug (~line 283) and one for Release (~line 327).

- [ ] **Step 2: Add the new key to Debug build settings**

Open `macos/Takt.xcodeproj/project.pbxproj`. Find the Debug configuration block (contains `INFOPLIST_KEY_NSHumanReadableCopyright = "";` around line 283). Immediately after that line, add:

```
				INFOPLIST_KEY_NSCalendarsFullAccessUsageDescription = "Takt reads your calendars to trigger automations based on upcoming events. Data stays on your device.";
```

(Match the existing indentation — tabs, not spaces. Check the surrounding lines.)

- [ ] **Step 3: Add the new key to Release build settings**

Find the Release configuration block (contains `INFOPLIST_KEY_NSHumanReadableCopyright = "";` around line 327). Add the same line immediately after it.

- [ ] **Step 4: Verify the project still parses**

Run: `xcodebuild -project macos/Takt.xcodeproj -list`
Expected: lists the `Takt` target without error. If Xcode complains about a malformed `.pbxproj`, revert and re-edit carefully — `.pbxproj` is tab-sensitive.

- [ ] **Step 5: Build to confirm**

Run: `make build`
Expected: succeeds. The generated `Info.plist` inside the `.app` bundle now contains the new key.

- [ ] **Step 6: Verify the key made it into the bundle**

Run: `plutil -p $(find ~/Library/Developer/Xcode/DerivedData -name 'Takt.app' -path '*Release*' 2>/dev/null | head -1)/Contents/Info.plist | grep -i calendar`
Expected: a line containing `NSCalendarsFullAccessUsageDescription` with the copy from Step 2. (If nothing is returned, the path resolution failed — find the built `.app` manually and re-run `plutil -p`.)

- [ ] **Step 7: Commit**

```bash
git add macos/Takt.xcodeproj/project.pbxproj
git commit -m "feat(calendar): add NSCalendarsFullAccessUsageDescription Info.plist key"
```

---

## Task 2: Create `TaktCore+Calendar.swift` with EventKit-backed bridge methods

**Files:**
- Create: `macos/Takt/TaktCore+Calendar.swift`
- Modify: `macos/Takt/TaktCore+Bridge.swift` (remove the Phase 1 stubs so only the real impls remain)
- Modify: `macos/Takt.xcodeproj/project.pbxproj` (add the new Swift file to the `Takt` target)

- [ ] **Step 1: Write the new file**

Create `macos/Takt/TaktCore+Calendar.swift` with this exact content:

```swift
import Foundation
import EventKit

// Extension that implements the calendar-related PlatformBridge methods.
// Separated from TaktCore+Bridge.swift so EventKit imports stay scoped.

private final class CalendarStoreHolder: @unchecked Sendable {
    static let shared = CalendarStoreHolder()
    let store = EKEventStore()
    private init() {}
}

private func mapAuthorizationStatus(_ raw: EKAuthorizationStatus) -> CalendarAccessStatus {
    switch raw {
    case .notDetermined:
        return .notDetermined
    case .fullAccess:
        return .authorized
    case .authorized:
        // Legacy pre-macOS-14 full access. Treat as authorized.
        return .authorized
    default:
        // .denied, .restricted, .writeOnly — none of these satisfy our read needs.
        return .denied
    }
}

private func isoString(from date: Date) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: date)
}

private func parseIsoDate(_ s: String) -> Date? {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let d = formatter.date(from: s) { return d }
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: s)
}

private func colorHex(from cgColor: CGColor?) -> String? {
    guard let cg = cgColor, let comps = cg.components, comps.count >= 3 else { return nil }
    let r = Int((comps[0] * 255).rounded())
    let g = Int((comps[1] * 255).rounded())
    let b = Int((comps[2] * 255).rounded())
    return String(format: "#%02X%02X%02X", r, g, b)
}

private let conferenceDomainPattern: NSRegularExpression = {
    let pattern = #"https?://(?:[\w.-]*\.)?(meet\.google\.com|zoom\.us|teams\.microsoft\.com|webex\.com)[^\s<>"']*"#
    return try! NSRegularExpression(pattern: pattern, options: [.caseInsensitive])
}()

private func detectConferenceUrl(notes: String?, location: String?) -> String? {
    for candidate in [notes, location] {
        guard let text = candidate else { continue }
        let range = NSRange(text.startIndex..., in: text)
        if let match = conferenceDomainPattern.firstMatch(in: text, range: range),
           let swiftRange = Range(match.range, in: text) {
            return String(text[swiftRange])
        }
    }
    return nil
}

private func mapEvent(_ ek: EKEvent, calendarId: String) -> CalendarEvent {
    let conferenceUrl: String?
    if #available(macOS 12.0, *), let direct = ek.conferenceURL?.absoluteString {
        conferenceUrl = direct
    } else {
        conferenceUrl = detectConferenceUrl(notes: ek.notes, location: ek.location)
    }
    return CalendarEvent(
        id: ek.eventIdentifier ?? UUID().uuidString,
        title: ek.title ?? "",
        start: isoString(from: ek.startDate),
        end: isoString(from: ek.endDate),
        notes: ek.notes,
        location: ek.location,
        url: ek.url?.absoluteString,
        conferenceUrl: conferenceUrl,
        calendarId: calendarId
    )
}

extension MacOSPlatformBridge {

    func getCalendarAccessStatus() throws -> CalendarAccessStatus {
        return mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
    }

    func requestCalendarAccess() throws -> CalendarAccessStatus {
        let semaphore = DispatchSemaphore(value: 0)
        let store = CalendarStoreHolder.shared.store
        if #available(macOS 14.0, *) {
            var outcome: Bool = false
            store.requestFullAccessToEvents { granted, _ in
                outcome = granted
                semaphore.signal()
            }
            semaphore.wait()
            _ = outcome // used implicitly via re-read below
        } else {
            var outcome: Bool = false
            store.requestAccess(to: .event) { granted, _ in
                outcome = granted
                semaphore.signal()
            }
            semaphore.wait()
            _ = outcome
        }
        return mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
    }

    func listCalendars() throws -> [CalendarInfo] {
        let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
        guard status == .authorized else {
            throw CalendarBridgeError.accessDenied
        }
        let store = CalendarStoreHolder.shared.store
        return store.calendars(for: .event).map { cal in
            CalendarInfo(
                id: cal.calendarIdentifier,
                title: cal.title,
                source: cal.source?.title ?? "Local",
                colorHex: colorHex(from: cal.cgColor)
            )
        }
    }

    func fetchEventsInWindow(
        calendarId: String,
        lookbackMinutes: UInt32,
        lookaheadMinutes: UInt32
    ) throws -> [CalendarEvent] {
        let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
        guard status == .authorized else {
            throw CalendarBridgeError.accessDenied
        }
        let store = CalendarStoreHolder.shared.store
        guard let cal = store.calendar(withIdentifier: calendarId) else {
            throw CalendarBridgeError.calendarNotFound(calendarId)
        }
        let now = Date()
        let start = now.addingTimeInterval(-Double(lookbackMinutes) * 60.0)
        let end = now.addingTimeInterval(Double(lookaheadMinutes) * 60.0)
        let predicate = store.predicateForEvents(withStart: start, end: end, calendars: [cal])
        let events = store.events(matching: predicate)
        return events.map { mapEvent($0, calendarId: calendarId) }
    }

    func fetchEventInstance(
        calendarId: String,
        eventId: String,
        eventStart: String
    ) throws -> CalendarEvent? {
        let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
        guard status == .authorized else {
            throw CalendarBridgeError.accessDenied
        }
        guard let anchor = parseIsoDate(eventStart) else {
            throw CalendarBridgeError.invalidEventStart
        }
        let store = CalendarStoreHolder.shared.store
        guard let cal = store.calendar(withIdentifier: calendarId) else {
            throw CalendarBridgeError.calendarNotFound(calendarId)
        }
        // ±6h window around the anchor, then filter by eventIdentifier.
        let windowSeconds: TimeInterval = 6 * 60 * 60
        let predicate = store.predicateForEvents(
            withStart: anchor.addingTimeInterval(-windowSeconds),
            end: anchor.addingTimeInterval(windowSeconds),
            calendars: [cal]
        )
        let matches = store.events(matching: predicate).filter { $0.eventIdentifier == eventId }
        guard !matches.isEmpty else { return nil }
        // Pick the one whose start is closest to the anchor (handles recurring series).
        let best = matches.min { a, b in
            abs(a.startDate.timeIntervalSince(anchor)) < abs(b.startDate.timeIntervalSince(anchor))
        }!
        return mapEvent(best, calendarId: calendarId)
    }
}

// MARK: - Error type
//
// The Rust bridge methods return Result<T, String>. UniFFI-generated Swift
// protocols expose them as throwing functions whose thrown error's
// localizedDescription becomes the Rust String. CalendarBridgeError packages
// the error messages Phase 1 agreed on (calendar_access_denied, etc).

enum CalendarBridgeError: LocalizedError {
    case accessDenied
    case calendarNotFound(String)
    case invalidEventStart

    var errorDescription: String? {
        switch self {
        case .accessDenied:
            return "calendar_access_denied"
        case .calendarNotFound(let id):
            return "calendar_not_found:\(id)"
        case .invalidEventStart:
            return "invalid_event_start"
        }
    }
}
```

- [ ] **Step 2: Remove the Phase 1 stubs from `TaktCore+Bridge.swift`**

Open `macos/Takt/TaktCore+Bridge.swift`. Delete the entire "MARK: - Calendar (Phase 1 stubs)" block — the five stub methods added in Phase 1 Task 7. The real implementations in `TaktCore+Calendar.swift` take over.

- [ ] **Step 3: Add the new file to the Xcode target**

Open `macos/Takt.xcodeproj/project.pbxproj`. Find the entry for `TaktCore+Bridge.swift` (`grep -n 'TaktCore+Bridge.swift' macos/Takt.xcodeproj/project.pbxproj`). You will see it appears in two places: a `PBXFileReference` section and a `PBXBuildFile` / `PBXSourcesBuildPhase` section. Add corresponding entries for `TaktCore+Calendar.swift` — easiest approach is to open the project in Xcode (`open macos/Takt.xcodeproj`), right-click the `Takt` group, "Add Files to Takt…", pick `TaktCore+Calendar.swift`, and let Xcode update the pbxproj. Save and close Xcode.

If you must edit the pbxproj directly, duplicate the two `TaktCore+Bridge.swift` entries and change the file name + generate new UUIDs for the entries. This is fragile — using Xcode is strongly recommended.

- [ ] **Step 4: Build the app**

Run: `make build`
Expected: succeeds. EventKit imports resolve, the `MacOSPlatformBridge` conformance is now satisfied by real methods instead of the Phase 1 stubs.

- [ ] **Step 5: Launch the app to confirm nothing broke**

Run: `open $(find ~/Library/Developer/Xcode/DerivedData -name 'Takt.app' -path '*Release*' 2>/dev/null | head -1)`
Expected: app launches normally, menu bar icon appears, existing tasks still function. The app does **not** prompt for calendar access (nothing calls `requestCalendarAccess` yet). No new UI.

- [ ] **Step 6: Commit**

```bash
git add macos/Takt/TaktCore+Calendar.swift \
        macos/Takt/TaktCore+Bridge.swift \
        macos/Takt.xcodeproj/project.pbxproj
git commit -m "feat(calendar): real EventKit-backed PlatformBridge methods"
```

---

## Task 3: Populate `TaskDto.health` in `TaskStore`

**Files:**
- Modify: `libtakt/src/store.rs` (`list_tasks` and `get_task`)
- Modify: `libtakt/src/models.rs` (add a `populate_health` helper if useful)

We want `list_tasks()` to call the bridge **at most twice**: once for `get_calendar_access_status()`, and once (conditionally) for `list_calendars()`. The result is cached across all tasks in a single call.

- [ ] **Step 1: Change `TaskStore` to hold a bridge reference**

Open `libtakt/src/store.rs`. The struct currently is:

```rust
pub struct TaskStore {
    pool: SqlitePool,
}
```

Replace with:

```rust
use std::sync::Arc;
use crate::platform::PlatformBridge;

pub struct TaskStore {
    pool: SqlitePool,
    bridge: Arc<dyn PlatformBridge>,
}
```

And update the constructor:

```rust
impl TaskStore {
    pub fn new(pool: SqlitePool, bridge: Arc<dyn PlatformBridge>) -> Self {
        Self { pool, bridge }
    }
    // ...
}
```

- [ ] **Step 2: Update the caller in `lib.rs`**

In `libtakt/src/lib.rs`, the `start()` method constructs `TaskStore::new(pool)`. Change it to:

```rust
let store = Arc::new(TaskStore::new(pool, Arc::clone(&bridge)));
```

- [ ] **Step 3: Run `cargo check`**

Run: `cargo check -p libtakt`
Expected: clean build. If the bridge is not yet in scope at the `TaskStore::new` call site, add an import or rearrange.

- [ ] **Step 4: Add the health-population helper in `store.rs`**

Still in `store.rs`, add this private function (after the `impl TaskStore` block or inside it — consistent with existing helpers):

```rust
use crate::models::{CalendarAccessStatus, Schedule, TaskHealth};
use std::collections::HashSet;

impl TaskStore {
    /// Populate `TaskDto.health` for every Calendar-scheduled task in the slice
    /// using at most two bridge calls (get_calendar_access_status + list_calendars).
    /// Non-Calendar tasks are left as `Healthy`.
    fn populate_health(&self, tasks: &mut [TaskDto]) {
        // Early return if no Calendar tasks — avoid any bridge calls.
        let has_calendar_task = tasks.iter().any(|t| matches!(t.schedule, Schedule::Calendar { .. }));
        if !has_calendar_task {
            return;
        }

        // Single access-status call shared by all Calendar tasks in this slice.
        let status = match self.bridge.get_calendar_access_status() {
            Ok(s) => s,
            Err(_) => {
                // If the bridge itself errors, surface Denied so the UI can react.
                CalendarAccessStatus::Denied
            }
        };

        match status {
            CalendarAccessStatus::NotDetermined => {
                for t in tasks.iter_mut() {
                    if matches!(t.schedule, Schedule::Calendar { .. }) {
                        t.health = TaskHealth::CalendarAccessNotDetermined;
                    }
                }
            }
            CalendarAccessStatus::Denied => {
                for t in tasks.iter_mut() {
                    if matches!(t.schedule, Schedule::Calendar { .. }) {
                        t.health = TaskHealth::CalendarAccessDenied;
                    }
                }
            }
            CalendarAccessStatus::Authorized => {
                // Second (and only second) bridge call: list_calendars.
                let known_ids: HashSet<String> = match self.bridge.list_calendars() {
                    Ok(list) => list.into_iter().map(|c| c.id).collect(),
                    Err(_) => HashSet::new(),
                };
                for t in tasks.iter_mut() {
                    if let Schedule::Calendar { calendar_id, .. } = &t.schedule {
                        if known_ids.contains(calendar_id) {
                            t.health = TaskHealth::Healthy;
                        } else {
                            t.health = TaskHealth::CalendarNotFound;
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 5: Call `populate_health` from `list_tasks` and `get_task`**

In `list_tasks`:

```rust
    pub async fn list_tasks(&self) -> anyhow::Result<Vec<TaskDto>> {
        let rows: Vec<Task> = sqlx::query_as("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        let mut dtos: Vec<TaskDto> = rows
            .iter()
            .map(|t| t.to_dto().map_err(|e| anyhow::anyhow!(e)))
            .collect::<Result<_, _>>()?;
        self.populate_health(&mut dtos);
        Ok(dtos)
    }
```

In `get_task`:

```rust
    pub async fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskDto>> {
        let row: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(t) = row {
            let mut dto = t.to_dto().map_err(|e| anyhow::anyhow!(e))?;
            // Treat as a slice of one for the shared code path.
            let mut slice = [dto];
            self.populate_health(&mut slice);
            Ok(Some(std::mem::replace(&mut slice[0], TaskDto {
                // unreachable placeholder — we return immediately
                id: String::new(),
                name: String::new(),
                description: None,
                enabled: false,
                run_if_missed: false,
                notify_on_run: false,
                schedule: Schedule::Cron { expression: String::new() },
                action: crate::models::Action::Settings { pane_url: String::new() },
                created_at: String::new(),
                updated_at: String::new(),
                last_run_at: None,
                next_run_at: None,
                health: TaskHealth::Healthy,
            })))
        } else {
            Ok(None)
        }
    }
```

The `mem::replace` dance is ugly because `TaskDto` doesn't implement `Default`. A cleaner alternative: wrap the single task in a `Vec`, call `populate_health`, then pop:

```rust
    pub async fn get_task(&self, id: &str) -> anyhow::Result<Option<TaskDto>> {
        let row: Option<Task> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(t) => {
                let mut v = vec![t.to_dto().map_err(|e| anyhow::anyhow!(e))?];
                self.populate_health(&mut v);
                Ok(Some(v.into_iter().next().unwrap()))
            }
            None => Ok(None),
        }
    }
```

Use the `Vec`-based version — it is clean and the allocation is trivial.

- [ ] **Step 6: Run `cargo check`**

Run: `cargo check -p libtakt`
Expected: clean build.

- [ ] **Step 7: Write a unit test for `populate_health`**

Add to `libtakt/src/store.rs` or create `libtakt/tests/store_health.rs`:

```rust
#[cfg(test)]
mod health_tests {
    use super::*;
    use crate::models::*;
    use crate::platform::PlatformBridge;
    use std::sync::{Arc, Mutex};

    struct MockBridge {
        status: CalendarAccessStatus,
        calendars: Vec<CalendarInfo>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl MockBridge {
        fn new(status: CalendarAccessStatus, calendars: Vec<CalendarInfo>) -> Self {
            Self { status, calendars, calls: Mutex::new(Vec::new()) }
        }
    }

    impl PlatformBridge for MockBridge {
        fn send_notification(&self, _: String, _: String, _: bool) {}
        fn run_on_main_sync(&self, _: u64) {}
        fn is_user_active(&self, _: u64) -> bool { true }
        fn get_calendar_access_status(&self) -> Result<CalendarAccessStatus, String> {
            self.calls.lock().unwrap().push("get_status");
            Ok(self.status.clone())
        }
        fn request_calendar_access(&self) -> Result<CalendarAccessStatus, String> {
            Ok(self.status.clone())
        }
        fn list_calendars(&self) -> Result<Vec<CalendarInfo>, String> {
            self.calls.lock().unwrap().push("list_calendars");
            Ok(self.calendars.clone())
        }
        fn fetch_events_in_window(&self, _: String, _: u32, _: u32) -> Result<Vec<CalendarEvent>, String> {
            Ok(vec![])
        }
        fn fetch_event_instance(&self, _: String, _: String, _: String) -> Result<Option<CalendarEvent>, String> {
            Ok(None)
        }
    }

    fn make_dto(schedule: Schedule) -> TaskDto {
        TaskDto {
            id: "t1".into(),
            name: "test".into(),
            description: None,
            enabled: true,
            run_if_missed: false,
            notify_on_run: false,
            schedule,
            action: Action::Settings { pane_url: "x".into() },
            created_at: "".into(),
            updated_at: "".into(),
            last_run_at: None,
            next_run_at: None,
            health: TaskHealth::Healthy,
        }
    }

    fn store_with(bridge: Arc<MockBridge>) -> TaskStore {
        // Use an in-memory sqlite pool — the tests only exercise populate_health,
        // not the query layer.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy(":memory:")
            .expect("in-memory pool");
        TaskStore::new(pool, bridge as Arc<dyn PlatformBridge>)
    }

    #[test]
    fn non_calendar_task_stays_healthy_and_no_bridge_calls() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::Authorized, vec![]));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Cron { expression: "0 * * * *".into() })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert!(bridge.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn calendar_task_not_determined() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::NotDetermined, vec![]));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarAccessNotDetermined);
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status"]);
    }

    #[test]
    fn calendar_task_denied() {
        let bridge = Arc::new(MockBridge::new(CalendarAccessStatus::Denied, vec![]));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarAccessDenied);
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status"]);
    }

    #[test]
    fn calendar_task_healthy_when_calendar_present() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo { id: "cal-1".into(), title: "Work".into(), source: "Google".into(), color_hex: None }],
        ));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status", "list_calendars"]);
    }

    #[test]
    fn calendar_task_not_found_when_calendar_missing() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo { id: "cal-other".into(), title: "Other".into(), source: "Local".into(), color_hex: None }],
        ));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![make_dto(Schedule::Calendar {
            calendar_id: "cal-1".into(),
            title_contains: None,
            minutes_before: 5,
        })];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::CalendarNotFound);
    }

    #[test]
    fn multiple_calendar_tasks_share_two_bridge_calls() {
        let bridge = Arc::new(MockBridge::new(
            CalendarAccessStatus::Authorized,
            vec![CalendarInfo { id: "cal-1".into(), title: "Work".into(), source: "Google".into(), color_hex: None }],
        ));
        let store = store_with(Arc::clone(&bridge));
        let mut tasks = vec![
            make_dto(Schedule::Calendar { calendar_id: "cal-1".into(), title_contains: None, minutes_before: 5 }),
            make_dto(Schedule::Calendar { calendar_id: "cal-1".into(), title_contains: None, minutes_before: 10 }),
            make_dto(Schedule::Calendar { calendar_id: "cal-missing".into(), title_contains: None, minutes_before: 0 }),
        ];
        store.populate_health(&mut tasks);
        assert_eq!(tasks[0].health, TaskHealth::Healthy);
        assert_eq!(tasks[1].health, TaskHealth::Healthy);
        assert_eq!(tasks[2].health, TaskHealth::CalendarNotFound);
        // At most two bridge calls total, regardless of task count.
        assert_eq!(bridge.calls.lock().unwrap().as_slice(), &["get_status", "list_calendars"]);
    }
}
```

- [ ] **Step 8: Run the new tests**

Run: `cargo test -p libtakt health_tests`
Expected: 6 tests pass.

- [ ] **Step 9: Run the full suite**

Run: `cargo test -p libtakt`
Expected: everything passes.

- [ ] **Step 10: Build the app and smoke-test**

Run: `make build-rust && make build`
Launch the app. Verify:
- Existing tasks still load and run.
- `list_tasks` is now calling the bridge on every refresh, but since there are no Calendar-scheduled tasks yet, the early-return path is taken and there are zero bridge calls in practice. You can verify with a log breakpoint if paranoid.

- [ ] **Step 11: Commit**

```bash
git add libtakt/src/store.rs libtakt/src/lib.rs
git commit -m "feat(calendar): populate TaskDto.health from platform bridge

Adds a shared populate_health helper called by list_tasks and
get_task. Uses at most two bridge calls per list_tasks() regardless
of task count, and zero calls when no Calendar-scheduled tasks exist."
```

---

## Task 4: Full Phase 2 verification and rollout commit

**Files:**
- None (verification-only task)

- [ ] **Step 1: Run the full Rust suite**

Run: `cargo test -p libtakt`
Expected: all tests pass including `health_tests` and `phase1_calendar_tests`.

- [ ] **Step 2: Run `cargo clippy`**

Run: `cargo clippy -p libtakt --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Build the Release app**

Run: `make build`
Expected: succeeds. No new warnings.

- [ ] **Step 4: Manual invariant smoke test**

Launch the built app. Walk the Phase-1 invariant checklist again:

- [ ] Menu bar icon appears normally.
- [ ] Existing tasks still present and running.
- [ ] Editor still shows 3 schedule tabs and 7 action tabs — no Calendar, no OpenEventLinks.
- [ ] App does **not** prompt for calendar access on launch.
- [ ] No new dialogs, badges, or banners.

- [ ] **Step 5: Verify the `Info.plist` key is bundled**

Run: `plutil -p $(find ~/Library/Developer/Xcode/DerivedData -name 'Takt.app' -path '*Release*' 2>/dev/null | head -1)/Contents/Info.plist | grep -i calendar`
Expected: prints the `NSCalendarsFullAccessUsageDescription` string.

- [ ] **Step 6: Phase 2 completion commit and tag**

```bash
git commit --allow-empty -m "chore(calendar): Phase 2 complete — EventKit adapter wired

Bridge methods now use real EventKit APIs. TaskDto.health is populated
from live calendar state. No user-visible change — feature still off."
git tag calendar-phase-2-complete
```

---

## Summary

At the end of Phase 2:

- `INFOPLIST_KEY_NSCalendarsFullAccessUsageDescription` is set in Debug and Release build settings.
- `MacOSPlatformBridge` has real `EKEventStore`-backed implementations of all five calendar methods.
- `TaskStore::list_tasks` and `get_task` populate `TaskDto.health` via the bridge, with at most two bridge calls per `list_tasks()` and zero calls when no Calendar tasks exist.
- Unit tests in `store.rs` exercise all five `TaskHealth` transitions via a `MockBridge`.
- User still sees zero change — no tabs, no badges, no prompts.

Phase 3 lands the `CalendarPoller`, executor signature change, `template.rs`, `url_extract.rs`, and the full fire-time dispatch routine.
