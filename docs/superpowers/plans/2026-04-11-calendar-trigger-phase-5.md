# Calendar Trigger — Phase 5 Implementation Plan (Polish)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Discoverability and documentation polish for the calendar trigger feature. Add template-variable hint lines below relevant text fields when the editor has a Calendar schedule, add the "Open meeting links" quick-start template to `TemplateGridView`, and document the feature in the README.

**Architecture:** Pure SwiftUI + Markdown work. No Rust changes, no new bridge methods. Reuses the existing `ActionTemplate` enum pattern.

**Tech Stack:** SwiftUI, Markdown.

**Spec reference:** `docs/superpowers/specs/2026-04-10-calendar-trigger-design.md` §7.4, §7.8, §10 "Step 5".

**Working directory:** `/Users/marcus.grando/git/cronmac`

**Prerequisite:** Phase 4 complete (`calendar-phase-4-complete` tag).

**Rollout invariant for this phase:** zero behavioral changes. The feature works identically to Phase 4 — this phase only adds discoverability text, a template button, and docs.

---

## Task 1: Template variable hint lines

**Files:**
- Modify: `macos/Takt/Views/ActionBuilderView.swift`

- [ ] **Step 1: Identify a helper to detect Calendar schedule**

In `ActionBuilderView`, the schedule is available via the parent editor (likely as a passed-in prop or environment object). Verify how the view accesses it. If it does not already see the schedule, thread a new prop:

```swift
struct ActionBuilderView: View {
    @Binding var action: Action
    var schedule: Schedule       // new: for template-var hint visibility
    var browsers: [String]
    var fileApps: [String]
    var onFilePathChanged: () -> Void
    // ...
}
```

Update the parent call site in `TaskEditorView` to pass `schedule: vm.schedule`.

- [ ] **Step 2: Add a helper computed property**

Inside `ActionBuilderView`, add:

```swift
    private var isCalendarSchedule: Bool {
        if case .calendar = schedule { return true }
        return false
    }

    private var templateVarHint: some View {
        Text("Use `{{event.title}}`, `{{event.start}}`, `{{event.conference_url}}`, etc.")
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .italic()
    }
```

- [ ] **Step 3: Render the hint below relevant fields**

For each of the following action types, locate the view block and render `templateVarHint` immediately under the user-input text fields when `isCalendarSchedule` is true:

- `OpenUrl`: under the URLs list.
- `Notify`: under the body field.
- `RunCommand`: under the args field.
- `Webhook`: under the body field.

Example insertion pattern:

```swift
if case .openUrl(let urls, _, _, _) = action {
    // ... existing URL list rendering ...
    if isCalendarSchedule {
        templateVarHint
    }
}
```

- [ ] **Step 4: Build and smoke-test**

Run: `make build`
Open the editor:

- [ ] Create a task with Cron + OpenUrl → **no** hint text visible.
- [ ] Create a task with Calendar + OpenUrl → hint text visible below the URL field.
- [ ] Switch between Cron and Calendar schedule with the same OpenUrl action → hint appears/disappears as expected.

- [ ] **Step 5: Commit**

```bash
git add macos/Takt/Views/ActionBuilderView.swift macos/Takt/Views/TaskEditorView.swift
git commit -m "feat(calendar): template variable hint lines for Calendar-scheduled actions"
```

---

## Task 2: "Open meeting links" quick-start template

**Files:**
- Modify: `macos/Takt/TaktApp.swift` (the `ActionTemplate` enum at line 81)
- Verify: `macos/Takt/Views/TemplateGridView.swift` (should pick up new cases automatically)

- [ ] **Step 1: Add a new case to `ActionTemplate`**

In `macos/Takt/TaktApp.swift`, find `enum ActionTemplate: String, Codable, Hashable, CaseIterable { ... }` (line 81). Add the new case:

```swift
enum ActionTemplate: String, Codable, Hashable, CaseIterable {
    case openUrl, openFile, openApp, runCommand, notify, webhook, settings, openMeetingLinks
    // ...
}
```

