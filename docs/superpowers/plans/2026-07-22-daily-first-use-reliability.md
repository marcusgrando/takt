# Daily First Use Reliability Implementation Plan

**Status:** Implementation complete; automated validation complete; manual smoke pending

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Make `DailyFirstUse` rearm correctly across macOS sleep and execute after one post-unlock keyboard/mouse event followed by the configured period of continuously eligible screen/session time.

**Architecture:** Keep all scheduling rules in Rust. Use one UniFFI activity snapshot with current eligibility, a non-consumable eligibility generation, native CoreGraphics input counters, an eligibility counter baseline, and a native last-input wall timestamp. Move the Daily First Use tracker into a focused Rust module, replace the long midnight sleep with short civil-date polling, and keep native macOS observations in a thread-safe Swift monitor.

**Implemented deviation from the original plan:** Final review rejected elapsed-idle inference across FFI. The completed contract uses native event counters for input identity and a native wall timestamp for the civil-day boundary, with no arbitrary timing tolerance.

**Post-review corrections:** Rust gives a simultaneous eligibility-generation change precedence over generic clock-discontinuity fail-closed handling so post-wake input is not discarded. Swift uses one lock-protected eligibility state object. Workspace session, distributed screen lock, system sleep, display sleep, and screen saver retain independent loss epochs and recovery tokens. Loss callbacks synchronously fail closed; recovery counters are captured before async main refresh; stale source tokens are rejected after newer losses; valid tokens are checked against native state; the final transition selects the latest valid recovery by logical capture order rather than counter value. Polling-only recovery remains explicitly best-effort.

**Tech Stack:** Rust, Tokio, Chrono, UniFFI, Swift, AppKit, CoreGraphics, SQLite/sqlx.

## Global Constraints

- Poll interval: 10 seconds.
- Clock discontinuity tolerance: 2 seconds.
- Login/unlock/wake input never starts the timer; the next input after eligibility does.
- Lock, logout, screen saver, display sleep, suspend, hibernation, date change, or eligibility-generation change resets the timer.
- Additional HID input is not required after counting starts.
- Execute at most once per local calendar day.
- No database migration and no changes to cron, calendar, interval, one-shot, or login semantics.
- Keep `execute_and_log`, `CancellationToken`, `last_run_at`, and final enabled/not-run-today guards.
- Never edit `macos/Takt/Generated/` manually; regenerate through `make build-rust`.
- Do not create commits unless the user explicitly requests them.

---

## File Structure

- Create `libtakt/src/scheduler/daily_first_use.rs`: pure tracker, clock continuity checks, short date waiter, async Daily First Use runner, and focused unit tests.
- Modify `libtakt/src/scheduler.rs`: declare the new module, route `Schedule::DailyFirstUse` to it, and expose existing execution helpers to the child module.
- Modify `libtakt/src/models.rs`: add the UniFFI `UserActivitySnapshot` record.
- Modify `libtakt/src/platform.rs`: replace `is_user_active` with one snapshot method.
- Modify Rust bridge mocks in `libtakt/src/tests.rs`, `libtakt/src/store.rs`, and `libtakt/tests/calendar_poller.rs`.
- Modify `macos/Takt/TaktCore+Bridge.swift`: retain the activity monitor and implement the generated snapshot method.
- Create `macos/Takt/TaktCore+Activity.swift`: native observers, main-thread native queries, and explicit keyboard/mouse event sampling.
- Create `macos/Takt/UserActivityEligibilityState.swift`: unified lock-protected source eligibility, generation, source-local epochs/tokens, recovery boundaries, and atomic snapshot state.
- Create `macos/TaktTests/UserActivityEligibilityStateTests.swift`: deterministic cross-source ordering, stale-token, compound-recovery, and concurrent-loss seam.
- Modify `docs/superpowers/specs/2026-07-22-daily-first-use-reliability-design.md` when review discoveries require behavior-level corrections.

---

### Task 1: Introduce the Activity Snapshot Contract

