# Fix 5 Validated Issues Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 5 validated bugs/issues ranging from Alta to Baixa severity.

**Architecture:** Each fix is independent. Tasks ordered by severity (highest first). All changes are localized to specific files with no cross-task dependencies.

**Tech Stack:** Swift/SwiftUI (macOS app), Rust (libtakt core library), croner crate for cron parsing.

---

### Task 1: Fix ScheduleBuilderView desync with async-loaded schedule (Alta)

**Files:**
- Modify: `macos/Takt/Views/ScheduleBuilderView.swift:1-72`

**Problem:** `onAppear` runs once with template defaults before `TaskEditorViewModel.loadData()` completes. The `initialized` guard prevents re-parsing when the binding updates.

**Fix:** Replace `onAppear` + `initialized` guard with `onChange(of: schedule)` that syncs local state reactively. Keep `onAppear` only for initial sync, but also react to subsequent binding changes.

- [ ] **Step 1: Replace onAppear guard with reactive sync**

Replace the `@State private var initialized = false` and the `.onAppear` block (lines 8, 57-72) with an `onChange`-based approach plus an `onAppear` for initial load:

```swift
struct ScheduleBuilderView: View {
    @Binding var schedule: Schedule

    @State private var recurring: RecurringState = defaultRecurring
    @State private var oneShotDate = Date()

    // ... (keep scheduleType, body, cronContent, etc. unchanged)

    // Replace the old .onAppear block at the end of the VStack with:
    .onAppear {
        syncStateFromSchedule()
    }
    .onChange(of: schedule) {
        syncStateFromSchedule()
    }
```

Remove the `@State private var initialized = false` property (line 8).

- [ ] **Step 2: Extract sync helper function**

Add this private method to ScheduleBuilderView (before `handleTypeChange`):

```swift
private func syncStateFromSchedule() {
    if case .cron(let expression) = schedule {
        let parsed = parseCron(expression)
        if parsed != recurring {
            recurring = parsed
        }
    }
    if case .oneShot(let runAt) = schedule {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: runAt) ?? ISO8601DateFormatter().date(from: runAt) {
            if date != oneShotDate {
                oneShotDate = date
            }
        } else {
            let fallback = Date().addingTimeInterval(3600)
            if fallback != oneShotDate {
                oneShotDate = fallback
            }
        }
    }
}
```

The `!=` guards prevent infinite loops: `syncStateFromSchedule` reads `schedule` → sets `recurring` → `emitCron()` writes back to `schedule` → `onChange` fires → but `recurring` hasn't changed so no write.

NOTE: This requires `RecurringState` to conform to `Equatable`. Check if it already does — if not, add conformance.

- [ ] **Step 3: Build and verify**

Run: `cd macos && xcodebuild build -scheme Takt -destination 'platform=macOS' 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add macos/Takt/Views/ScheduleBuilderView.swift
git commit -m "fix: sync ScheduleBuilderView state reactively via onChange instead of one-shot onAppear"
```

---

### Task 2: Clarify shell args UI label (Média)

**Files:**
- Modify: `macos/Takt/Views/ActionBuilderView.swift:269-275`

**Problem:** Label says "Arguments (one per line)" but for sh/bash/zsh, args are concatenated into the shell command string, not passed as separate argv. Users expect argv isolation.

- [ ] **Step 1: Update the label to clarify shell behavior**

In `ActionBuilderView.swift`, the `runCommandFields` section around lines 269-275, change the subtitle based on the current shell:

```swift
VStack(alignment: .leading, spacing: 4) {
    HStack(spacing: 4) {
        Text("Arguments")
            .font(.system(size: 12, weight: .medium))
        switch shell {
        case .sh, .bash, .zsh:
            Text("(appended to command as shell text)")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        case .python:
            Text("(one per line, passed as sys.argv)")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        case .appleScript:
            Text("(one per line)")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }
```

- [ ] **Step 2: Build and verify**

Run: `cd macos && xcodebuild build -scheme Takt -destination 'platform=macOS' 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add macos/Takt/Views/ActionBuilderView.swift
git commit -m "fix: clarify shell args label based on shell type to avoid argv confusion"
```

---

### Task 3: Replace debug DB wipe with env var flag (Média)

**Files:**
- Modify: `libtakt/src/db.rs:10-12`

**Problem:** `cfg!(debug_assertions)` unconditionally deletes SQLite on every debug launch, preventing testing of persistence, missed runs, migrations, and history.

- [ ] **Step 1: Replace cfg check with env var**

In `db.rs`, replace lines 10-12:

```rust
// Old:
if cfg!(debug_assertions) {
    let _ = std::fs::remove_file(&db_path);
}

// New:
if std::env::var("TAKT_RESET_DB").is_ok() {
    let _ = std::fs::remove_file(&db_path);
}
```

- [ ] **Step 2: Build and verify**

