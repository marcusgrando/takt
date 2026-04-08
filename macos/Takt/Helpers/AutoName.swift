import Foundation

enum AutoName {

    static func generateAutoName(action: Action, schedule: Schedule) -> String {
        "\(describeAction(action)) \u{2014} \(describeSchedule(schedule))"
    }

    // MARK: - Action description

    private static func describeAction(_ action: Action) -> String {
        switch action {
        case .openUrl(let urls, _, _, _):
            let nonEmpty = urls.filter { !$0.isEmpty }
            if nonEmpty.isEmpty { return "Open URL" }
            let first = nonEmpty[0]
            let label: String
            if let parsed = URL(string: first), let host = parsed.host {
                let clean = host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
                label = clean
            } else {
                label = String(first.prefix(30))
            }
            if nonEmpty.count > 1 {
                return "Open \(label) +\(nonEmpty.count - 1)"
            }
            return "Open \(label)"

        case .openFile(let path, _, _, _):
            if path.isEmpty { return "Open file" }
            let name = (path as NSString).lastPathComponent
            return "Open \(name)"

        case .openApp(let appPath, _, _):
            if appPath.isEmpty { return "Open app" }
            let name = (appPath as NSString).lastPathComponent.replacingOccurrences(of: ".app", with: "")
            return "Open \(name)"

        case .runCommand(let command, _, _):
            if command.isEmpty { return "Run command" }
            let cmd = command.split(whereSeparator: { $0.isWhitespace }).first.map(String.init) ?? command
            let base = (cmd as NSString).lastPathComponent
            return "Run \(base)"

        case .notify(let title, _, _):
            if title.isEmpty { return "Reminder" }
            return "Reminder: \(title)"

        case .webhook(let url, let method, _, _):
            if url.isEmpty { return "Webhook" }
            let methodStr = httpMethodString(method)
            if let parsed = URL(string: url), let host = parsed.host {
                let clean = host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
                return "\(methodStr) \(clean)"
            }
            return "\(methodStr) webhook"

        case .settings(let paneUrl):
            let parts = paneUrl.split(separator: ":")
            let id = parts.count > 1 ? String(parts[1]) : ""
            let name = id.split(separator: ".").last.map(String.init) ?? "Settings"
            let cleaned = name
                .replacingOccurrences(of: "-Settings", with: "")
                .replacingOccurrences(of: ".extension", with: "")
                .replacingOccurrences(of: "-", with: " ")
            return "Open \(cleaned.isEmpty ? "Settings" : cleaned)"
        }
    }

    // MARK: - Schedule description

    private static func describeSchedule(_ schedule: Schedule) -> String {
        switch schedule {
        case .dailyFirstUse(let delayMinutes):
            return "Daily after \(delayMinutes) min"

        case .oneShot(let runAt):
            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            if let date = formatter.date(from: runAt) ?? ISO8601DateFormatter().date(from: runAt) {
                let df = DateFormatter()
                df.dateFormat = "MMM d, h:mm a"
                return df.string(from: date)
            }
            return "One time"

        case .cron(let expression):
            let state = parseCron(expression)
            switch state.frequency {
            case .interval:
                if state.intervalUnit == .minutes {
                    return "Every \(state.intervalValue) min"
                }
                let suffix = state.intervalValue > 1 ? "s" : ""
                return "Every \(state.intervalValue) hour\(suffix)"

            case .daily:
                return "Daily at \(formatTime(state.hour, state.minute))"

            case .weekly:
                let dayNames = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                let days = state.weekdays.sorted().map { dayNames[$0] }.joined(separator: ", ")
                return "\(days) at \(formatTime(state.hour, state.minute))"

            case .monthly:
                if state.monthlyMode == .each {
                    let days = state.monthDays.sorted().map(String.init).joined(separator: ", ")
                    return "Monthly on \(days) at \(formatTime(state.hour, state.minute))"
                }
                let dayNames = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                return "Monthly \(state.ordinalPosition.rawValue) \(dayNames[state.ordinalWeekday]) at \(formatTime(state.hour, state.minute))"

            case .custom:
                return "Cron \(expression)"
            }
        }
    }

    // MARK: - Helpers

    private static func formatTime(_ hour: Int, _ minute: Int) -> String {
        String(format: "%02d:%02d", hour, minute)
    }

    private static func httpMethodString(_ method: HttpMethod) -> String {
        switch method {
        case .get: return "GET"
        case .post: return "POST"
        case .put: return "PUT"
        case .patch: return "PATCH"
        case .delete: return "DELETE"
        }
    }
}
