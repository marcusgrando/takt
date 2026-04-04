# Smoke Test — cronmac v0.1.0

**Date:** 2026-04-03  
**Platform:** macOS (aarch64 / Apple Silicon)  
**Build tool:** Bun + Tauri 2 + Rust  

---

## Build Results

| Step | Command | Result |
|------|---------|--------|
| Rust unit tests | `cargo test` (src-tauri/) | **PASS** — 7/7 tests passed |
| Frontend build | `bun run build` | **PASS** — TypeScript + Vite built in 1.38s |
| Full Tauri bundle | `bunx tauri build` | **PASS** — Rust release compiled in ~1m 46s |

---

## Smoke Test Checklist

### App compiles without errors ✓
`bunx tauri build` completed successfully. Release binary built at:
`src-tauri/target/release/tauri-app`

### All Rust unit tests pass ✓
```
running 7 tests
test executor::tests::tests::test_run_shell_command ... ok
test db::tests::test_db_connects_and_migrates ... ok
test store::tests::test_delete_task ... ok
test store::tests::test_create_and_list_tasks ... ok
test store::tests::test_log_execution ... ok
test store::tests::test_update_task ... ok
test executor::tests::tests::test_open_url_skipped_in_ci ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### TypeScript compiles without errors ✓
`tsc && vite build` ran clean — no TypeScript errors, 1910 modules transformed.

### `.app` bundle created in expected location ✓
```
src-tauri/target/release/bundle/macos/cronmac.app
src-tauri/target/release/bundle/dmg/cronmac_0.1.0_aarch64.dmg
```
Bundle structure verified: `Contents/Info.plist`, `Contents/MacOS/tauri-app`, `Contents/Resources/` all present.  
`CFBundleShortVersionString` = `0.1.0`  
`LSMinimumSystemVersion` = `13.0`

### Tauri config: `trayIcon` configured ✓ / `activationPolicy: Accessory` (dock suppressed) ✓
- `tauri.conf.json` has `"trayIcon": { "iconPath": "icons/tray-icon.png", "iconAsTemplate": true }` configured.
- `src-tauri/src/lib.rs:33` sets `app.set_activation_policy(tauri::ActivationPolicy::Accessory)` on macOS — this hides the app from the Dock and removes the menubar entry, making it a pure menu-bar/accessory app.
- **Note:** `Info.plist` does not include `LSUIElement` directly; Tauri sets this at runtime via `set_activation_policy`. This is the standard Tauri 2 approach.

### LaunchAgent plist generation logic verified via code review ✓
`src-tauri/src/launch_agent.rs` implements:
- `launch_agent_path()` — resolves `~/Library/LaunchAgents/com.cronmac.app.plist`
- `app_bundle_path()` — detects whether running from a `.app` bundle (walks ancestors for `.app` extension)
- `plist_content()` — generates valid Apple plist XML with `Label`, `ProgramArguments`, `RunAtLoad: true`, `KeepAlive: false`
- `xml_escape()` — properly escapes `&`, `<`, `>` in paths
- `ensure_registered()` — only registers if running as bundled `.app`, skips if plist already exists, creates parent dir, runs `launchctl load`

### HistoryView component present and wired ✓
- `src/components/HistoryView.tsx` exists
- `src/App.tsx:7` imports `HistoryView`
- `src/App.tsx:82-84` renders `<HistoryView onBack={() => setView('list')} />` when `view === 'history'`
- Footer button on `App.tsx:89-94` routes to the history view

### TaskWizard component present and wired ✓
- `src/components/TaskWizard.tsx` exists
- `src/App.tsx:6` imports `TaskWizard`
- `src/App.tsx:44-52` renders `<TaskWizard ... />` for the `add` view
- `src/App.tsx:71-80` renders `<TaskWizard initialTask={editTask} ... />` for the `edit` view

---

## Artifacts

| Artifact | Path | Size |
|----------|------|------|
| Release binary | `src-tauri/target/release/tauri-app` | — |
| macOS `.app` bundle | `src-tauri/target/release/bundle/macos/cronmac.app` | — |
| `.dmg` installer | `src-tauri/target/release/bundle/dmg/cronmac_0.1.0_aarch64.dmg` | ~6.7 MB |

---

## Summary

All 8 smoke test items **PASS**. The v0.1.0 build is clean, all Rust tests pass, the TypeScript frontend compiles without errors, and the `.app` bundle + `.dmg` installer were successfully created for aarch64 macOS.
