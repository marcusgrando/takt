import SwiftUI

struct HistoryView: View {
    let core: TaktCore
    var onBack: () -> Void

    @State private var logs: [ExecutionLog] = []
    @State private var tasks: [TaskDto] = []
    @State private var isLoading = true
    @State private var error: String?

    private var nameMap: [String: String] {
        Dictionary(uniqueKeysWithValues: tasks.map { ($0.id, $0.name) })
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Button("Back") { onBack() }
                    .buttonStyle(.borderless)
                    .font(.system(size: 13))
                Spacer()
                Text("History")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                Color.clear.frame(width: 40)
            }
            .padding(.horizontal, 16)
            .frame(height: 48)

            Divider()

            // Content
            if isLoading {
                VStack(spacing: 8) {
                    ProgressView()
                    Text("Loading...")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error {
                VStack(spacing: 12) {
                    Text(error)
                        .font(.system(size: 12))
                        .foregroundStyle(.red)
                    Button("Retry") { Task { await loadData() } }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                }
                .padding()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if logs.isEmpty {
                VStack(spacing: 4) {
                    Text("No history yet")
                        .font(.system(size: 13, weight: .medium))
                    Text("Executions will appear here.")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(logs, id: \.id) { log in
                            LogEntryView(log: log, taskName: nameMap[log.taskId] ?? log.taskId)
                            Divider()
                        }
                    }
                }
            }
        }
        .task { await loadData() }
    }

    @MainActor
    private func loadData() async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let logsResult = core.listLogs(taskId: nil, limit: 50)
            async let tasksResult = core.listTasks()
            logs = try await logsResult
            tasks = try await tasksResult
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

// MARK: - LogEntryView

struct LogEntryView: View {
    let log: ExecutionLog
    let taskName: String

    @State private var expanded = false

    private var hasDetails: Bool {
        log.stdout != nil || log.stderr != nil || log.error != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Button {
                    if hasDetails { expanded.toggle() }
                } label: {
                    Image(systemName: expanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 10))
                        .foregroundStyle(hasDetails ? .secondary : .tertiary)
                        .frame(width: 16)
                }
                .buttonStyle(.borderless)
                .disabled(!hasDetails)

                Text(taskName)
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)

                Spacer()

                statusBadge

                Text(formatRelativeTime(log.startedAt))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }

            if expanded && hasDetails {
                VStack(alignment: .leading, spacing: 8) {
                    if let error = log.error {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Error")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(.red)
                            Text(error)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.red)
                                .padding(8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color.red.opacity(0.05))
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                    }
                    if let stdout = log.stdout {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("stdout")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(.secondary)
                            ScrollView {
                                Text(stdout)
                                    .font(.system(size: 11, design: .monospaced))
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(maxHeight: 100)
                            .padding(8)
                            .background(Color(nsColor: .textBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                    }
                    if let stderr = log.stderr {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("stderr")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(.secondary)
                            ScrollView {
                                Text(stderr)
                                    .font(.system(size: 11, design: .monospaced))
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(maxHeight: 100)
                            .padding(8)
                            .background(Color(nsColor: .textBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                    }
                }
                .padding(.leading, 24)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private var statusBadge: some View {
        let (text, bgColor, fgColor) = statusInfo
        Text(text)
            .font(.system(size: 10))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(bgColor)
            .foregroundStyle(fgColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
    }

    private var statusInfo: (String, Color, Color) {
        switch log.status {
        case "success":
            return ("success", Color.green.opacity(0.12), .green)
        case "failure":
            return ("failure", Color.red.opacity(0.12), .red)
        case "schedule_error":
            return ("schedule error", Color.red.opacity(0.12), .red)
        default:
            return ("skipped", Color.secondary.opacity(0.12), .secondary)
        }
    }

    private func formatRelativeTime(_ isoString: String) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let date = formatter.date(from: isoString) ?? ISO8601DateFormatter().date(from: isoString) else {
            return "unknown"
        }
        let diffSec = Int(Date().timeIntervalSince(date))
        if diffSec < 0 { return "just now" }
        if diffSec < 60 { return "\(diffSec)s ago" }
        let m = diffSec / 60
        if m < 60 { return "\(m)m ago" }
        let h = m / 60
        if h < 24 { return "\(h)h ago" }
        let d = h / 24
        if d < 7 { return "\(d)d ago" }
        let df = DateFormatter()
        df.dateStyle = .medium
        return df.string(from: date)
    }
}
