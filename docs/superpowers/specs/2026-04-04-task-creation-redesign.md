# Task Creation Flow Redesign

## Problem

The current task creation wizard is too verbose for a tray popover app:
- 3-step wizard (Details → Schedule → Action) forces linear navigation even for simple tasks
- Step 1 asks for a name before the user knows what the task does
- Schedule Builder presents 4 types + 6 cron presets + custom input simultaneously
- Action Builder has 6 action types with distinct forms — feels like an admin panel
- Contrast issues: `TabsList` with `bg-muted` background is nearly indistinguishable from `bg-background`, causing poor visual hierarchy

## Design

Two-tier UI: popover as a template launcher, full window for all configuration.

### Popover (280x400, existing window)

The popover remains the primary entry point. No forms live in the popover.

**Default state:** Task list (unchanged from current). Each item shows name, next execution, toggle on/off. Tapping a task opens the full window for editing.

**"+" state:** Replaces the list with a template grid (2 columns, 3 rows):

| Template    | Icon | Default Action                        | Default Schedule     |
|-------------|------|---------------------------------------|----------------------|
| Open URL    | Link | `OpenUrl { url: "", browser: None }`  | `Cron "0 9 * * *"`  |
| Open File   | File | `OpenFile { path: "" }`               | `Cron "0 9 * * *"`  |
| Run Command | Terminal | `RunCommand { command: "", args: [], shell: "Zsh" }` | `Cron "0 * * * *"` |
| Reminder    | Bell | `Notify { title: "", body: "", sound: true }` | `Cron "0 9 * * *"` |
| Webhook     | Globe | `Webhook { url: "", method: "GET", headers: {}, body: None }` | `Cron "0 * * * *"` |
| Custom      | Settings | `OpenUrl { url: "", browser: None }` | `Cron "0 9 * * *"` |

**Template click behavior:**
1. Creates a task in the backend with template defaults (enabled = true)
2. Opens the full window with the new task loaded for editing
3. Focus is set to the primary field of the action type (URL, path, command, title...)

**Back navigation:** A "Back" button or arrow returns from template grid to the task list without creating anything.

**Footer:** "History" button (unchanged).

### Full Window (~500x600, new native window)

A standard macOS decorated window (title bar with close/minimize/zoom). Opens centered on screen.

**Title bar area:** Window title shows "New Task" or "Edit Task". A primary "Create"/"Save" button sits in the top-right area of the content (always visible, no scroll needed).

**Layout — single-page form with visual sections:**

#### Name
- Text input, pre-filled with auto-generated name based on action + schedule
- Auto-name updates in real-time as the user fills in action/schedule fields
- Examples: "Open google.com — Daily 9am", "Run backup.sh — Every hour", "Reminder: standup — Weekdays 9:45am"
- Once the user manually edits the name, auto-generation stops (track via a `nameManuallyEdited` boolean)

#### Action Section
- Horizontal tabs for the 6 action types: URL, File, Cmd, Notify, Keys, Hook
- Pre-selected based on the template chosen
- Each type shows only its essential fields inline
- Advanced fields (webhook headers/body, command args) are in a collapsible disclosure section labeled "Advanced", collapsed by default

**Action fields by type:**

| Type       | Essential Fields          | Advanced (collapsed) |
|------------|--------------------------|----------------------|
| OpenUrl    | URL, Browser (optional)   | —                    |
| OpenFile   | File path                 | —                    |
| RunCommand | Shell (select), Command   | Arguments (textarea) |
| Notify     | Title, Body, Sound toggle | —                    |
| Shortcut   | Keys (comma-separated)    | —                    |
| Webhook    | Method + URL              | Headers, Body        |

#### Schedule Section
- Row 1: Type selector — `[Recurring] [One time] [Login] [Wake]`
- Row 2 (Recurring only): Preset buttons — `[Every hour] [Daily] [Weekly] [Custom]`
- Custom cron input only appears when "Custom" preset is selected
- One time: datetime-local input
- Login/Wake: descriptive text only (no extra fields)

#### Description Section
- Collapsible disclosure, collapsed by default
- Optional textarea

#### Delete
- "Delete" button in the bottom-left corner of the window (edit mode only)
- Requires confirmation before deleting

### Contrast Fix

The root cause is `TabsList` using `bg-muted` (`oklch(0.97)`) which is nearly identical to `bg-background` (`oklch(1.0)`).

**Fix:**
- `TabsList` background: use `bg-secondary` or a slightly darker shade to create visible separation from page background
- `TabsTrigger` active state: use `bg-background` (white) with `shadow-sm` — creates clear contrast against the darker list background
- Schedule preset buttons: active = `bg-primary text-primary-foreground`, inactive = `bg-background border-border`
- All interactive elements must have clear visual distinction between default, hover, and active states

### Technical Notes

**New Tauri window:** The full window is a second webview window created on demand via Tauri's `WebviewWindow::builder`. It communicates with the same backend commands. The popover triggers window creation by invoking a Tauri command or using the window API from the frontend.

**Shared components:** The Action and Schedule form components are reused between both contexts. The difference is only which fields are shown (the full window shows everything, including advanced sections).

**Task lifecycle change:** Tasks are now created with defaults when a template is clicked (before the user fills in details). This means:
- The "Create" button in the full window becomes "Save" (the task already exists)
- If the user closes the window without saving, the task should be deleted (cleanup on window close)
- Alternative: don't create until "Save" is clicked, but pass template defaults as URL params or state

**Recommended approach:** Don't create on template click. Instead, pass the template type to the full window as state. The window opens in "create" mode with defaults pre-filled. "Create" button saves to backend. This avoids orphaned tasks.

**Popover → Full Window communication:** The popover opens the full window via Tauri's JS window API (`new WebviewWindow(...)`) with the template type or task ID encoded in the URL query string (e.g., `/editor?template=OpenUrl` or `/editor?taskId=abc123`). The full window reads the query params on mount to determine create vs. edit mode and which defaults to apply. Both windows share the same Tauri backend state — no IPC between windows is needed beyond the URL params.

**Auto-name generation:** A pure frontend function that takes `Action` + `Schedule` and returns a string. Runs on every change to either. Stored as the actual name value — not a computed display.

## Out of Scope

- Natural language input parsing (explored during brainstorming, deferred for complexity)
- Changes to the backend API or data model
- Changes to the History view
- Dark mode adjustments (follow the same contrast principles)
