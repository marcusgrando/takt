import SwiftUI
import UserNotifications

@main
struct TaktApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        MenuBarExtra("Takt", systemImage: "clock") {
            if let vm = appDelegate.vm {
                TaskListView(vm: vm)
            } else {
                ProgressView("Starting...")
                    .frame(width: 280, height: 400)
            }
        }
        .menuBarExtraStyle(.window)

        Window("Editor", id: "editor") {
            if let core = appDelegate.core {
                TaskEditorView(
                    vm: TaskEditorViewModel(
                        core: core,
                        taskId: appDelegate.editingTaskId,
                        template: appDelegate.editingTemplate
                    ),
                    onSave: { Task { await appDelegate.vm?.refresh() } }
                )
            }
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 480, height: 600)
    }
}

struct EditorParams: Codable, Hashable {
    var taskId: String?
    var template: ActionTemplate?
}

enum ActionTemplate: String, Codable, Hashable, CaseIterable {
    case openUrl, openFile, openApp, runCommand, notify, webhook, settings

    var label: String {
        switch self {
        case .openUrl: return "Open URL"
        case .openFile: return "Open File"
        case .openApp: return "Open App"
        case .runCommand: return "Run Command"
        case .notify: return "Reminder"
        case .webhook: return "Webhook"
        case .settings: return "Settings"
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
        }
    }

    var defaultAction: Action {
        switch self {
        case .openUrl:
            return .openUrl(url: "", browser: nil, postShortcuts: [], shortcutDelaySecs: 1)
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
        }
    }

    var defaultSchedule: Schedule {
        switch self {
        case .runCommand, .webhook:
            return .cron(expression: "0 * * * *")
        default:
            return .cron(expression: "0 9 * * *")
        }
    }
}

@Observable
final class AppDelegate: NSObject, NSApplicationDelegate {
    var vm: TaskListViewModel?
    var core: TaktCore?
    var editingTaskId: String?
    var editingTemplate: ActionTemplate?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

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
}
