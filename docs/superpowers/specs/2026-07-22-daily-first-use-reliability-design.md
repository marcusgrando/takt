# Daily First Use Reliability Design

**Date:** 2026-07-22
**Status:** Implementation complete; automated validation complete; manual smoke pending

## Problem

`DailyFirstUse` sometimes does not run or runs hours after the user starts using the Mac. Exact-time cron schedules continue to work.

The current daily rearm logic computes a wall-clock duration until the next midnight and passes that duration to one long `tokio::time::sleep`. On macOS, Tokio uses a monotonic uptime clock that does not advance while the computer is suspended. If six hours remained when the Mac entered sleep, the task waits for another six hours of awake time after resume before it starts monitoring user activity again.

The existing activity algorithm also does not match the intended product behavior. It accumulates repeated HID activity. The required behavior is a five-minute continuously eligible session that begins with one real keyboard or mouse event.

## Required Behavior

For each local calendar day:

1. The schedule starts in a waiting state.
2. The session must be the active console session, logged in, unlocked, and displayed on an active screen.
3. The first keyboard or mouse event while the session is eligible starts the configured delay.
4. Additional keyboard or mouse events are not required during the delay.
5. The session and screen must remain continuously eligible for the full delay.
6. Lock, logout, screen saver, display sleep, system suspend, or hibernation resets the delay.
7. After the delay completes, the task runs once for that local calendar day.
8. A new local calendar day rearms the schedule without requiring an application restart.
9. If the Mac crosses midnight while suspended, the schedule rearms promptly after resume rather than waiting for stale uptime.

The configured `delay_minutes` remains authoritative; five minutes is the current UI default, not a hard-coded scheduler value.

## Selected Approach

Use a Rust-owned state machine and obtain one user activity snapshot from the macOS platform bridge on each poll.

This keeps scheduling and business rules inside `libtakt`, consistent with the project architecture. Swift remains responsible only for native macOS observations.

Alternatives rejected:

- Two separate bridge calls for session state and HID state: creates avoidable duplicate native queries and can return inconsistent observations if state changes between calls.
- Swift-owned timer: moves business logic into the platform layer and splits scheduler state across Swift and Rust.
- One persisted rearm timestamp: adds persistence complexity without solving the activity-state requirements.

## Platform Snapshot

Add a UniFFI record returned by `PlatformBridge` with enough information for Rust to make the scheduling decision:

- `session_active`: true only when the user is in the active console session, the session is unlocked, no screen saver is active, and at least one online display is awake.
- `eligibility_generation`: a monotonically increasing value changed whenever eligibility is lost, preserving transient lock, saver, display-sleep, and suspend events for every scheduled task.
- `input_event_count`: a native aggregate of accepted keyboard/mouse CoreGraphics event counters sampled for the current snapshot.
- `eligibility_input_event_count`: the native counter captured independently for each notification source: workspace session, distributed screen lock, system sleep, display sleep, and screen saver. Each source carries its own loss epoch. When the desktop becomes eligible, Swift selects the boundary from the latest valid recovery callback by logical capture order, without comparing opaque counter values. This excludes recovery gestures while preserving later input during compound and cross-source recovery. If recovery is discovered only by periodic polling because notification delivery was missed, capture occurs at the observed eligibility transition and is best-effort.
- `last_input_at_unix_millis`: the native wall-clock timestamp of the most recent accepted input, used to distinguish pre-midnight input from input on the armed local date; unavailable timing is represented as `None` and fails closed.

Swift gathers all values in one bridge call. Observer registration and AppKit queries remain on main. Native callbacks use `queue: nil`: loss callbacks synchronously mark their source ineligible and increment aggregate generation when eligibility is lost, while recovery callbacks synchronously capture a source-local epoch token and CoreGraphics counter before dispatching refresh work asynchronously to main. One `NSLock`-protected state object owns source eligibility, generation, pending boundaries, epochs, logical recovery ordering, and snapshot reads. A later loss invalidates only its source's token; main refresh rejects tokens from an older source epoch and validates current tokens against native state. Workspace-session and distributed-lock sources reconcile the same native session condition, but reconciliation does not manufacture another notified counter boundary. Source-local ordering follows callback order at the lock; no ordering across independent notification centers is assumed. Input identity uses `CGEventSource.counterForEventType`, avoiding event taps, new permissions, cross-FFI elapsed-time inference, arbitrary floating-point tolerance, and `DispatchQueue.main.sync`. Accepted event types exclude release events so the release half of an unlock/wake gesture cannot become a separate post-eligibility input. Generated bindings are regenerated through `scripts/build-rust.sh`; files under `macos/Takt/Generated/` are never edited manually.

