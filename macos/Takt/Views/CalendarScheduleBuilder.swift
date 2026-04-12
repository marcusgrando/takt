import SwiftUI

struct CalendarScheduleBuilder: View {
    @Binding var schedule: Schedule
    var core: TaktCore

    @State private var accessStatus: CalendarAccessStatus = .notDetermined
    @State private var calendars: [CalendarInfo] = []
    @State private var loading = false
    @State private var loadError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            switch accessStatus {
            case .notDetermined:
                notDeterminedView
            case .denied:
                deniedView
            case .authorized:
                authorizedView
            }
        }
        .task {
            await refreshAccessStatus()
        }
    }

    private var notDeterminedView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Takt needs access to your calendars to trigger tasks from events.")
                .font(.system(size: 13))
            HStack(spacing: 12) {
                Button("Grant Calendar Access") {
                    Task { await requestAccess() }
                }
                .buttonStyle(.borderedProminent)
                Button("Open System Settings") {
                    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars") {
                        NSWorkspace.shared.open(url)
                    }
                }
            }
            Text("If the dialog doesn't appear, use System Settings to grant access, then reopen this editor.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        }
    }

    private var deniedView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Calendar access is denied.")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.orange)
            Text("Grant access in System Settings → Privacy & Security → Calendars, then reopen this editor.")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            Button("Open System Settings") {
                if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars") {
                    NSWorkspace.shared.open(url)
                }
            }
        }
    }

    @ViewBuilder
    private var authorizedView: some View {
        if loading {
            ProgressView("Loading calendars…")
        } else if let err = loadError {
            Text(err).foregroundStyle(.red).font(.system(size: 12))
        } else if calendars.isEmpty {
            Text("No calendars found. Add a calendar in the system Calendar app, then reopen this editor.")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        } else {
            formFields
        }
    }

    @ViewBuilder
    private var formFields: some View {
        let (currentCalendarId, currentTitle, currentMinutes) = currentValues()

        HStack {
            Text("Calendar").font(.system(size: 13, weight: .medium))
            Spacer()
            Picker("", selection: Binding(
                get: { currentCalendarId },
                set: { newId in
                    updateSchedule(calendarId: newId, titleContains: currentTitle, minutesBefore: currentMinutes)
                }
            )) {
                Text("Select a calendar…").tag("")
                ForEach(calendars, id: \.id) { cal in
                    Text("\(cal.title) (\(cal.source))").tag(cal.id)
                }
            }
            .labelsHidden()
            .frame(width: 220)
        }

        HStack(alignment: .top) {
            Text("Title contains").font(.system(size: 13, weight: .medium))
            Spacer()
            TextField("any event", text: Binding(
                get: { currentTitle ?? "" },
                set: { newValue in
                    let trimmed = newValue.trimmingCharacters(in: .whitespaces)
                    updateSchedule(
                        calendarId: currentCalendarId,
                        titleContains: trimmed.isEmpty ? nil : trimmed,
                        minutesBefore: currentMinutes
                    )
                }
            ))
            .textFieldStyle(.roundedBorder)
            .frame(width: 220)
        }

        HStack {
            Text("Trigger").font(.system(size: 13, weight: .medium))
            Spacer()
            Stepper(
                "\(currentMinutes) minutes before",
                value: Binding(
                    get: { Int(currentMinutes) },
                    set: { newValue in
                        let clamped = max(0, min(120, newValue))
                        updateSchedule(
                            calendarId: currentCalendarId,
                            titleContains: currentTitle,
                            minutesBefore: UInt32(clamped)
                        )
                    }
                ),
                in: 0...120
            )
            .labelsHidden()
            Text("\(currentMinutes) min before")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }

    private func currentValues() -> (String, String?, UInt32) {
        if case .calendar(let calendarId, let titleContains, let minutesBefore) = schedule {
            return (calendarId, titleContains, minutesBefore)
        }
        return ("", nil, 5)
    }

    private func updateSchedule(calendarId: String, titleContains: String?, minutesBefore: UInt32) {
        schedule = .calendar(
            calendarId: calendarId,
            titleContains: titleContains,
            minutesBefore: minutesBefore
        )
    }

    private func refreshAccessStatus() async {
        do {
            let status = try await core.getCalendarAccessStatus()
            self.accessStatus = status
            if status == .authorized {
                await loadCalendars()
            }
        } catch {
            self.loadError = (error as NSError).localizedDescription
        }
    }

    private func requestAccess() async {
        do {
            let status = try await core.requestCalendarAccess()
            self.accessStatus = status
            if status == .authorized {
                await loadCalendars()
            }
        } catch {
            self.loadError = (error as NSError).localizedDescription
        }
    }

    private func loadCalendars() async {
        self.loading = true
        defer { self.loading = false }
        do {
            let list = try await core.listCalendars()
            self.calendars = list
            self.loadError = nil
        } catch {
            self.loadError = "Failed to load calendars: \((error as NSError).localizedDescription)"
        }
    }
}
