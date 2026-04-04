# ScheduleBuilder Redesign — Reminders-Style

## Goal

Redesign the ScheduleBuilder component to replace the current cron-preset buttons with a frequency-based UI inspired by macOS Reminders. Keep the existing 3 schedule types (Recurring, OneShot, DailyFirstUse) and the `Schedule` Rust enum unchanged — the frontend generates cron expressions from the visual controls.

## Architecture

The ScheduleBuilder remains a controlled component (`value: Schedule`, `onChange`). Internally, the Recurring tab parses the incoming cron expression into a structured form (frequency + options) and converts back to cron on every change. No backend changes are needed — the `Schedule` enum stays the same.

## Schedule Types

### 1. Recurring (Cron)

**Frequency selector** — dropdown with options: Hourly, Daily, Weekly, Monthly, Custom.

Each frequency shows specific controls:

#### Hourly
- "Every `[X]`" + unit selector (Minutes / Hours)
- No time field (interval-based)
- Generates cron: `*/X * * * *` (minutes) or `0 */X * * *` (hours)

#### Daily
- "Every `[X]` days"
- Time field: `[HH]:[MM]`
- Generates cron: `MM HH */X * *` (for X>1) or `MM HH * * *` (for X=1)

#### Weekly
- "Every `[X]` weeks"
- Day-of-week grid: 7 buttons (Sun-Sat), multi-select, toggle on click
- Time field: `[HH]:[MM]`
- Generates cron: `MM HH * * 0,1,3` (selected days as comma-separated)
- Note: "Every X weeks" with X>1 cannot be expressed in standard cron. For X>1, the UI shows the field but the cron expression only captures the days+time (equivalent to weekly). This is acceptable — the common case is X=1.

#### Monthly
- "Every `[X]` months"
- Radio choice between two modes:
  - **Each**: grid of days 1-31, multi-select, toggle on click. Generates cron: `MM HH 4,15 */X *`
  - **On the**: two dropdowns — position (first, second, third, fourth, last) + weekday (Sunday-Saturday). Generates cron: complex expression using day-of-week field with `#` or `L` modifiers. Since standard cron doesn't support `#` natively, this will use croner-compatible syntax or fall back to listing specific days.
- Time field: `[HH]:[MM]`
- Note: "Every X months" with X>1 uses `*/X` in month field. For "On the" with positions, use cron weekday syntax where supported.

#### Custom
- Raw cron expression input field (monospace font)
- No additional controls — time is implicit in the expression
- Fallback for advanced users

### 2. One Time (OneShot)

Two separate styled fields:
- **Date**: `<input type="date">` styled to match the app theme
- **Time**: Two number inputs `[HH]:[MM]` styled consistently with the Recurring time fields

Combines into ISO 8601 string for `run_at`.

### 3. Daily First Use

No changes. Shows explanatory text: "Runs once per day, 5 minutes after you start using your Mac."

## UI Components

### Time Field
Reusable sub-component used by Daily, Weekly, Monthly, and OneShot:
- Two `<input type="number">` fields for hours (0-23) and minutes (0-59)
- Separated by `:` character
- Compact, inline layout

### Day Grid (Monthly)
- 7-column grid, rows for days 1-31
- Each day is a toggle button
- Selected state: primary color background with white text
- Unselected: muted background

### Weekday Grid (Weekly)
- Horizontal row of 7 buttons (Sun, Mon, Tue, Wed, Thu, Fri, Sat)
- Same toggle behavior as day grid
- Uses 3-letter abbreviations

### Interval Field
Reusable for "Every X [unit]":
- Number input for the value
- Unit label or dropdown (depending on frequency)

## Cron Parsing (Expression to UI)

When editing an existing task, the component must parse the cron expression back into the visual form:

- `*/5 * * * *` → Hourly, every 5 minutes
- `0 */2 * * *` → Hourly, every 2 hours
- `30 9 * * *` → Daily, every 1 day, 09:30
- `0 8 * * 1,3,5` → Weekly, every 1 week, Mon+Wed+Fri, 08:00
- `0 9 1,15 * *` → Monthly, each, days 1+15, 09:00
- Anything that doesn't match a known pattern → Custom, show raw expression

The parser should be best-effort. If it can't confidently determine the frequency, default to Custom with the raw expression shown.

## Data Flow

```
User interaction
  → Update local structured state (frequency, days, time, etc.)
  → Convert to cron expression
  → Call onChange({ type: 'Cron', expression: cronString })
```

```
Incoming value (edit mode)
  → Parse cron expression
  → Populate structured state (frequency, days, time, etc.)
  → Render appropriate controls
```

## Styling

- Follow existing shadcn/Tailwind patterns in the codebase
- Use existing UI components: Tabs, Button, Input, Select, Label
- Day/weekday grids use Button with variant toggle (default/outline)
- Frequency selector uses Select component
- Radio buttons for Each/On the use native radio or custom styled radio
- Consistent with the macOS-native feel (SF Pro font, rounded corners, muted colors)

## Files to Modify

- `src/components/ScheduleBuilder.tsx` — complete rewrite of the component
- Possibly extract sub-components if the file gets too large:
  - `src/components/schedule/TimeField.tsx`
  - `src/components/schedule/DayGrid.tsx`
  - `src/components/schedule/WeekdayGrid.tsx`
  - `src/components/schedule/CronParser.ts` — cron ↔ structured state conversion

## No Backend Changes

The `Schedule` enum remains:
```rust
pub enum Schedule {
    Cron { expression: String },
    OneShot { run_at: String },
    DailyFirstUse,
}
```

All complexity lives in the frontend conversion between visual controls and cron expressions.
