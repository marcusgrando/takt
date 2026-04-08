import Foundation
import UserNotifications
import AppKit
import CoreGraphics

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

    func isUserActive(idleThresholdSecs: UInt64) -> Bool {
        // 1. Check if screen is locked
        if let dict = CGSessionCopyCurrentDictionary() as? [String: Any] {
            // "CGSSessionScreenIsLocked" is true when the login window / lock screen is showing
            if let locked = dict["CGSSessionScreenIsLocked"] as? Bool, locked {
                return false
            }
            // "kCGSSessionOnConsoleKey" is false when the session is not on the console (e.g. fast user switch)
            if let onConsole = dict["kCGSSessionOnConsoleKey"] as? Bool, !onConsole {
                return false
            }
        }

        // 2. Check HID idle time (keyboard + mouse combined)
        let idleSeconds = CGEventSource.secondsSinceLastEventType(
            .combinedSessionState,
            eventType: CGEventType(rawValue: ~0)!  // all event types
        )
        return idleSeconds < Double(idleThresholdSecs)
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
