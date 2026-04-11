# Calendar Trigger — Phase 4 Implementation Plan (UI Flip-On)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Atomically expose the calendar feature to users. Add the Calendar schedule tab, the Open Event Links action button, the real `CalendarScheduleBuilder` view, the `OpenEventLinks` action form, task-list health badges, real `AutoName` strings, and remove every Phase-1 fallback arm and save guard. After this phase a user can create a fully functional calendar-triggered task end-to-end.

**Architecture:** Add `.calendar` to `ScheduleTypeTag` and `.openEventLinks` to `ActionTypeTag`. Replace the Phase-1 `// TODO: Phase 4` fallback branches with real tag mappings. Add new SwiftUI views `CalendarScheduleBuilder.swift` and extend `ActionBuilderView` with the new action form. Wire `TaskItemView` to render health badges from `task.health`. Update `AutoName` with real cases. Remove save-time guards from `TaskEditorViewModel`.

**Tech Stack:** SwiftUI, EventKit (via the PlatformBridge from Phase 2).

**Spec reference:** `docs/superpowers/specs/2026-04-10-calendar-trigger-design.md` §7.1–§7.7, §10 "Step 4".

**Working directory:** `/Users/marcus.grando/git/cronmac`

**Prerequisite:** Phase 3 complete (`calendar-phase-3-complete` tag).

**Rollout invariant for this phase:** after this phase the feature is **on**. Users will see new UI elements. There is no halfway state — everything lands in a single conceptual step, even though individual tasks in this plan are atomic commits.

---

## Task 1: Add `.calendar` case to `ScheduleTypeTag`

**Files:**
- Modify: `macos/Takt/Views/ScheduleBuilderView.swift`

- [ ] **Step 1: Locate `ScheduleTypeTag`**

Run: `grep -n "enum ScheduleTypeTag" macos/Takt/Views/ScheduleBuilderView.swift`
Expected: one hit showing the enum definition (likely at the bottom of the file).

- [ ] **Step 2: Add the new case**

Add `case calendar` to the enum and populate its `label`, `systemImage`, and `allCases` (if any) metadata. Example edit:

```swift
enum ScheduleTypeTag: String, CaseIterable, Identifiable {
    case cron
    case oneShot
    case dailyFirstUse
    case calendar

    var id: String { rawValue }

    var label: String {
        switch self {
        case .cron: return "Cron"
        case .oneShot: return "One-shot"
        case .dailyFirstUse: return "Daily first use"
        case .calendar: return "Calendar"
        }
    }

    var systemImage: String {
        switch self {
        case .cron: return "clock.arrow.circlepath"
        case .oneShot: return "calendar.badge.clock"
        case .dailyFirstUse: return "sunrise.fill"
        case .calendar: return "calendar"
        }
    }
}
```

Verify against the actual definition in the file — your tree may have different metadata names. Match what exists and add the `case calendar` branch to each.

- [ ] **Step 3: Remove the Phase 1 fallback arm in `scheduleType`**

Still in `ScheduleBuilderView.swift`, find the `scheduleType` computed property added in Phase 1:

```swift
    private var scheduleType: ScheduleTypeTag {
        switch schedule {
        case .cron: return .cron
        case .oneShot: return .oneShot
        case .dailyFirstUse: return .dailyFirstUse
        // TODO: Phase 4 — add `.calendar` to ScheduleTypeTag and return it here.
        case .calendar: return .cron
        }
    }
```

Replace the `.calendar` arm with the real mapping:

```swift
        case .calendar: return .calendar
```

And delete the `// TODO: Phase 4` comment.

- [ ] **Step 4: Handle `.calendar` in `handleTypeChange`**

Find `handleTypeChange(_ tag: ScheduleTypeTag)` (around line 299). The existing switch covers three cases. Add a `.calendar` arm that emits a default-empty Calendar schedule:

```swift
    private func handleTypeChange(_ tag: ScheduleTypeTag) {
        switch tag {
        case .cron:
            schedule = .cron(expression: buildCron(recurring))
        case .oneShot:
            let formatter = ISO8601DateFormatter()
            schedule = .oneShot(runAt: formatter.string(from: oneShotDate))
        case .dailyFirstUse:
            schedule = .dailyFirstUse(delayMinutes: 5)
        case .calendar:
            schedule = .calendar(calendarId: "", titleContains: nil, minutesBefore: 5)
        }
    }
```

