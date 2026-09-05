# Takt interface

Takt is a native macOS menu bar scheduler for personal task automation.
Preserve its compact SwiftUI interface, system typography, semantic colors,
SF Symbols, and native light and dark appearances.

## Startup and scene state

`AppDelegate` owns initialization, the task list model, and editor parameters.
Scene content observes it directly with `@ObservedObject` in `MenuBarContent`
and `EditorSceneContent`. Reading published properties only inside `App.body`
does not reliably refresh the menu or editor after asynchronous changes.

`MenuLayout` defines a shared 340 by 360 point panel for every menu state.
Show progress while initializing,
the task list on success, and a native `ContentUnavailableView` with the actual
error and a quit action on failure. Startup outcomes are logged under the
`app.takt` subsystem and `startup` category.

`TaskListView` owns task navigation and `EditorWindowContent` preserves editor
state for the current parameters. Rust remains the source of truth for task
data, validation, and scheduling. Calendar permissions remain in the native
bridge and are requested only through the existing user action.

## Menu density

The 40 point header uses only the existing 24 point logo, with an accessible
Takt label. The user prefers the logo alone, without a separate wordmark.
The footer is 32 points high.
Use the regular native macOS menu font for task names. The user preferred the
logo but rejected the heavier typography, so avoid medium or semibold task names.
Healthy task rows use one 36 point line with a 13 point title, an action-type
symbol, a switch, and aligned run, edit, and delete controls. Keep the run slot
empty for actions that require calendar context. Errors and delete confirmation
may expand the row. Preserve full names in tooltips and accessible labels,
and keep health warnings actionable. Use semantic colors for both appearances.

## Verification

Run the Swift regression tests and the signed Release build. Verify startup
through LaunchServices with existing tasks, then open and reopen the menu and
editor. Compilation alone does not verify scene observation.