Rust owns the state machine. It starts counting when the current native counter differs from the eligibility baseline and the native last-input timestamp belongs to the armed local date. This detects input after eligibility or after midnight even when it occurs before the first Rust snapshot, while unchanged counters cannot become false input because of FFI timing variation.

## Scheduler State Machine

The per-task loop has two activity states:

### WaitingForInput

- No delay is running.
- If the platform snapshot is ineligible, remain waiting.
- When the native input counter differs from the eligibility counter baseline and the last input belongs to the armed local date, transition to `Counting`.
- On notified recovery paths, Swift captures one epoch-bound token per notification source before delayed main refresh. A valid source token is checked against current native state; stale tokens from older epochs cannot re-enable eligibility. Compound recovery uses the most recent surviving source boundary by logical capture order, never by numeric counter ordering. Therefore unlock/wake input remains excluded while later input before effective eligibility or the first Rust snapshot remains distinguishable. Polling-only recovery is fail-closed and best-effort because no callback boundary exists without an event tap or new permission.

### Counting

- Record the monotonic start instant when the qualifying input is observed.
- Continue polling the platform snapshot.
- Do not require further HID input.
- If the snapshot becomes ineligible or `eligibility_generation` changes, reset to `WaitingForInput`. Generation changes preserve losses that start and finish between polls and are observed independently by multiple tasks.
- If clock discontinuity occurs without a new eligibility generation, reset fail-closed using the current counter as baseline and discard the current observation. If the same snapshot also carries a new generation, use the native eligibility baseline so valid post-wake input is preserved.
- When monotonic eligible time reaches `delay_minutes`, re-read the task, verify it is enabled and has not run today, then execute through the existing `execute_and_log` path.

After execution, clear activity state and wait for the local date to change.

## Daily Rearm

Replace the single long sleep until midnight with a short, cancellation-aware polling loop based on the local civil date.

Each cycle:

1. Check the cancellation token.
2. Read the current local date.
3. Return when the date differs from the date on which the task last ran.
4. Otherwise sleep only for the short poll interval.
5. After the waiter observes a new date, take the first activity snapshot immediately rather than sleeping for a second poll.

A short monotonic sleep may retain a small remainder across suspend, but the next iteration immediately re-evaluates the wall-clock date. It cannot retain hours of stale uptime.

## Suspend Detection

Track both clocks around each poll:

- monotonic elapsed time from `std::time::Instant`;
- wall-clock elapsed time from UTC time.

If wall-clock elapsed time exceeds monotonic elapsed time by more than two seconds, the system was suspended or the wall clock changed significantly. Reset the activity state conservatively.

This replaces the current assumption that a large `Instant::elapsed()` value detects system sleep. On macOS, `Instant` is based on uptime and does not advance during suspend.

App Nap does not create the same wall-versus-monotonic gap while the system remains awake, so it must not be classified as suspend solely because one poll was delayed.

## Polling

Use one short constant polling interval for:

- cancellation responsiveness;
- session-state observation;
- input detection;
- local-date rearm.

Use a ten-second internal polling interval. This bounds input detection, cancellation, and daily rearm latency to ten seconds while keeping native polling modest. The interval is not user-configurable.

## Persistence and Execution

No database migration is required.

Existing fields remain authoritative:

- `last_run_at` prevents a second execution on the same local day;
- `enabled` is rechecked before execution;
- `schedule_json` continues to store `delay_minutes`;
- `next_run_at` remains unchanged for this fix.

Execution continues through the shared `execute_and_log` function. Cron, one-shot, interval, login, and calendar schedules are outside this change.

## Cancellation and Updates

The existing `CancellationToken` remains the cancellation mechanism.

Every wait and poll must select between:

