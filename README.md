# Takt

macOS menu bar task scheduler. Automate opening files, URLs, apps, running commands, sending notifications, webhooks, and keyboard shortcuts — all on a schedule.

Built with SwiftUI (macOS 14+) and a Rust core library (`libtakt`) via UniFFI.

## Calendar triggers

Takt can trigger tasks from your macOS system calendars (iCloud, Google Calendar synced to macOS, Local, and anything else that shows up in the Calendar app).

To use a calendar trigger:

1. Open the task editor and select the **Calendar** schedule tab.
2. On first use, click **Grant Calendar Access** and approve the prompt. macOS grants full calendar access, but Takt only reads events.
3. Pick a calendar from the dropdown.
4. Optionally enter a case-insensitive substring in **Title contains** to match only events whose title contains that word (e.g. `standup`).
5. Set **Trigger** to the number of minutes before the event start you want the task to fire (0–120 minutes).

Combine with the **Open Event Links** action to automatically open the video conference link and any URLs found in the event notes when the trigger fires:

- **Open video conference link** detects Meet, Zoom, Teams, and Webex links in the event's conference URL field or notes.
- **Open links from event notes** scans the notes and location fields for http(s) URLs and opens each in order, deduplicated against the conference URL.

You can also pair a calendar trigger with any other action. Open URL, Notify, and Webhook support event template variables in their text fields:

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

For example, a Notify action with body `Meeting "{{event.title}}" starts now` produces a notification with the real event title substituted.

Run Command receives the same fields as environment variables named `TAKT_EVENT_TITLE`, `TAKT_EVENT_START`, `TAKT_EVENT_END`, `TAKT_EVENT_NOTES`, `TAKT_EVENT_LOCATION`, `TAKT_EVENT_URL`, `TAKT_EVENT_CONFERENCE_URL`, and `TAKT_EVENT_CALENDAR_ID`. Missing values are empty strings.

Use `printf '%s\n' "$TAKT_EVENT_TITLE"` in shell commands or `os.environ["TAKT_EVENT_TITLE"]` in Python after importing `os`. In AppleScript, use `system attribute "TAKT_EVENT_TITLE"`. Keep shell variable expansions quoted and never evaluate their contents as code.

Calendar templates inside command source or raw shell arguments are rejected to prevent calendar content from executing code. Migrate existing Run Command tasks from `{{event.*}}` to the environment variables above. Python and AppleScript arguments support templates because those arguments are passed separately from the script.

Commands have a five-minute execution limit and a 1 MiB limit for each output stream. Webhooks have a 30-second deadline and a 1 MiB response limit. Exceeding a limit records a failure.

**Quick start**: the template grid includes an **Open Meeting Links** template that pre-selects a Calendar schedule with the Open Event Links action, 5 minutes before the event. Click it, pick your work calendar, and save.

**Health badges**: if a task references a calendar that was deleted from the system, or if calendar access was later denied, Takt shows a badge in the task list so you can fix or delete the task.

## Development

```bash
# Prerequisites: Rust toolchain, Xcode 16+
rustup target add aarch64-apple-darwin

# Run format, lint, Rust and Swift tests
make check

# Build the macOS app without signing, as CI does
make build-ci

# Build & run
open macos/Takt.xcodeproj   # Cmd+R in Xcode
```

## Build

```bash
xcodebuild -project macos/Takt.xcodeproj -scheme Takt -configuration Release build
```

## License

Private — all rights reserved.
