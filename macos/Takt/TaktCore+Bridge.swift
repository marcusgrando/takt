import AppKit
import Foundation
import UserNotifications

// MacOSPlatformBridge implements the UniFFI-generated PlatformBridge protocol.
// @unchecked Sendable is safe because UserActivityMonitor delegates mutable
// activity state to one lock-protected state object; remaining methods use
// thread-safe APIs or dispatch AppKit work asynchronously to the main thread.
final class MacOSPlatformBridge: PlatformBridge, @unchecked Sendable {
    private let activityMonitor: UserActivityMonitor

    init() {
        dispatchPrecondition(condition: .onQueue(.main))
        activityMonitor = UserActivityMonitor()
    }

    func sendNotification(title: String, body: String, sound: Bool) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        if sound { content.sound = .default }
        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    func runOnMainSync(callbackId: UInt64) {
        DispatchQueue.main.async {
            executeCallback(callbackId: callbackId)
        }
    }

    func getUserActivitySnapshot() -> UserActivitySnapshot {
        activityMonitor.snapshot()
    }

}

// NSWorkspace lookups — moved from Rust to Swift for correct main-thread access
@MainActor
enum WorkspaceHelper {
    static func listBrowsers() -> [String] {
        guard let url = URL(string: "https://example.com") else { return [] }
        return NSWorkspace.shared.urlsForApplications(toOpen: url)
            .compactMap { $0.deletingPathExtension().lastPathComponent }
            .sorted()
    }

    static func listAppsForFile(path: String) -> [String] {
        let url = URL(fileURLWithPath: path)
        return NSWorkspace.shared.urlsForApplications(toOpen: url)
            .compactMap { $0.deletingPathExtension().lastPathComponent }
            .sorted()
    }
}