(Verify the exact Swift enum case spelling — UniFFI may generate `calendar(calendarId:titleContains:minutesBefore:)` or similar. Read the regenerated Swift binding to confirm.)

- [ ] **Step 5: Build**

Run: `make build`
Expected: succeeds. Launching the app now shows a fourth schedule tab, but clicking it renders no sub-view (we haven't implemented `CalendarScheduleBuilder` yet — Task 3).

- [ ] **Step 6: Commit**

```bash
git add macos/Takt/Views/ScheduleBuilderView.swift
git commit -m "feat(calendar): add Calendar case to ScheduleTypeTag"
```

---

## Task 2: Add `.openEventLinks` case to `ActionTypeTag`

**Files:**
- Modify: `macos/Takt/Views/ActionBuilderView.swift`

- [ ] **Step 1: Locate `ActionTypeTag`**

Run: `grep -n "enum ActionTypeTag" macos/Takt/Views/ActionBuilderView.swift`
Expected: one hit.

- [ ] **Step 2: Add the case and its metadata**

Add `case openEventLinks` and extend the label/systemImage/template accessors:

```swift
enum ActionTypeTag: String, CaseIterable, Identifiable {
    case openUrl, openFile, openApp, runCommand, notify, webhook, settings, openEventLinks

    var id: String { rawValue }

    var label: String {
        switch self {
        case .openUrl: return "Open URL"
        case .openFile: return "Open File"
        case .openApp: return "Open App"
        case .runCommand: return "Run"
        case .notify: return "Notify"
        case .webhook: return "Webhook"
        case .settings: return "Settings"
        case .openEventLinks: return "Event Links"
        }
    }

    var systemImage: String {
        switch self {
        case .openUrl: return "link"
        case .openFile: return "doc"
        case .openApp: return "macwindow"
        case .runCommand: return "terminal"
        case .notify: return "bell"
        case .webhook: return "globe"
        case .settings: return "gearshape"
        case .openEventLinks: return "calendar.badge.clock"
        }
    }
}
```

Match the existing switches exactly and add the new case to each.

- [ ] **Step 3: Fix the action type selector layout**

The existing `ActionBuilderView` renders the type selector in two rows of 4 and 3:

```swift
VStack(spacing: 2) {
    HStack(spacing: 2) {
        ForEach(ActionTypeTag.allCases.prefix(4)) { tag in actionTypeButton(tag) }
    }
    HStack(spacing: 2) {
        ForEach(ActionTypeTag.allCases.suffix(3)) { tag in actionTypeButton(tag) }
    }
}
```

With 8 cases, change the second row to `.suffix(4)`:

```swift
        ForEach(ActionTypeTag.allCases.suffix(4)) { tag in actionTypeButton(tag) }
```

- [ ] **Step 4: Remove the Phase 1 fallback in `actionType`**

Find the `actionType` computed property. Replace the Phase 1 fallback `case .openEventLinks: return .openUrl` with the real mapping:

```swift
        case .openEventLinks: return .openEventLinks
```

And remove the `// TODO: Phase 4` comment.

- [ ] **Step 5: Extend `handleTypeChange`**

Find `handleTypeChange(_ tag: ActionTypeTag)` around line 661. Add a `.openEventLinks` arm:

```swift
    private func handleTypeChange(_ tag: ActionTypeTag) {
        action = tag.template.defaultAction
    }
```

If the current impl uses `tag.template.defaultAction` (which comes from the `ActionTemplate` enum at `TaktApp.swift:81`), you need to extend `ActionTemplate` too (Task 6). For Task 2, add a direct mapping here as a fallback so this step compiles:

```swift
    private func handleTypeChange(_ tag: ActionTypeTag) {
        switch tag {
        case .openEventLinks:
            action = .openEventLinks(openConference: true, openNotesLinks: true, browser: nil)
        default:
            action = tag.template.defaultAction
        }
    }
```

(Once Task 6 extends `ActionTemplate`, the `default` branch suffices and the explicit `.openEventLinks` case can be removed.)

- [ ] **Step 6: Build**

Run: `make build`
Expected: succeeds. Launching the app shows an eighth action button, but clicking it renders no fields (Task 4 adds the real form).

- [ ] **Step 7: Commit**

```bash
git add macos/Takt/Views/ActionBuilderView.swift
git commit -m "feat(calendar): add OpenEventLinks case to ActionTypeTag"
```

---

## Task 3: Implement `CalendarScheduleBuilder` view

**Files:**
- Create: `macos/Takt/Views/CalendarScheduleBuilder.swift`
- Modify: `macos/Takt/Views/ScheduleBuilderView.swift` (render the new builder when `.calendar` is selected)
- Modify: `macos/Takt.xcodeproj/project.pbxproj` (add the new file to the target)

- [ ] **Step 1: Create the file**

Create `macos/Takt/Views/CalendarScheduleBuilder.swift`:

```swift
import SwiftUI

struct CalendarScheduleBuilder: View {
    @Binding var schedule: Schedule
    var core: TaktCore

    @State private var accessStatus: CalendarAccessStatus = .notDetermined
    @State private var calendars: [CalendarInfo] = []
    @State private var loading = false
    @State private var loadError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            switch accessStatus {
            case .notDetermined:
                notDeterminedView
            case .denied:
                deniedView
            case .authorized:
                authorizedView
            }
        }
        .task {
            await refreshAccessStatus()
        }
    }

    private var notDeterminedView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Takt needs access to your calendars to trigger tasks from events.")
                .font(.system(size: 13))
            Button("Grant Calendar Access") {
                Task { await requestAccess() }
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private var deniedView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Calendar access is denied.")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.orange)
            Text("Grant access in System Settings → Privacy & Security → Calendars, then reopen this editor.")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            Button("Open System Settings") {
                if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars") {
                    NSWorkspace.shared.open(url)
                }
            }
        }
    }

    @ViewBuilder
    private var authorizedView: some View {
        if loading {
            ProgressView("Loading calendars…")
        } else if let err = loadError {
            Text(err).foregroundStyle(.red).font(.system(size: 12))
        } else if calendars.isEmpty {
            Text("No calendars found. Add a calendar in the system Calendar app, then reopen this editor.")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        } else {
            formFields
        }
    }

    @ViewBuilder
    private var formFields: some View {
        let (currentCalendarId, currentTitle, currentMinutes) = currentValues()

        HStack {
            Text("Calendar").font(.system(size: 13, weight: .medium))
            Spacer()
            Picker("", selection: Binding(
                get: { currentCalendarId },
                set: { newId in
                    updateSchedule(calendarId: newId, titleContains: currentTitle, minutesBefore: currentMinutes)
                }
            )) {
                Text("Select a calendar…").tag("")
                ForEach(calendars, id: \.id) { cal in
                    Text("\(cal.title) (\(cal.source))").tag(cal.id)
                }
            }
            .labelsHidden()
            .frame(width: 220)
        }

        HStack(alignment: .top) {
            Text("Title contains").font(.system(size: 13, weight: .medium))
            Spacer()
            TextField("any event", text: Binding(
                get: { currentTitle ?? "" },
                set: { newValue in
                    let trimmed = newValue.trimmingCharacters(in: .whitespaces)
                    updateSchedule(
                        calendarId: currentCalendarId,
                        titleContains: trimmed.isEmpty ? nil : trimmed,
                        minutesBefore: currentMinutes
                    )
                }
            ))
            .textFieldStyle(.roundedBorder)
            .frame(width: 220)
        }

        HStack {
            Text("Trigger").font(.system(size: 13, weight: .medium))
            Spacer()
            Stepper(
                "\(currentMinutes) minutes before",
                value: Binding(
                    get: { Int(currentMinutes) },
                    set: { newValue in
                        let clamped = max(0, min(120, newValue))
                        updateSchedule(
                            calendarId: currentCalendarId,
                            titleContains: currentTitle,
                            minutesBefore: UInt32(clamped)
                        )
                    }
                ),
                in: 0...120
            )
            .labelsHidden()
            Text("\(currentMinutes) min before")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }

    private func currentValues() -> (String, String?, UInt32) {
        if case .calendar(let calendarId, let titleContains, let minutesBefore) = schedule {
            return (calendarId, titleContains, minutesBefore)
        }
        return ("", nil, 5)
    }

    private func updateSchedule(calendarId: String, titleContains: String?, minutesBefore: UInt32) {
        schedule = .calendar(
            calendarId: calendarId,
            titleContains: titleContains,
            minutesBefore: minutesBefore
        )
    }

    private func refreshAccessStatus() async {
        do {
            let status = try await core.getCalendarAccessStatus()
            self.accessStatus = status
            if status == .authorized {
                await loadCalendars()
            }
        } catch {
            self.loadError = (error as NSError).localizedDescription
        }
    }

    private func requestAccess() async {
        do {
            let status = try await core.requestCalendarAccess()
            self.accessStatus = status
            if status == .authorized {
                await loadCalendars()
            }
        } catch {
            self.loadError = (error as NSError).localizedDescription
        }
    }

    private func loadCalendars() async {
        self.loading = true
        defer { self.loading = false }
        do {
            let list = try await core.listCalendars()
            self.calendars = list
            self.loadError = nil
        } catch {
            self.loadError = "Failed to load calendars: \((error as NSError).localizedDescription)"
        }
    }
}
```

**Verify UniFFI signatures**: the exact Swift spellings of `.calendar(calendarId:...)`, `CalendarAccessStatus.notDetermined`, `core.listCalendars()`, and the `CalendarInfo` field names (`colorHex` vs `color_hex`) come from the regenerated bindings. If the regenerated names differ, adjust the view to match — the conceptual structure stays the same.

- [ ] **Step 2: Wire the builder into `ScheduleBuilderView`**

Find where `ScheduleBuilderView` dispatches on `scheduleType` to render cron/oneShot/dailyFirstUse sub-views. Add a `.calendar` arm:

```swift
        case .calendar:
            CalendarScheduleBuilder(schedule: $schedule, core: core)
```

You may need to thread `core: TaktCore` through `ScheduleBuilderView` — check how the parent editor passes data. If `core` is not currently available inside `ScheduleBuilderView`, add it as an `@EnvironmentObject` or pass it via the initializer (propagate from `TaskEditorView`).

- [ ] **Step 3: Add the new file to the Xcode target**

Open `macos/Takt.xcodeproj` in Xcode. Right-click the `Views` group → "Add Files to Takt…" → pick `CalendarScheduleBuilder.swift`. Save and close Xcode.

- [ ] **Step 4: Build**

Run: `make build`
Expected: succeeds. Selecting the Calendar tab in the editor shows the "Grant Calendar Access" button on first use.

- [ ] **Step 5: Commit**

```bash
git add macos/Takt/Views/CalendarScheduleBuilder.swift \
        macos/Takt/Views/ScheduleBuilderView.swift \
        macos/Takt.xcodeproj/project.pbxproj
git commit -m "feat(calendar): CalendarScheduleBuilder view with permission and picker"
```

---

## Task 4: Add the `OpenEventLinks` action form to `ActionBuilderView`

**Files:**
- Modify: `macos/Takt/Views/ActionBuilderView.swift`

- [ ] **Step 1: Add the render branch**

Find the section in `ActionBuilderView` that dispatches on `actionType` to render type-specific fields (look for the pattern that renders the `OpenUrl` fields). Add a new branch for `.openEventLinks`:

```swift
} else if case .openEventLinks(let openConference, let openNotesLinks, let browser) = action {
    VStack(alignment: .leading, spacing: 12) {
        Toggle("Open video conference link", isOn: Binding(
            get: { openConference },
            set: { newValue in
                action = .openEventLinks(openConference: newValue, openNotesLinks: openNotesLinks, browser: browser)
            }
        ))
        Toggle("Open links from event notes", isOn: Binding(
            get: { openNotesLinks },
            set: { newValue in
                action = .openEventLinks(openConference: openConference, openNotesLinks: newValue, browser: browser)
            }
        ))
        Picker("Browser", selection: Binding(
            get: { browser ?? "__default__" },
            set: { newValue in
                let b: String? = newValue == "__default__" ? nil : newValue
                action = .openEventLinks(openConference: openConference, openNotesLinks: openNotesLinks, browser: b)
            }
        )) {
            Text("Default browser").tag("__default__")
            ForEach(browsers, id: \.self) { b in
                Text(b).tag(b)
            }
        }
        Text("Only fires with a Calendar schedule. Run Now will fail without event context.")
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .italic()
    }
```

Thread this into the existing view switch alongside the other actions.

- [ ] **Step 2: Build and smoke test**

Run: `make build`
Open the app, create a new task, select the "Event Links" action tab. Verify:
- Two toggles are visible and start `true`.
- Browser picker renders with "Default browser" selected.
- The italic note is visible.

- [ ] **Step 3: Commit**

```bash
git add macos/Takt/Views/ActionBuilderView.swift
git commit -m "feat(calendar): OpenEventLinks action form with toggles and browser picker"
```

---

## Task 5: Task list health badges + real AutoName

**Files:**
- Modify: `macos/Takt/Views/TaskItemView.swift` (add the badge)
- Modify: `macos/Takt/Helpers/AutoName.swift` (real cases for Calendar and OpenEventLinks)

- [ ] **Step 1: Render the badge in `TaskItemView`**

Find where `TaskItemView` renders the action label row (around line 134 in Phase 1 exploration). Immediately after the label, add a conditional badge based on `task.health`:

```swift
        if task.health != .healthy {
            HStack(spacing: 4) {
                Image(systemName: healthBadgeIcon(task.health))
                    .font(.system(size: 10))
                Text(healthBadgeText(task.health))
                    .font(.system(size: 10, weight: .medium))
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(healthBadgeColor(task.health).opacity(0.2))
            .foregroundStyle(healthBadgeColor(task.health))
            .clipShape(Capsule())
        }
```

Add these helper functions to the same file (private to `TaskItemView`):

```swift
    private func healthBadgeText(_ health: TaskHealth) -> String {
        switch health {
        case .healthy: return ""
        case .calendarNotFound: return "Calendar missing"
        case .calendarAccessDenied: return "Access denied"
        case .calendarAccessNotDetermined: return "Grant access"
        }
    }

    private func healthBadgeIcon(_ health: TaskHealth) -> String {
        switch health {
        case .healthy: return ""
        case .calendarNotFound: return "calendar.badge.exclamationmark"
        case .calendarAccessDenied: return "lock.fill"
        case .calendarAccessNotDetermined: return "hand.raised.fill"
        }
    }

    private func healthBadgeColor(_ health: TaskHealth) -> Color {
        switch health {
        case .healthy: return .primary
        case .calendarNotFound: return .red
        case .calendarAccessDenied: return .orange
        case .calendarAccessNotDetermined: return .yellow
        }
    }
```

- [ ] **Step 2: Replace the Phase 1 minimal `actionLabel` / `actionColor` entries**

Find the Phase 1 entries `case .openEventLinks: return "event"` and `case .openEventLinks: return .teal`. Both are already reasonable — leave them, or refine "event" to "links" if desired. No required change here.

- [ ] **Step 3: Real AutoName cases**

Open `macos/Takt/Helpers/AutoName.swift`. Replace the Phase 1 minimal cases for Calendar and OpenEventLinks with real formatted strings.

In `describeAction`:

```swift
        case .openEventLinks(let openConference, let openNotesLinks, _):
            switch (openConference, openNotesLinks) {
            case (true, true): return "Open meeting links"
            case (true, false): return "Open video call"
            case (false, true): return "Open notes links"
            case (false, false): return "Open (nothing selected)"
            }
```

In `describeSchedule`:

```swift
        case .calendar(let calendarId, let titleContains, let minutesBefore):
            _ = calendarId  // the title lookup would need a calendar list; for now we use the raw presence
            let suffix = minutesBefore == 0 ? "when event starts" : "\(minutesBefore) min before"
            if let needle = titleContains, !needle.isEmpty {
                return "\(suffix) \"\(needle)\""
            } else {
                return "Before any calendar event (\(suffix))"
            }
```

- [ ] **Step 4: Build and smoke-test**

Run: `make build`
Open the app. Create a task with `Schedule::Calendar` + `Action::OpenEventLinks`. Verify:
- The auto-generated name looks sensible (e.g., "Open meeting links — 5 min before").
- If you revoke calendar access in System Settings, the task displays the "Access denied" badge next time the list refreshes.

- [ ] **Step 5: Commit**

```bash
git add macos/Takt/Views/TaskItemView.swift macos/Takt/Helpers/AutoName.swift
git commit -m "feat(calendar): task list health badges and real AutoName strings"
```

---

## Task 6: Remove save guards, add real validation, wire editor

**Files:**
- Modify: `macos/Takt/ViewModels/TaskEditorViewModel.swift`

- [ ] **Step 1: Remove the Phase 1 save guards**

Find the block added in Phase 1 Task 10:

```swift
        if case .calendar = schedule {
            self.error = "Calendar schedules are not yet supported"
            return false
        }
        if case .openEventLinks = action {
            self.error = "Open Event Links action is not yet supported"
            return false
        }
```

Delete both if-statements.

- [ ] **Step 2: Add real validation**

In the same file, add validation for `Schedule::Calendar` and for the `OpenEventLinks` + non-Calendar combination. Insert after the existing cron/oneShot validation blocks:

```swift
        if case .calendar(let calendarId, _, _) = schedule {
            let trimmed = calendarId.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty {
                error = "Please select a calendar"
                return false
            }
        }

        if case .openEventLinks = action {
            if case .calendar = schedule {
                // OK — OpenEventLinks requires Calendar schedule
            } else {
                error = "Open Event Links requires a Calendar schedule"
                return false
            }
        }
```

- [ ] **Step 3: Build**

Run: `make build`
Expected: succeeds.

- [ ] **Step 4: Manual save-path tests**

Open the app. For each of these combinations, attempt to save and verify the behavior:

- [ ] Cron + OpenUrl → saves successfully (regression check).
- [ ] Calendar with empty calendar_id + any action → save refused with "Please select a calendar" in the inline red box.
- [ ] Calendar with a picked calendar + OpenEventLinks → saves successfully.
- [ ] Cron + OpenEventLinks → save refused with "Open Event Links requires a Calendar schedule".
- [ ] Calendar with picked calendar + Notify → saves successfully (OpenEventLinks is not mandatory).

- [ ] **Step 5: Commit**

```bash
git add macos/Takt/ViewModels/TaskEditorViewModel.swift
git commit -m "feat(calendar): remove Phase 1 save guards, add real schedule/action validation"
```

---

## Task 7: End-to-end smoke test

**Files:**
- None (verification-only task)

- [ ] **Step 1: Launch the app from a clean build**

Run: `make build && open $(find ~/Library/Developer/Xcode/DerivedData -name 'Takt.app' -path '*Release*' 2>/dev/null | head -1)`

- [ ] **Step 2: Create a real calendar task**

1. Open the editor.
2. Select the Calendar schedule tab. First use: click "Grant Calendar Access" → native EventKit prompt appears → click Allow.
3. Pick a real calendar from the dropdown.
4. Leave `title_contains` empty.
5. Set `minutes_before` to 1.
6. Select the "Event Links" action tab.
7. Save.

- [ ] **Step 3: Create a test event in your system calendar**

Using the macOS Calendar app, create a new event on the calendar you picked:
- Start time: 2 minutes from now.
- Add a video conference link (Zoom, Meet, or Teams) if your calendar supports it — otherwise add a fake one like `https://meet.google.com/test-abc-def` to the notes.
- Add a second URL to the notes: `https://example.com/agenda`.

- [ ] **Step 4: Wait for the trigger**

The Takt poller polls every 5 minutes. To shortcut the wait, restart the app (`Cmd+Q` on the menu bar, then reopen). On next poll tick (within 5 minutes), the poller reserves the slot. At `event_start - 1 minute`, the dispatch routine fires.

- [ ] **Step 5: Verify the browsers open**

At the trigger time, verify that:
- Your default browser opens the conference URL tab.
- A second tab opens the `example.com/agenda` URL.
- The Takt history view (if accessible) shows a `success` entry with "Triggered by event: {title} @ {start}" in the stdout.

- [ ] **Step 6: Clean up**

Delete the test event from Calendar. Delete the Takt task. Verify no stray `calendar_dispatches` rows remain:

```bash
sqlite3 ~/.local/share/takt/takt.db "SELECT * FROM calendar_dispatches;"
```

(For dev builds, use `takt-dev.db`.)

- [ ] **Step 7: Phase 4 completion commit and tag**

```bash
git commit --allow-empty -m "chore(calendar): Phase 4 complete — feature live end-to-end

Users can now create calendar-triggered tasks, grant permission,
pick a calendar, filter by title, set a minutes-before offset, and
pick the OpenEventLinks action. Health badges render for broken
tasks. The first end-to-end smoke test confirmed browsers open
with the conference and notes URLs as expected."
git tag calendar-phase-4-complete
```

---

## Summary

At the end of Phase 4:

- `ScheduleTypeTag` has `.calendar`; `ActionTypeTag` has `.openEventLinks`.
- Phase 1 fallback arms and save guards are fully removed.
- `CalendarScheduleBuilder.swift` handles all three access states (`NotDetermined`, `Denied`, `Authorized`) with the correct UX per spec §7.2.
- `ActionBuilderView` renders the OpenEventLinks form with toggles and browser picker.
- `TaskItemView` renders health badges for `CalendarNotFound`, `CalendarAccessDenied`, `CalendarAccessNotDetermined`.
- `AutoName` generates real strings for Calendar schedules and OpenEventLinks actions.
- `TaskEditorViewModel.save()` validates empty `calendar_id` and `OpenEventLinks`+non-Calendar combinations with blocking inline errors.
- An end-to-end smoke test with a real EventKit event has confirmed the feature works.

Phase 5 polishes: template variable hint lines, quick-start template, README section.
