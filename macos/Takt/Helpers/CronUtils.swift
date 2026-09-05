import Foundation

// MARK: - Types

enum FrequencyType: String, CaseIterable {
    case interval, daily, weekly, monthly, custom

    var label: String {
        switch self {
        case .interval: return "Interval"
        case .daily: return "Daily"
        case .weekly: return "Weekly"
        case .monthly: return "Monthly"
        case .custom: return "Custom"
        }
    }
}

enum IntervalUnit: String, CaseIterable {
    case minutes, hours

    var label: String {
        switch self {
        case .minutes: return "Minutes"
        case .hours: return "Hours"
        }
    }
}

enum MonthlyMode: String {
    case each, onThe
}

enum OrdinalPosition: String, CaseIterable {
    case first, second, third, fourth, last

    var label: String { rawValue.capitalized }
}

// MARK: - RecurringState

struct RecurringState: Equatable {
    var frequency: FrequencyType = .daily
    var intervalValue: Int = 1
    var intervalUnit: IntervalUnit = .hours
    var weekdays: [Int] = [1] // 0=Sun, 1=Mon, ..., 6=Sat
    var monthlyMode: MonthlyMode = .each
    var monthDays: [Int] = [1] // 1-31
    var ordinalPosition: OrdinalPosition = .first
    var ordinalWeekday: Int = 1 // 0=Sun, ..., 6=Sat
    var hour: Int = 9
    var minute: Int = 0
    var customExpression: String = "0 * * * *"
}

let defaultRecurring = RecurringState()

// MARK: - Build Cron

func buildCron(_ state: RecurringState) -> String {
    switch state.frequency {
    case .interval:
        if state.intervalUnit == .minutes {
            return state.intervalValue == 1
                ? "* * * * *"
                : "*/\(state.intervalValue) * * * *"
        }
        // hours
        return state.intervalValue == 1
            ? "0 * * * *"
            : "0 */\(state.intervalValue) * * *"

    case .daily:
        return "\(state.minute) \(state.hour) * * *"

    case .weekly:
        let days = state.weekdays.isEmpty ? "1" : state.weekdays.sorted().map(String.init).joined(separator: ",")
        return "\(state.minute) \(state.hour) * * \(days)"

    case .monthly:
        if state.monthlyMode == .each {
            let days = state.monthDays.isEmpty ? "1" : state.monthDays.sorted().map(String.init).joined(separator: ",")
            return "\(state.minute) \(state.hour) \(days) * *"
        }
        // "On the" mode
        let ordMap: [OrdinalPosition: String] = [
            .first: "1", .second: "2", .third: "3", .fourth: "4", .last: "L"
        ]
        let ord = ordMap[state.ordinalPosition] ?? "1"
        if ord == "L" {
            return "\(state.minute) \(state.hour) * * \(state.ordinalWeekday)L"
        }
        return "\(state.minute) \(state.hour) * * \(state.ordinalWeekday)#\(ord)"

    case .custom:
        return state.customExpression
    }
}

// MARK: - Parse Cron

func parseCron(_ expression: String) -> RecurringState {
    let parts = expression.trimmingCharacters(in: .whitespaces).split(separator: " ", omittingEmptySubsequences: true).map(String.init)
    guard parts.count == 5 else {
        var s = defaultRecurring
        s.frequency = .custom
        s.customExpression = expression
        return s
    }

    let minField = parts[0]
    let hourField = parts[1]
    let domField = parts[2]
    let monField = parts[3]
    let dowField = parts[4]

    // Interval detection
    if domField == "*" && monField == "*" && dowField == "*" {
        // Minutes interval: */N * * * * or * * * * *
        if hourField == "*" && (minField == "*" || minField.range(of: #"^\*/\d+$"#, options: .regularExpression) != nil) {
            if let minInterval = parseStepInterval(minField) {
                var s = defaultRecurring
                s.frequency = .interval
                s.intervalUnit = .minutes
                s.intervalValue = minInterval
                return s
            }
        }

        // Hours interval: 0 */N * * * or 0 * * * *
        if minField == "0" && (hourField == "*" || hourField.range(of: #"^\*/\d+$"#, options: .regularExpression) != nil) {
            if let hourInterval = parseStepInterval(hourField) {
                var s = defaultRecurring
                s.frequency = .interval
                s.intervalUnit = .hours
                s.intervalValue = hourInterval
                return s
            }
        }

        // Daily: M H * * *
        if let min = Int(minField), let hour = Int(hourField) {
            var s = defaultRecurring
            s.frequency = .daily
            s.hour = hour
            s.minute = min
            return s
        }
    }

    // Weekly: M H * * 0,1,3
    if domField == "*" && dowField != "*" && !dowField.contains("#") && !dowField.contains("L") {
        if let min = Int(minField), let hour = Int(hourField), monField == "*" {
            if let weekdays = parseIntegerList(dowField, allowed: 0...6) {
                var s = defaultRecurring
                s.frequency = .weekly
                s.hour = hour
                s.minute = min
                s.weekdays = weekdays
                return s
            }
        }
    }

    // Monthly "each": M H 1,15 * *
    if domField != "*" && !domField.contains("/") && dowField == "*" && monField == "*" {
        if let min = Int(minField), let hour = Int(hourField) {
            if let monthDays = parseIntegerList(domField, allowed: 1...31) {
                var s = defaultRecurring
                s.frequency = .monthly
                s.hour = hour
                s.minute = min
                s.monthlyMode = .each
                s.monthDays = monthDays
                return s
            }
        }
    }

    // Monthly "on the": M H * * 1#2 or M H * * 1L
    if domField == "*" && dowField != "*" && (dowField.contains("#") || dowField.contains("L")) && monField == "*" {
        if let min = Int(minField), let hour = Int(hourField) {
            if dowField.contains("#") {
                let parts = dowField.split(separator: "#")
                if parts.count == 2, let weekday = Int(parts[0]), let ordNum = Int(parts[1]) {
                    let posMap: [Int: OrdinalPosition] = [1: .first, 2: .second, 3: .third, 4: .fourth]
                    if let pos = posMap[ordNum] {
                        var s = defaultRecurring
                        s.frequency = .monthly
                        s.hour = hour
                        s.minute = min
                        s.monthlyMode = .onThe
                        s.ordinalPosition = pos
                        s.ordinalWeekday = weekday
                        return s
                    }
                }
            } else if dowField.hasSuffix("L") {
                let dayStr = String(dowField.dropLast())
                if let weekday = Int(dayStr) {
                    var s = defaultRecurring
                    s.frequency = .monthly
                    s.hour = hour
                    s.minute = min
                    s.monthlyMode = .onThe
                    s.ordinalPosition = .last
                    s.ordinalWeekday = weekday
                    return s
                }
            }
        }
    }

    // Fallback: custom
    var s = defaultRecurring
    s.frequency = .custom
    s.customExpression = expression
    return s
}

// MARK: - Helpers

private func parseIntegerList(_ field: String, allowed: ClosedRange<Int>) -> [Int]? {
    var values: [Int] = []
    for part in field.split(separator: ",", omittingEmptySubsequences: false) {
        guard let value = Int(part), allowed.contains(value) else { return nil }
        values.append(value)
    }
    return values.isEmpty ? nil : values
}

private func parseStepInterval(_ field: String) -> Int? {
    if field == "*" { return 1 }
    if let range = field.range(of: #"^\*/(\d+)$"#, options: .regularExpression) {
        let match = String(field[range])
        let numStr = String(match.dropFirst(2)) // drop "*/"
        return Int(numStr)
    }
    return nil
}
