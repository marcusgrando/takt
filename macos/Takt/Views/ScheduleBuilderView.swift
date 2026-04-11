import SwiftUI

struct ScheduleBuilderView: View {
    @Binding var schedule: Schedule
    var core: TaktCore

    @State private var recurring: RecurringState = defaultRecurring
    @State private var oneShotDate = Date()

    private var scheduleType: ScheduleTypeTag {
        switch schedule {
        case .cron: return .cron
        case .oneShot: return .oneShot
        case .dailyFirstUse: return .dailyFirstUse
        // TODO: Phase 4 — add `.calendar` to ScheduleTypeTag and return it here.
        // Phase 1 keeps Calendar schedules unreachable from the UI by falling
        // back to an existing tag. This branch should never execute because
        // no Phase-1 code path produces a Schedule::Calendar value.
        case .calendar: return .cron
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Schedule type tabs
            HStack(spacing: 2) {
                ForEach(ScheduleTypeTag.allCases) { tag in
                    Button {
                        handleTypeChange(tag)
                    } label: {
                        Label(tag.label, systemImage: tag.systemImage)
                            .font(.system(size: 11))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 5)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .background(scheduleType == tag ? Color.accentColor : Color.clear)
                    .foregroundStyle(scheduleType == tag ? .white : .primary)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                }
            }
            .padding(2)
            .background(Color(nsColor: .controlBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 0.5)
            )

            // Type-specific content
            switch schedule {
            case .cron:
                cronContent
            case .oneShot:
                oneShotContent
            case .dailyFirstUse:
                dailyFirstUseContent
            case .calendar:
                CalendarScheduleBuilder(schedule: $schedule, core: core)
            }
        }
        .onAppear {
            syncStateFromSchedule()
        }
        .onChange(of: schedule) {
            syncStateFromSchedule()
        }
    }

    // MARK: - Cron

    @ViewBuilder
    private var cronContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Frequency selector
            HStack {
                Text("Frequency")
                    .font(.system(size: 13, weight: .medium))
                Spacer()
                Picker("", selection: Binding(
                    get: { recurring.frequency },
                    set: { newFreq in
                        recurring.frequency = newFreq
                        emitCron()
                    }
                )) {
                    ForEach(FrequencyType.allCases, id: \.self) { freq in
                        Text(freq.label).tag(freq)
                    }
                }
                .labelsHidden()
                .frame(width: 120)
            }