**Files:**
- Modify: `libtakt/src/models.rs`
- Modify: `libtakt/src/platform.rs`
- Modify: `libtakt/src/tests.rs`
- Modify: `libtakt/src/store.rs`
- Modify: `libtakt/tests/calendar_poller.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, uniffi::Record)]
pub struct UserActivitySnapshot {
    pub session_active: bool,
    pub eligibility_generation: u64,
    pub input_event_count: u64,
    pub eligibility_input_event_count: u64,
    pub last_input_at_unix_millis: Option<i64>,
}
```

```rust
fn get_user_activity_snapshot(&self) -> crate::models::UserActivitySnapshot;
```

- [x] **Step 1: Add a failing record test**

Add a focused test in `libtakt/src/models.rs`:

```rust
#[test]
fn user_activity_snapshot_preserves_activity_fields() {
    let snapshot = UserActivitySnapshot {
        session_active: true,
        eligibility_generation: 7,
        input_event_count: 11,
        eligibility_input_event_count: 9,
        last_input_at_unix_millis: Some(1_753_200_001_500),
    };

    assert!(snapshot.session_active);
    assert_eq!(snapshot.eligibility_generation, 7);
    assert_eq!(snapshot.input_event_count, 11);
    assert_eq!(snapshot.eligibility_input_event_count, 9);
    assert_eq!(snapshot.last_input_at_unix_millis, Some(1_753_200_001_500));
}
```

- [x] **Step 2: Verify RED**

Run:

```bash
cargo test --package libtakt user_activity_snapshot_preserves_activity_fields
```

Expected: compilation fails because `UserActivitySnapshot` does not exist.

- [x] **Step 3: Add the record and replace the bridge method**

Add the record near other UniFFI records in `models.rs`. Replace:

```rust
fn is_user_active(&self, idle_threshold_secs: u64) -> bool;
```

with:

```rust
fn get_user_activity_snapshot(&self) -> crate::models::UserActivitySnapshot;
```

Document that missing native data must fail closed.

- [x] **Step 4: Update all Rust mocks**

Use an inert eligible snapshot where existing tests do not exercise Daily First Use:

```rust
UserActivitySnapshot {
    session_active: true,
    eligibility_generation: 0,
    input_event_count: 0,
    eligibility_input_event_count: 0,
    last_input_at_unix_millis: None,
}
```

Update every `PlatformBridge` implementation in the three listed test locations.

- [x] **Step 5: Verify GREEN and trait completeness**

Run:

```bash
cargo test --package libtakt user_activity_snapshot_preserves_activity_fields
cargo test --package libtakt --no-run
rg -n "is_user_active|isUserActive" libtakt
```

Expected: tests compile and pass; search finds no old Rust method.

---

### Task 2: Build the Pure Daily First Use Tracker

**Files:**
- Create: `libtakt/src/scheduler/daily_first_use.rs`
- Modify: `libtakt/src/scheduler.rs`

**Interfaces:**
- Consumes: `UserActivitySnapshot` from Task 1.
- Produces private tracker types:

```rust
const POLL_INTERVAL: Duration = Duration::from_secs(10);
const CLOCK_DISCONTINUITY_TOLERANCE: Duration = Duration::from_secs(2);

enum Phase {
    WaitingForEligible,
    WaitingForInput {
        baseline_input_event_count: u64,
    },
    Counting {
        started_at: Instant,
    },
}

struct DailyFirstUseTracker {
    phase: Phase,
    armed_date: NaiveDate,
    eligibility_generation: Option<u64>,
}

enum TrackerOutcome {
    Continue,
    Ready,
}
```

The tracker transition receives current snapshot, local date, monotonic time, configured delay, and clock-discontinuity status.

- [x] **Step 1: Write failing baseline/input tests**

Add tests in the new module:

```rust
#[test]
fn first_eligible_snapshot_only_establishes_baseline()
```

```rust
#[test]
fn unlock_input_does_not_start_counting()
```

```rust
#[test]
fn input_after_eligibility_baseline_starts_counting()
```