Run: `cd /Users/marcus.grando/git/cronmac && cargo build -p libtakt 2>&1 | tail -10`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add libtakt/src/db.rs
git commit -m "fix: replace debug_assertions DB wipe with TAKT_RESET_DB env var for opt-in reset"
```

---

### Task 4: Calculate next_run_at for cron tasks (Média)

**Files:**
- Modify: `libtakt/src/scheduler.rs:385-410` (execute_and_log function)

**Problem:** `execute_and_log` always passes `None` for `next_run_at`. The field is never populated.

**Fix:** Add `schedule` parameter to `execute_and_log`. For Cron schedules, compute next fire time using croner and pass it to `update_last_run`.

- [ ] **Step 1: Add helper function for next cron time**

Add this function in `scheduler.rs`, before `execute_and_log`:

```rust
/// Compute the next fire time for a cron expression from now.
fn next_cron_fire(expression: &str) -> Option<String> {
    let expr = normalize_cron(expression);
    let cron = croner::Cron::new(&expr).parse().ok()?;
    let next = cron.find_next_occurrence(&Local::now(), false).ok()?;
    Some(next.to_rfc3339())
}
```

- [ ] **Step 2: Update execute_and_log signature and body**

Add `schedule: &Schedule` parameter:

```rust
pub(crate) async fn execute_and_log(
    executor: &dyn ActionExecutor,
    store: &TaskStore,
    bridge: &dyn PlatformBridge,
    task_id: &str,
    task_name: &str,
    notify: bool,
    action: &Action,
    schedule: &Schedule,
) -> Result<(), String> {
    let result = executor.execute(action).await;
    let (status, stdout, stderr, error) = match &result {
        Ok(r) => ("success", r.stdout.clone(), r.stderr.clone(), None),
        Err(e) => ("failure", None, None, Some(e.to_string())),
    };
    if notify {
        send_run_notification(bridge, task_name, status == "success");
    }
    let _ = store
        .log_execution(task_id, status, stdout, stderr, error.clone())
        .await;
    let next_run = match schedule {
        Schedule::Cron { expression } => next_cron_fire(expression),
        _ => None,
    };
    let _ = store.update_last_run(task_id, next_run).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
```

- [ ] **Step 3: Update all call sites to pass schedule**

There are 4 call sites in `scheduler.rs`. Each one needs the schedule passed. The schedule is available at each site:

1. **catch_up_missed** (~line 149): The `task` variable has `task.schedule`. Clone it before the spawn:
   ```rust
   let schedule = task.schedule.clone();
   // inside spawn:
   execute_and_log(&*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule)
   ```

2. **Cron job closure** (~line 191): Add schedule clone in the closure captures:
   ```rust
   let schedule = task.schedule.clone();
   // inside closure:
   execute_and_log(&*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule)
   ```

3. **OneShot spawn** (~line 231): Already has task data. Add schedule:
   ```rust
   let schedule = task.schedule.clone();
   // inside spawn:
   execute_and_log(&*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule)
   ```

4. **DailyFirstUse spawn** (~line 348): Same pattern:
   ```rust
   let schedule = task.schedule.clone();
   // inside spawn:
   execute_and_log(&*executor, &store, &*bridge, &task_id, &task_name, notify, &action, &schedule)
   ```

- [ ] **Step 4: Build and verify**

Run: `cd /Users/marcus.grando/git/cronmac && cargo build -p libtakt 2>&1 | tail -10`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add libtakt/src/scheduler.rs
git commit -m "fix: compute and persist next_run_at for cron tasks after execution"
```

---

### Task 5: Eliminate action defaults duplication (Baixa)

**Files:**
- Modify: `macos/Takt/Views/ActionBuilderView.swift:611-628` (handleTypeChange)
- Modify: `macos/Takt/TaktApp.swift` (add ActionTemplate init from ActionTypeTag)

**Problem:** `ActionBuilderView.handleTypeChange` hardcodes the same default values as `ActionTemplate.defaultAction`. Two sources of truth.

**Fix:** Add a computed property on `ActionTypeTag` that maps to `ActionTemplate`, then use `template.defaultAction` in `handleTypeChange`.

- [ ] **Step 1: Add template mapping to ActionTypeTag**

In `ActionBuilderView.swift`, add a computed property to the `ActionTypeTag` enum (around line 648):

```swift
var template: ActionTemplate {
    switch self {
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

- [ ] **Step 2: Simplify handleTypeChange**

Replace the entire `handleTypeChange` method body (lines 611-628):

```swift
private func handleTypeChange(_ tag: ActionTypeTag) {
    action = tag.template.defaultAction
}
```

- [ ] **Step 3: Build and verify**

Run: `cd macos && xcodebuild build -scheme Takt -destination 'platform=macOS' 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add macos/Takt/Views/ActionBuilderView.swift
git commit -m "fix: deduplicate action defaults by delegating to ActionTemplate.defaultAction"
```
