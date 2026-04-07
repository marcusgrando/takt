import Foundation
import UserNotifications
import AppKit

// MacOSPlatformBridge implements the UniFFI-generated PlatformBridge protocol.
// @unchecked Sendable is safe: no mutable state, all methods use thread-safe APIs.
final class MacOSPlatformBridge: PlatformBridge, @unchecked Sendable {
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