The new-input rule compares native CoreGraphics event counters. On notified recovery paths Swift captures `eligibility_input_event_count` synchronously in the native callback and carries it until the desktop becomes eligible. Rust accepts input when `input_event_count` differs and `last_input_at_unix_millis` belongs to the armed local date. Missing timestamps fail closed. Polling-only recovery captures at the observed transition and is best-effort. No elapsed-age reconstruction or floating-point tolerance is used.

- [x] **Step 2: Verify RED**

Run:

```bash
cargo test --package libtakt daily_first_use::tests::first_eligible_snapshot_only_establishes_baseline
```

Expected: compilation fails because tracker types do not exist.

- [x] **Step 3: Implement only `WaitingForEligible` and `WaitingForInput`**

Rules:

```text
ineligible snapshot -> WaitingForEligible
eligible snapshot -> compare native current and eligibility-baseline counters
qualifying post-eligibility, same-day input -> Counting starting at observation time
```

The input used to make the desktop eligible is captured in Swift's baseline and never triggers. A later input may qualify even before Rust's first snapshot.

- [x] **Step 4: Add and pass counting tests**

Add:

```rust
#[test]
fn counting_does_not_require_additional_input()
```

```rust
#[test]
fn counting_requires_the_full_configured_delay()
```

```rust
#[test]
fn zero_delay_still_requires_a_qualifying_input()
```

Input counters and timestamps are ignored after entering `Counting`; only continuous eligibility, generation, date, clock continuity, and configured delay remain relevant.

- [x] **Step 5: Add and pass reset tests**

Add:

```rust
#[test]
fn current_ineligibility_resets_counting()
```

```rust
#[test]
fn eligibility_generation_change_resets_counting_even_when_currently_active()
```

```rust
#[test]
fn resume_requires_new_baseline_input_and_full_delay()
```

```rust
#[test]
fn local_date_change_resets_unfinished_counting()
```

Generation is non-consumable: each tracker stores its own observed value, so multiple schedules all detect the same transient interruption.

- [x] **Step 6: Verify tracker suite**

Run:

```bash
cargo test --package libtakt daily_first_use::tests
```

Expected: all pure tracker tests pass without real sleeps, database, executor, or Swift.

---

### Task 3: Detect Suspend and Rearm by Civil Date

**Files:**
- Modify: `libtakt/src/scheduler/daily_first_use.rs`

**Interfaces:**

```rust
struct ClockSample {
    wall: DateTime<Utc>,
    monotonic: Instant,
}

fn clock_discontinuity(previous: ClockSample, current: ClockSample) -> bool;
```

```rust
async fn wait_for_date_change_or_cancel<N, S, SF>(
    cancel: &CancellationToken,
    current_date: NaiveDate,
    local_date: N,
    sleep_one_poll: S,
) -> DateWaitOutcome
where
    N: FnMut() -> NaiveDate,
    S: FnMut() -> SF,
    SF: Future<Output = ()>;
```

- [x] **Step 1: Write failing clock tests**

Add:

```rust
#[test]
fn wall_monotonic_gap_detects_suspend()
```

```rust
#[test]
fn equally_delayed_clocks_do_not_treat_app_nap_as_suspend()
```

```rust
#[test]
fn backward_wall_clock_change_breaks_continuity()
```

Use fixed samples. A difference greater than two seconds or negative wall elapsed is discontinuous.

- [x] **Step 2: Implement clock continuity and connect it to tracker reset**

Do not infer sleep from a large `Instant::elapsed()` alone. App Nap advances both clocks and remains continuous. If a discontinuous snapshot also carries a new eligibility generation, use the eligibility baseline and preserve qualifying post-wake input. Without generation change, reset fail-closed using the current counter and discard that observation. Focused tests cover both branches.

- [x] **Step 3: Write failing date waiter tests**

Add async tests with injected date and sleeper:

```rust
#[tokio::test]
async fn date_wait_exits_after_one_short_poll_when_date_changes()
```

```rust
#[tokio::test]
async fn date_wait_returns_promptly_when_cancelled()
```

Use `std::future::ready(())` and `std::future::pending()`; no Tokio paused-time feature is required.

- [x] **Step 4: Implement the short waiter**

Production wrapper:

