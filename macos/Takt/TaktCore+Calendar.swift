import Foundation
import EventKit

// Extension that implements the calendar-related PlatformBridge methods.
// Separated from TaktCore+Bridge.swift so EventKit imports stay scoped.

private final class CalendarStoreHolder: @unchecked Sendable {
    static let shared = CalendarStoreHolder()
    let store = EKEventStore()
    private init() {}
}

private func mapAuthorizationStatus(_ raw: EKAuthorizationStatus) -> CalendarAccessStatus {
    switch raw {
    case .notDetermined:
        return .notDetermined
    case .fullAccess:
        return .authorized
    case .authorized:
        return .authorized
    default:
        return .denied
    }
}

private func isoString(from date: Date) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: date)
}

private func parseIsoDate(_ s: String) -> Date? {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let d = formatter.date(from: s) { return d }
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.date(from: s)
}

private func colorHex(from cgColor: CGColor?) -> String? {
    guard let cg = cgColor, let comps = cg.components, comps.count >= 3 else { return nil }
    let r = Int((comps[0] * 255).rounded())
    let g = Int((comps[1] * 255).rounded())
    let b = Int((comps[2] * 255).rounded())
    return String(format: "#%02X%02X%02X", r, g, b)
}

private let conferenceDomainPattern: NSRegularExpression = {
    let pattern = #"https?://(?:[\w.-]*\.)?(meet\.google\.com|zoom\.us|teams\.microsoft\.com|webex\.com)[^\s<>"']*"#
    return try! NSRegularExpression(pattern: pattern, options: [.caseInsensitive])
}()

private func detectConferenceUrl(notes: String?, location: String?) -> String? {
    for candidate in [notes, location] {
        guard let text = candidate else { continue }
        let range = NSRange(text.startIndex..., in: text)
        if let match = conferenceDomainPattern.firstMatch(in: text, range: range),
           let swiftRange = Range(match.range, in: text) {
            return String(text[swiftRange])
        }
    }
    return nil
}

private func mapEvent(_ ek: EKEvent, calendarId: String) -> CalendarEvent {
    let conferenceUrl = detectConferenceUrl(notes: ek.notes, location: ek.location)
    return CalendarEvent(
        id: ek.eventIdentifier ?? UUID().uuidString,
        title: ek.title ?? "",
        start: isoString(from: ek.startDate),
        end: isoString(from: ek.endDate),
        notes: ek.notes,
        location: ek.location,
        url: ek.url?.absoluteString,
        conferenceUrl: conferenceUrl,
        calendarId: calendarId
    )
}

// All EventKit calls are dispatched to the main thread.
// These bridge methods run on a tokio background thread (via lib.rs spawn).
// EventKit may internally synchronize with main for authorization status,
// calendar queries, and event fetches. The main thread is free because
// UniFFI's async polling yields it via withUnsafeContinuation.
//
// Pattern: dispatch work to DispatchQueue.main.async, block the tokio
// thread with a semaphore until done.

private func runOnMain<T>(_ block: @escaping () throws -> T) throws -> T {
    if Thread.isMainThread {
        return try block()
    }
    let semaphore = DispatchSemaphore(value: 0)
    var result: Result<T, Error>!
    DispatchQueue.main.async {
        do {
            result = .success(try block())
        } catch {
            result = .failure(error)
        }
        semaphore.signal()
    }
    semaphore.wait()
    return try result.get()
}

extension MacOSPlatformBridge {

    func getCalendarAccessStatus() throws -> CalendarAccessStatus {
        return try runOnMain {
            mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
        }
    }

    func requestCalendarAccess() throws -> CalendarAccessStatus {
        let semaphore = DispatchSemaphore(value: 0)
        let store = CalendarStoreHolder.shared.store
        DispatchQueue.main.async {
            if #available(macOS 14.0, *) {
                store.requestFullAccessToEvents { _, _ in semaphore.signal() }
            } else {
                store.requestAccess(to: .event) { _, _ in semaphore.signal() }
            }
        }
        semaphore.wait()
        return try runOnMain {
            mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
        }
    }

    func listCalendars() throws -> [CalendarInfo] {
        return try runOnMain {
            let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
            guard status == .authorized else {
                throw TaktError.CalendarAccessDenied
            }
            let store = CalendarStoreHolder.shared.store
            return store.calendars(for: .event).map { cal in
                CalendarInfo(
                    id: cal.calendarIdentifier,
                    title: cal.title,
                    source: cal.source?.title ?? "Local",
                    colorHex: colorHex(from: cal.cgColor)
                )
            }
        }
    }

    func fetchEventsInWindow(
        calendarId: String,
        lookbackMinutes: UInt32,
        lookaheadMinutes: UInt32
    ) throws -> [CalendarEvent] {
        return try runOnMain {
            let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
            guard status == .authorized else {
                throw TaktError.CalendarAccessDenied
            }
            let store = CalendarStoreHolder.shared.store
            guard let cal = store.calendar(withIdentifier: calendarId) else {
                throw TaktError.CalendarNotFound(id: calendarId)
            }
            let now = Date()
            let start = now.addingTimeInterval(-Double(lookbackMinutes) * 60.0)
            let end = now.addingTimeInterval(Double(lookaheadMinutes) * 60.0)
            let predicate = store.predicateForEvents(withStart: start, end: end, calendars: [cal])
            let events = store.events(matching: predicate)
            return events.map { mapEvent($0, calendarId: calendarId) }
        }
    }

    func fetchEventInstance(
        calendarId: String,
        eventId: String,
        eventStart: String
    ) throws -> CalendarEvent? {
        return try runOnMain {
            let status = mapAuthorizationStatus(EKEventStore.authorizationStatus(for: .event))
            guard status == .authorized else {
                throw TaktError.CalendarAccessDenied
            }
            guard let anchor = parseIsoDate(eventStart) else {
                throw TaktError.Execution(msg: "invalid_event_start")
            }
            let store = CalendarStoreHolder.shared.store
            guard let cal = store.calendar(withIdentifier: calendarId) else {
                throw TaktError.CalendarNotFound(id: calendarId)
            }
            // ±6h window around the anchor, then filter by eventIdentifier.
            let windowSeconds: TimeInterval = 6 * 60 * 60
            let predicate = store.predicateForEvents(
                withStart: anchor.addingTimeInterval(-windowSeconds),
                end: anchor.addingTimeInterval(windowSeconds),
                calendars: [cal]
            )
            let matches = store.events(matching: predicate).filter { $0.eventIdentifier == eventId }
            guard !matches.isEmpty else { return nil }
            let best = matches.min { a, b in
                abs(a.startDate.timeIntervalSince(anchor)) < abs(b.startDate.timeIntervalSince(anchor))
            }!
            return mapEvent(best, calendarId: calendarId)
        }
    }
}