- [ ] **Step 2: Add `label` and `systemImage`**

Extend the `label` switch:

```swift
    var label: String {
        switch self {
        case .openUrl: return "Open URL"
        case .openFile: return "Open File"
        case .openApp: return "Open App"
        case .runCommand: return "Run Command"
        case .notify: return "Reminder"
        case .webhook: return "Webhook"
        case .settings: return "Settings"
        case .openMeetingLinks: return "Open Meeting Links"
        }
    }
```

And the `systemImage` switch:

```swift
    var systemImage: String {
        switch self {
        case .openUrl: return "link"
        case .openFile: return "doc"
        case .openApp: return "macwindow"
        case .runCommand: return "terminal"
        case .notify: return "bell"
        case .webhook: return "globe"
        case .settings: return "gearshape"
        case .openMeetingLinks: return "calendar.badge.clock"
        }
    }
```

- [ ] **Step 3: Add `defaultAction` and `defaultSchedule`**

Extend `defaultAction`:

```swift
        case .openMeetingLinks:
            return .openEventLinks(openConference: true, openNotesLinks: true, browser: nil)
```

Extend `defaultSchedule`:

```swift
        case .openMeetingLinks:
            return .calendar(calendarId: "", titleContains: nil, minutesBefore: 5)
```

(The empty `calendarId` is intentional per spec §7.8 — the `CalendarScheduleBuilder` already handles this correctly. Phase 4 Task 6's save validation blocks the save until the user picks a real calendar, giving the inline "Please select a calendar" error.)

- [ ] **Step 4: Update the `ActionBuilderView.handleTypeChange` mapping**

If Phase 4 Task 2 left an explicit `case .openEventLinks` arm in `handleTypeChange` as a workaround, you can now remove it — `tag.template.defaultAction` will return the right thing via the new `openMeetingLinks` case. Check whether the workaround is still present and clean it up if so.

Wait — there is no direct mapping from `ActionTypeTag.openEventLinks` to `ActionTemplate.openMeetingLinks` unless the `template` accessor on `ActionTypeTag` is extended. Check the existing pattern for how `ActionTypeTag` maps to `ActionTemplate` and add the new mapping if needed. If the accessor does not exist, leave the Phase 4 workaround in `handleTypeChange` as-is.

- [ ] **Step 5: Build and smoke-test**

Run: `make build`
Open the app. In `TemplateGridView`, verify:

- [ ] A new "Open Meeting Links" template button appears with the calendar icon.
- [ ] Clicking it opens the editor with `Schedule::Calendar` + `Action::OpenEventLinks` pre-selected.
- [ ] The calendar dropdown shows "Select a calendar…" placeholder; saving without picking one shows the inline "Please select a calendar" error.
- [ ] Picking a calendar and saving creates the task successfully.

- [ ] **Step 6: Commit**

```bash
git add macos/Takt/TaktApp.swift macos/Takt/Views/ActionBuilderView.swift
git commit -m "feat(calendar): add Open Meeting Links quick-start template"
```

---

## Task 3: README documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Locate the existing feature documentation**

Open `README.md`. Find where other schedule types and actions are documented (search for "Cron", "OneShot", or "actions"). Identify the appropriate section to insert the calendar docs.

- [ ] **Step 2: Add the Calendar triggers section**

Insert a new subsection after the existing schedule type documentation:

```markdown
### Calendar triggers

Takt can trigger tasks from your macOS system calendars (iCloud, Google Calendar
synced to macOS, Local, and anything else that shows up in the Calendar app).

To use a calendar trigger:

1. Open the task editor and select the **Calendar** schedule tab.
2. On first use, click **Grant Calendar Access** and approve the prompt. Takt
   asks for read-only access — it never creates, edits, or deletes events.
3. Pick a calendar from the dropdown.
4. Optionally enter a case-insensitive substring in **Title contains** to match
   only events whose title contains that word (e.g. `standup`).
5. Set **Trigger** to the number of minutes before the event start you want the
   task to fire (0–120 minutes).

Combine with the **Open Event Links** action to automatically open the video
conference link and any URLs found in the event notes when the trigger fires:

- **Open video conference link** detects Meet, Zoom, Teams, and Webex links in
  the event's conference URL field or notes.
- **Open links from event notes** scans the notes and location fields for
  http(s) URLs and opens each in order, deduplicated against the conference URL.

You can also pair a calendar trigger with any other action. The trigger exposes
event data to existing actions via template variables in string fields:

| Variable                   | Example value                              |
|----------------------------|--------------------------------------------|
| `{{event.title}}`          | `Team standup`                             |
| `{{event.start}}`          | `2026-04-12T09:00:00Z`                     |
| `{{event.end}}`            | `2026-04-12T09:30:00Z`                     |
| `{{event.notes}}`          | Whatever is in the event notes             |
| `{{event.location}}`       | Event location if set                      |
| `{{event.url}}`            | The event's own URL                        |
| `{{event.conference_url}}` | Meet/Zoom/Teams/Webex link if detected     |
| `{{event.calendar_id}}`    | EventKit calendar identifier               |

For example, a Notify action with body `Meeting "{{event.title}}" starts now`
produces a notification with the real event title substituted.

**Quick start**: the template grid includes an **Open Meeting Links** template
that pre-selects a Calendar schedule with the Open Event Links action, 5 minutes
before the event. Click it, pick your work calendar, and save.

**Health badges**: if a task references a calendar that was deleted from the
system, or if calendar access was later denied, Takt shows a badge in the task
list so you can fix or delete the task.
```

Adjust the wording to match the rest of the README's voice.

- [ ] **Step 3: Add a brief note to the features summary (if one exists)**

If the README has a bullet list of supported schedule types or actions at the top, add entries for Calendar and Open Event Links alongside the existing ones.

- [ ] **Step 4: Verify the Markdown renders cleanly**

Run: `pandoc README.md -o /tmp/readme.html 2>&1 || cat README.md | head -100`
(Or just eyeball the diff.) Expected: no syntax errors, tables render, the new section is well-placed.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: add Calendar triggers section to README"
```

---

## Task 4: Final Phase 5 verification

- [ ] **Step 1: Run the full Rust suite**

Run: `cargo test -p libtakt`
Expected: all tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p libtakt --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Build the Release app**

Run: `make build`
Expected: succeeds.

- [ ] **Step 4: Walk the full polish checklist**

- [ ] Template variable hint text appears under OpenUrl / Notify / RunCommand / Webhook fields when the schedule is Calendar, and disappears when switched to any other schedule.
- [ ] "Open Meeting Links" template button appears in `TemplateGridView` with the calendar icon.
- [ ] Clicking the template opens the editor with Calendar + OpenEventLinks pre-selected, calendar dropdown empty.
- [ ] README renders the new Calendar triggers section correctly.

- [ ] **Step 5: Phase 5 completion commit and tag**

```bash
git commit --allow-empty -m "chore(calendar): Phase 5 complete — feature polished and documented

Template variable hints, Open Meeting Links quick-start template,
and the README Calendar triggers section are all landed. Feature
is fully shipped."
git tag calendar-phase-5-complete
```

---

## Summary

At the end of Phase 5:

- Template variable hint lines are visible in the editor when a Calendar schedule is active, and only there.
- A new "Open Meeting Links" quick-start template appears in `TemplateGridView`, pre-selecting Calendar + OpenEventLinks with `minutes_before = 5`.
- The README has a Calendar triggers section documenting the schedule type, the action, template variables, and the quick-start template.
- No behavioral changes versus Phase 4 — only discoverability and docs.

The calendar trigger feature is now complete across all five rollout phases:

1. Phase 1 — scaffold (enums, schema, bridge contract, defensive arms, save guards).
2. Phase 2 — EventKit adapter wired, health population live.
3. Phase 3 — CalendarPoller, dispatch routine, template vars, restart reconstitute, tests.
4. Phase 4 — UI flip: users can create and run calendar-triggered tasks end-to-end.
5. Phase 5 — polish: hints, template, README.