```rust
async fn wait_until_next_local_day_or_cancel(
    cancel: &CancellationToken,
    current_date: NaiveDate,
) -> DateWaitOutcome {
    wait_for_date_change_or_cancel(
        cancel,
        current_date,
        || Local::now().date_naive(),
        || tokio::time::sleep(POLL_INTERVAL),
    )
    .await
}
```

Delete the long sleep-until-midnight calculation.

- [x] **Step 5: Verify focused tests**

Run:

```bash
cargo test --package libtakt daily_first_use::tests
```

Expected: date changes are detected after at most one short poll; cancellation returns without waiting for the poll.

---

### Task 4: Integrate the Tracker with the Scheduler Runner

**Files:**
- Modify: `libtakt/src/scheduler/daily_first_use.rs`
- Modify: `libtakt/src/scheduler.rs`

**Interfaces:**
- Reuses: `execute_and_log`, `ran_today`, `TaskStore::get_task`, existing `CancellationToken`, and existing final execution guards.
- Produces a private `daily_first_use::run(...)` called from the existing `Schedule::DailyFirstUse` arm.

- [x] **Step 1: Add focused tests for local-date guards**

Extract a pure helper:

```rust
fn last_run_is_on_local_date(last_run_at: Option<&str>, date: NaiveDate) -> bool;
```

Add fixed RFC3339 tests for today versus yesterday.

- [x] **Step 2: Route the schedule arm to the new runner**

Convert `delay_minutes` safely:

```rust
let required_delay = Duration::from_secs(delay_minutes.saturating_mul(60));
```

Preserve token registration and cancellation behavior.

- [x] **Step 3: Implement the cancel-aware poll loop**

Each poll must:

1. Select cancellation or ten-second sleep.
2. Capture wall, monotonic, and local-date values.
3. Obtain one `get_user_activity_snapshot()`.
4. Reset for date/generation/current-ineligibility/clock discontinuity.
5. Advance tracker.
6. On readiness, re-fetch task and guard existence, enabled state, current date, and `last_run_at`.
7. Call existing `execute_and_log` once.
8. Wait for local date change with the short waiter.
9. After date change, take the next activity snapshot immediately without a second poll sleep.

Do not change action execution or persistence semantics. Runner composition tests assert one snapshot per poll and prove failed execution logs failure, updates `last_run_at`, and is not retried on the same day.

- [x] **Step 4: Remove the old algorithm**

Delete old HID constants, accumulated-seconds logic, incorrect `Instant` sleep detection, and `wait_until_tomorrow_or_cancel` long sleep.

- [x] **Step 5: Verify Rust behavior and regressions**

Run:

```bash
cargo test --package libtakt daily_first_use::tests -- --nocapture
make test
```

Expected: new tests and all existing Rust tests pass; no test waits five minutes or overnight.

---

### Task 5: Implement the Native macOS Activity Monitor

**Files:**
- Create: `macos/Takt/TaktCore+Activity.swift`
- Create: `macos/Takt/UserActivityEligibilityState.swift`
- Create: `macos/TaktTests/UserActivityEligibilityStateTests.swift`
- Modify: `macos/Takt/TaktCore+Bridge.swift`

**Interfaces:**
- Consumes generated `UserActivitySnapshot` and `PlatformBridge.getUserActivitySnapshot()`.
- Produces one synchronous, thread-safe snapshot callable from Tokio threads.

- [x] **Step 1: Add a focused monitor type**

Use a retained monitor created on the main actor before `core.start()`:

```swift
private let activityMonitor: UserActivityMonitor

init() {
    dispatchPrecondition(condition: .onQueue(.main))
    activityMonitor = UserActivityMonitor()
}
```

`UserActivityMonitor` owns observer tokens, uses `[weak self]`, and removes observers in `deinit`. It delegates all mutable eligibility/recovery data to `UserActivityEligibilityState`, which owns one `NSLock`; this avoids split ownership and lock inversion.

- [x] **Step 2: Track eligibility and generation fail-closed**