            switch recurring.frequency {
            case .interval:
                intervalContent
            case .daily:
                TimeFieldView(hour: $recurring.hour, minute: $recurring.minute, onChange: emitCron)
            case .weekly:
                VStack(spacing: 12) {
                    WeekdayGridView(selected: $recurring.weekdays, onChange: emitCron)
                    Divider()
                    TimeFieldView(hour: $recurring.hour, minute: $recurring.minute, onChange: emitCron)
                }
            case .monthly:
                monthlyContent
            case .custom:
                customContent
            }
        }
    }

    @ViewBuilder
    private var intervalContent: some View {
        HStack(spacing: 8) {
            Text("Every")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
            TextField("", value: $recurring.intervalValue, format: .number)
                .textFieldStyle(.roundedBorder)
                .frame(width: 60)
                .multilineTextAlignment(.center)
                .onChange(of: recurring.intervalValue) { emitCron() }
            Picker("", selection: Binding(
                get: { recurring.intervalUnit },
                set: {
                    recurring.intervalUnit = $0
                    emitCron()
                }
            )) {
                Text("Minutes").tag(IntervalUnit.minutes)
                Text("Hours").tag(IntervalUnit.hours)
            }
            .labelsHidden()
            .frame(width: 100)
        }
    }

    @ViewBuilder
    private var monthlyContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Each / On the radio
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Button {
                        recurring.monthlyMode = .each
                        emitCron()
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: recurring.monthlyMode == .each ? "largecircle.fill.circle" : "circle")
                                .foregroundStyle(.primary)
                            Text("Each")
                                .font(.system(size: 13, weight: .medium))
                        }
                    }
                    .buttonStyle(.plain)
                }

                if recurring.monthlyMode == .each {
                    DayGridView(selected: $recurring.monthDays, onChange: emitCron)
                }

                HStack {
                    Button {
                        recurring.monthlyMode = .onThe
                        emitCron()
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: recurring.monthlyMode == .onThe ? "largecircle.fill.circle" : "circle")
                                .foregroundStyle(.primary)
                            Text("On the")
                                .font(.system(size: 13, weight: .medium))
                        }
                    }
                    .buttonStyle(.plain)
                }

                if recurring.monthlyMode == .onThe {
                    HStack(spacing: 8) {
                        Picker("", selection: Binding(
                            get: { recurring.ordinalPosition },
                            set: {
                                recurring.ordinalPosition = $0
                                emitCron()
                            }
                        )) {
                            ForEach(OrdinalPosition.allCases, id: \.self) { pos in
                                Text(pos.label).tag(pos)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 100)

                        Picker("", selection: Binding(
                            get: { recurring.ordinalWeekday },
                            set: {
                                recurring.ordinalWeekday = $0
                                emitCron()
                            }
                        )) {
                            ForEach(0..<7, id: \.self) { i in
                                Text(["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][i]).tag(i)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 120)
                    }
                }
            }

            Divider()

            TimeFieldView(hour: $recurring.hour, minute: $recurring.minute, onChange: emitCron)
        }
    }

    @ViewBuilder
    private var customContent: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Cron expression")
                .font(.system(size: 12, weight: .medium))
            TextField("0 * * * *", text: $recurring.customExpression)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 12, design: .monospaced))
                .onChange(of: recurring.customExpression) {
                    schedule = .cron(expression: recurring.customExpression)
                }
        }
    }

    // MARK: - OneShot

    @ViewBuilder
    private var oneShotContent: some View {
        VStack(spacing: 12) {
            DatePicker("Date & Time", selection: $oneShotDate, in: Date()..., displayedComponents: [.date, .hourAndMinute])
                .datePickerStyle(.field)
                .onChange(of: oneShotDate) {
                    let formatter = ISO8601DateFormatter()
                    schedule = .oneShot(runAt: formatter.string(from: oneShotDate))
                }
        }
    }

    // MARK: - DailyFirstUse

    @ViewBuilder
    private var dailyFirstUseContent: some View {
        if case .dailyFirstUse(let delayMinutes) = schedule {
            VStack(alignment: .leading, spacing: 12) {
                Text("Runs once per day after active keyboard/mouse use with screen unlocked.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)

                HStack(spacing: 8) {
                    Text("After")
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                    TextField("", value: Binding(
                        get: { Int(delayMinutes) },
                        set: { schedule = .dailyFirstUse(delayMinutes: UInt64(max(1, $0))) }
                    ), format: .number)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 60)
                    .multilineTextAlignment(.center)
                    Text(delayMinutes == 1 ? "minute" : "minutes")
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    // MARK: - Helpers

    private func syncStateFromSchedule() {
        if case .cron(let expression) = schedule {
            // Don't re-parse when the change originated from the custom expression field,
            // otherwise parseCron may recognize a preset and switch away from Custom mode.
            if recurring.frequency == .custom && expression == recurring.customExpression {
                return
            }
            let parsed = parseCron(expression)
            if parsed != recurring {
                recurring = parsed
            }
        }
        if case .oneShot(let runAt) = schedule {
            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            if let date = formatter.date(from: runAt) ?? ISO8601DateFormatter().date(from: runAt) {
                if date != oneShotDate {
                    oneShotDate = date
                }
            } else {
                let fallback = Date().addingTimeInterval(3600)
                if fallback != oneShotDate {
                    oneShotDate = fallback
                }
            }
        }
    }

    private func handleTypeChange(_ tag: ScheduleTypeTag) {
        switch tag {
        case .cron:
            schedule = .cron(expression: buildCron(recurring))
        case .oneShot:
            let formatter = ISO8601DateFormatter()
            schedule = .oneShot(runAt: formatter.string(from: oneShotDate))
        case .dailyFirstUse:
            schedule = .dailyFirstUse(delayMinutes: 5)
        }
    }

    private func emitCron() {
        schedule = .cron(expression: buildCron(recurring))
    }
}

// MARK: - ScheduleTypeTag

enum ScheduleTypeTag: String, CaseIterable, Identifiable {
    case cron, oneShot, dailyFirstUse

    var id: String { rawValue }

    var label: String {
        switch self {
        case .cron: return "Recurring"
        case .oneShot: return "One time"
        case .dailyFirstUse: return "Daily first use"
        }
    }

    var systemImage: String {
        switch self {
        case .cron: return "repeat"
        case .oneShot: return "1.circle"
        case .dailyFirstUse: return "sunrise"
        }
    }
}
