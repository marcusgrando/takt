import Combine
import SwiftUI
import UserNotifications

@main
struct TaktApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @Environment(\.openWindow) private var openWindow

    var body: some Scene {
        MenuBarExtra("Takt", image: "MenuBarIcon") {
            if let vm = appDelegate.vm {
                TaskListView(vm: vm, openEditor: { params in
                    appDelegate.editorParams = params
                    appDelegate.captureAppToReactivate()
                    appDelegate.dismissPopover()
                    NSApp.setActivationPolicy(.regular)
                    openWindow(id: "editor")
                    NSApp.activate(ignoringOtherApps: true)
                })
            } else {
                ProgressView("Starting...")
                    .frame(width: 280, height: 400)
            }
        }
        .menuBarExtraStyle(.window)

        // Editor window — uses id-only Window since Window(for:) has SDK compat issues.
        // Params are stored on AppDelegate before calling openWindow(id:).
        // .id(editorParams) forces SwiftUI to destroy and recreate EditorWindowContent
        // when params change, ensuring a fresh ViewModel for each edit/new task.
        Window("", id: "editor") {
            if let core = appDelegate.core, let params = appDelegate.editorParams {
                EditorWindowContent(
                    core: core,
                    params: params,
                    onSave: { Task { await appDelegate.vm?.refresh() } }
                )
                .id(params)
            }
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 500, height: 600)
    }
}

/// C2 fix: Wraps TaskEditorView with a @State ViewModel that survives body re-evaluations.
/// Without this, every App body re-render would recreate the ViewModel and lose form edits.
struct EditorWindowContent: View {
    let core: TaktCore
    let params: EditorParams
    let onSave: () -> Void

    @State private var vm: TaskEditorViewModel?

    var body: some View {
        Group {
            if let vm {
                TaskEditorView(vm: vm, onSave: onSave)
            } else {
                ProgressView()
            }
        }
        .task {
            if vm == nil {
                vm = TaskEditorViewModel(
                    core: core,
                    taskId: params.taskId,
                    template: params.template
                )
            }
        }
    }
}

struct EditorParams: Codable, Hashable {
    var taskId: String?
    var template: ActionTemplate?
    var openId: UUID = UUID()
}

enum ActionTemplate: String, Codable, Hashable, CaseIterable {
    case openUrl, openFile, openApp, runCommand, notify, webhook, settings, openMeetingLinks

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

    var defaultAction: Action {
        switch self {
        case .openUrl:
            return .openUrl(urls: [""], browser: nil, postShortcuts: [], shortcutDelaySecs: 1)
        case .openFile:
            return .openFile(path: "", app: nil, postShortcuts: [], shortcutDelaySecs: 1)
        case .openApp:
            return .openApp(appPath: "", postShortcuts: [], shortcutDelaySecs: 1)
        case .runCommand:
            return .runCommand(command: "", args: [], shell: .zsh)
        case .notify:
            return .notify(title: "", body: "", sound: true)
        case .webhook:
            return .webhook(url: "", method: .get, headers: [:], body: nil)
        case .settings:
            return .settings(paneUrl: "x-apple.systempreferences:com.apple.settings.General")
        case .openMeetingLinks:
            return .openEventLinks(openConference: true, openNotesLinks: true, browser: nil)
        }
    }

    var defaultSchedule: Schedule {
        switch self {
        case .runCommand, .webhook:
            return .cron(expression: "0 * * * *")
        case .openMeetingLinks:
            return .calendar(calendarId: "", titleContains: nil, minutesBefore: 5)
        default:
            return .cron(expression: "0 9 * * *")
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, ObservableObject {
    @Published var vm: TaskListViewModel?
    @Published var core: TaktCore?
    @Published var editorParams: EditorParams?
    private var appToReactivate: NSRunningApplication?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        // I7: Kill previous instances
        killPreviousInstances()

        // Request notification permission
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }

        // Request accessibility
        let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true] as CFDictionary
        AXIsProcessTrustedWithOptions(opts)

        Task { @MainActor in
            let bridge = MacOSPlatformBridge()
            let core = TaktCore(bridge: bridge)
            do {
                try await core.start()
                self.core = core
                self.vm = TaskListViewModel(core: core)
            } catch {
                print("Failed to initialize Takt: \(error)")
            }
        }
    }

    /// Dismiss the MenuBarExtra popover by clicking its status bar button.
    /// This triggers the native dismiss path so the icon highlight resets correctly.
    func dismissPopover() {
        for window in NSApp.windows {
            // The MenuBarExtra panel holds a reference to its status item button
            guard window.responds(to: NSSelectorFromString("statusItem")),
                  let statusItem = window.value(forKey: "statusItem") as? NSStatusItem,
                  let button = statusItem.button else { continue }
            button.performClick(nil)
            return
        }
    }

    /// Captures the app that was frontmost before the editor activates Takt.
    func captureAppToReactivate() {
        guard let frontApp = NSWorkspace.shared.frontmostApplication else {
            appToReactivate = nil
            return
        }

        let currentPID = ProcessInfo.processInfo.processIdentifier
        appToReactivate = frontApp.processIdentifier == currentPID ? nil : frontApp
    }

    /// Called by macOS when the last "real" window closes.
    /// The MenuBarExtra panel is not counted, so this fires when the editor closes.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        hideFromDock()
        return false // Don't terminate — keep running as menu bar app
    }

    /// Hides the app from Dock and Cmd+Tab switcher.
    func hideFromDock() {
        let previous = appToReactivate
        appToReactivate = nil

        // 1. Order out any lingering non-MenuBarExtra windows.
        //    SwiftUI Window scenes can leave hidden NSWindows that prevent
        //    macOS from honoring the activation policy change.
        for window in NSApp.windows {
            if window.responds(to: NSSelectorFromString("statusItem")),
               window.value(forKey: "statusItem") is NSStatusItem { continue }
            if window.isVisible { window.orderOut(nil) }
        }

        // 2. Cooperatively yield activation (macOS 14+).
        if let previous, !previous.isTerminated {
            NSApp.yieldActivation(to: previous)
        } else if let next = NSWorkspace.shared.runningApplications
            .first(where: { $0.activationPolicy == .regular && $0 != .current }) {
            NSApp.yieldActivation(to: next)
        } else {
            NSApp.deactivate()
        }

        // 3. The prohibited toggle trick: going through .prohibited forces
        //    macOS to fully evict us from the Cmd+Tab switcher before we
        //    settle on .accessory.
        NSApp.setActivationPolicy(.prohibited)
        DispatchQueue.main.async {
            NSApp.setActivationPolicy(.accessory)
        }
    }

    // I7: Single instance enforcement
    private func killPreviousInstances() {
        guard let bundleId = Bundle.main.bundleIdentifier else { return }
        let myPID = ProcessInfo.processInfo.processIdentifier
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId)
        for app in running where app.processIdentifier != myPID {
            app.forceTerminate()
        }
    }
}