Track workspace session, distributed screen lock, system sleep, display sleep, and screen saver separately under one lock. Each source owns eligibility, a loss epoch, and an optional recovery token/boundary. A loss callback synchronously changes only its source before any main dispatch. Increment `generation` only on the aggregate eligible-to-ineligible transition, so concurrent snapshots fail closed immediately and duplicate/compound losses do not increment more than once. Never clear or consume generation when read. Missing session/display information yields aggregate `sessionActive = false`.

Observe validated macOS 14+ notifications for:

- active/resigned workspace session;
- lock/unlock;
- screen saver start/stop;
- display sleep/wake and screen-parameter changes;
- system sleep/wake.

Observer registration and AppKit queries remain on main thread. Notification callbacks use `queue: nil` to capture thread-safe CoreGraphics counters or synchronously mark scalar loss state. Recovery callbacks then dispatch query/refresh work asynchronously to main. Snapshot reads never use `DispatchQueue.main.sync`.

- [x] **Step 3: Sample native keyboard/mouse event identity and timestamp**

Use explicit accepted keyboard/mouse event types with `CGEventSource.counterForEventType`. Inject the counter source into `UserActivityEligibilityState`. On notified recovery, capture one epoch-bound token per source synchronously in the callback. A new loss invalidates only that source's token. Main refresh rejects stale tokens, validates current tokens against native state, and may reconcile workspace/distributed sources that represent the same native session condition without manufacturing a second notified boundary. Source-local ordering is the order callbacks enter the lock; no cross-center ordering is assumed. At aggregate recovery, select the boundary with the latest logical capture sequence; never use numeric minimum/maximum because native counters are opaque and can roll over. Periodic polling may use the transition-time counter as an explicitly best-effort fallback. Deterministic tests cover delayed cross-source loss, stale same-source recovery, workspace/distributed crossed ordering, compound recovery, counter rollover, stable baseline during already-eligible polling, duplicate loss, and concurrent loss visibility. Return the current aggregate plus a native wall timestamp derived from the native observation. Exclude release events so the release half of unlock/wake cannot become a separate trigger. Missing timing returns `nil` and fails closed.

This task adds no event tap or permission. Synthetic-event exclusion remains outside scope because the selected counter API does not distinguish it.

- [x] **Step 4: Implement the bridge method**

```swift
func getUserActivitySnapshot() -> UserActivitySnapshot {
    activityMonitor.snapshot()
}
```

Update the `@unchecked Sendable` comment to explain actual lock-based synchronization.

- [x] **Step 5: Compile after bindings regeneration in Task 6**

No generated Swift file is edited directly.

---

### Task 6: Regenerate Bindings and Verify End-to-End

**Files:**
- Generated locally by script: `macos/Takt/Generated/**` (ignored, never manually edited)

- [x] **Step 1: Run formatting and Rust quality checks**

```bash
make check
```

Result: focused and package tests plus Clippy with `-D warnings` pass. The new scheduler module passes standalone `rustfmt --check`. Repository-wide `make check` remains red only because the same pre-existing rustfmt baseline was reproduced from clean `HEAD`.

- [x] **Step 2: Regenerate Rust library and UniFFI bindings**

```bash
make build-rust
```

Expected: static library and Swift bindings generate successfully.

Confirm, without editing:

```bash
rg -n "UserActivitySnapshot|getUserActivitySnapshot" macos/Takt/Generated/LibTakt.swift
```

- [x] **Step 3: Build macOS Debug and Release**

```bash
make build-debug
make build
```

Expected: both commands finish with `BUILD SUCCEEDED`.

- [ ] **Step 4: Run native smoke matrix with a one-minute delay**

Verify:

1. No post-eligibility input: no execution.
2. First post-unlock input starts delay; no further input required.
3. Lock, screen saver, display sleep, suspend, logout, or fast-user switch resets.
4. Unlock/wake input does not count; next input starts a full delay.
5. A transient loss between polls resets through generation.
6. Overnight suspend rearms within roughly one poll after wake.
7. Continued use does not execute twice on the same day.
8. Exact-time cron tasks remain unchanged.

- [x] **Step 5: Inspect final diff**

```bash
git diff --check
git status --short
```

Expected: only planned source, test, and documentation files changed; no generated files or database artifacts tracked.
