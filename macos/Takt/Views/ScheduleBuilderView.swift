import SwiftUI

struct ScheduleBuilderView: View {
    @Binding var schedule: Schedule

    @State private var recurring: RecurringState = defaultRecurring
    @State private var oneShotDate = Date()
    @State private var initialized = false

    private var scheduleType: ScheduleTypeTag {
        switch schedule {
        case .cron: return .cron
        case .oneShot: return .oneShot
        case .dailyFirstUse: return .dailyFirstUse
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            // Schedule type tabs
            Picker("Type", selection: Binding(
                get: { scheduleType },
                set: { handleTypeChange($0) }
            )) {
                ForEach(ScheduleTypeTag.allCases) { tag in
                    Text(tag.label).tag(tag)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            // Type-specific content
            switch schedule {
            case .cron:
                cronContent
            case .oneShot:
                oneShotContent
            case .dailyFirstUse:
                dailyFirstUseContent
            }
        }
        .onAppear {
            guard !initialized else { return }
            initialized = true
            if case .cron(let expression) = schedule {
                recurring = parseCron(expression)
            }
            if case .oneShot(let runAt) = schedule {
                let formatter = ISO8601DateFormatter()
                formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
                if let date = formatter.date(from: runAt) ?? ISO8601DateFormatter().date(from: runAt) {
                    oneShotDate = date
                } else {
                    oneShotDate = Date().addingTimeInterval(3600)
                }
            }
        }
    }

    // MARK: - Cron

    @ViewBuilder
    private var cronContent: some View {
        VStack(spacing: 16) {
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
        VStack(spacing: 12) {
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
                Text("Runs once per day after continuous active use of your Mac.")
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
}