- token cancellation;
- the next short timer tick.

Editing, disabling, or deleting a task must stop the old state machine promptly. Rescheduling creates a new loop with clean state.

## Error Handling

- Native snapshot failures should produce an ineligible snapshot or an existing scheduler error path, never an assumed active state.
- Invalid or unavailable HID timing must not start the delay.
- Backward wall-clock changes reset temporal state conservatively.
- Task absence, disabled state, or an execution already recorded for the day prevents execution through existing guards.

## Tests

Add deterministic Rust tests around the state machine and rearm helper. Use controllable wall and monotonic time seams rather than real five-minute or overnight waits.

Required regression cases:

1. A local date change across simulated system suspend exits the daily rearm wait after one short poll.
2. An eligible session without HID input does not start the delay.
3. The first qualifying HID input starts the delay.
4. No additional HID input is required after counting starts.
5. Continuous eligibility for the configured delay allows execution.
6. Lock, logout, screen saver, or inactive display resets counting.
7. A transient eligibility loss between polls resets every task through `eligibility_generation`.
8. Suspend detected through wall-versus-monotonic divergence resets counting.
9. Unlock or resume establishes a baseline; only a later input starts a new full delay.
10. Input after eligibility but before the first Rust snapshot starts counting; unlock/wake input does not.
11. Input after midnight but before the first poll starts counting; pre-midnight input does not.
12. A task runs at most once per local day.
13. A failed executor is attempted once, logged as failure, updates `last_run_at`, and is not retried on the same day.
14. Each activity poll obtains exactly one native snapshot.
15. Cancellation exits both waiting and counting states promptly.
16. A clock discontinuity plus new generation preserves valid post-wake input; discontinuity without generation change discards the current observation.
17. Compound system/display/session recovery uses the latest valid source boundary, including when native counters roll over.
18. A delayed workspace loss cannot erase a valid distributed-unlock boundary, and crossed workspace/distributed callback application still uses logical capture order.
19. A new loss invalidates an older recovery token from the same source epoch; stale main work cannot re-enable eligibility or choose the new baseline.
20. Polling an already eligible desktop does not move the eligibility baseline or absorb input before Rust's next snapshot.
21. A loss callback is visible synchronously to a concurrent snapshot and increments generation exactly once before any main refresh.
22. Existing scheduler and store tests continue to pass.

Swift eligibility and recovery behavior has deterministic standalone tests with an injected counter source, including independent workspace/distributed ordering, source-local stale recovery rejection, compound recovery, opaque counter rollover, duplicate loss handling, and concurrent loss visibility. Regenerated bindings plus Debug and Release macOS builds validate integration; the manual lifecycle smoke matrix remains pending and non-blocking.

## Acceptance Criteria

- A task that ran yesterday and whose Mac slept overnight begins waiting for new input shortly after resume, not hours later.
- On notified recovery paths, the first accepted keyboard or mouse input after the latest valid source recovery boundary starts the configured delay, including input before delayed main refresh or the first Rust snapshot. Independent workspace/distributed ordering, stale source epochs, compound recovery, and counter rollover cannot select an invalid earlier boundary. Polling-only recovery remains best-effort.
- The task runs after the full delay with the session continuously active, even without further input.
- Any lock, screen inactivity, logout, suspend, or hibernation during the delay resets it.
- The task never runs twice on the same local calendar day.
- Exact-time cron behavior remains unchanged.
- Focused and package Rust tests plus Clippy with `-D warnings` pass.
- The new scheduler module passes standalone `rustfmt --check`; repository-wide `make check` remains blocked only by the pre-existing rustfmt baseline reproduced from `HEAD`.
- Generated Swift bindings and Debug/Release macOS app builds succeed.
- Manual smoke validation remains pending and is not a blocking automated check.

## Rollback

The change is isolated to the `DailyFirstUse` loop and platform activity snapshot. Rollback restores the previous bridge method and scheduler loop without database changes or data migration.

## Non-Goals

- Changing the UI default delay.
- Adding configuration for polling frequency.
- Persisting partially elapsed activity windows across application restarts.
- Changing cron, calendar, interval, one-shot, or login scheduling semantics.
- Adding a new database representation for `next_run_at`.
