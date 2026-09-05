import SwiftUI

struct TaskItemView: View {
    let task: TaskDto
    var vm: TaskListViewModel
    var openEditor: (EditorParams) -> Void

    @State private var isRunning = false
    @State private var isToggling = false
    @State private var isDeleting = false
    @State private var confirmDelete = false
    @State private var itemError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: actionSymbol)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(task.enabled ? actionColor : .secondary)
                    .frame(width: 18)
                    .help(actionLabel)
                    .accessibilityLabel(actionLabel)

                Text(task.name)
                    .font(Font(NSFont.menuFont(ofSize: 13)))
                    .foregroundStyle(task.enabled ? .primary : .secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .help(task.name)
                    .frame(maxWidth: .infinity, alignment: .leading)

                if task.health != .healthy {
                    Button {
                        openEditor(EditorParams(taskId: task.id, template: nil))
                    } label: {
                        Image(systemName: healthBadgeIcon(task.health))
                            .font(.system(size: 12))
                            .foregroundStyle(healthBadgeColor(task.health))
                            .frame(width: 22, height: 28)
                    }
                    .buttonStyle(.plain)
                    .help(healthBadgeTooltip(task.health))
                    .accessibilityLabel(healthBadgeText(task.health))
                }

                Toggle("Enable \(task.name)", isOn: Binding(
                    get: { task.enabled },
                    set: { _ in
                        guard !isToggling else { return }
                        Task { await toggleEnabled() }
                    }
                ))
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .disabled(isToggling)
                .help(task.enabled ? "Disable task" : "Enable task")

                HStack(spacing: 2) {
                    if canRunManually {
                        Button {
                            Task { await handleRun() }
                        } label: {
                            Group {
                                if isRunning {
                                    ProgressView()
                                        .controlSize(.small)
                                        .frame(width: 14, height: 14)
                                } else {
                                    Image(systemName: "play.fill")
                                        .font(.system(size: 11))
                                        .foregroundStyle(.primary.opacity(0.8))
                                }
                            }
                            .frame(width: 24, height: 28)
                        }
                        .buttonStyle(.borderless)
                        .disabled(isRunning)
                        .help("Run now")
                        .accessibilityLabel("Run \(task.name) now")
                    } else {
                        Color.clear.frame(width: 24, height: 28)
                            .accessibilityHidden(true)
                    }

                    Button {
                        openEditor(EditorParams(taskId: task.id, template: nil))
                    } label: {
                        Image(systemName: "slider.horizontal.3")
                            .font(.system(size: 11))
                            .foregroundStyle(.primary.opacity(0.8))
                            .frame(width: 24, height: 28)
                    }
                    .buttonStyle(.borderless)
                    .help("Edit")
                    .accessibilityLabel("Edit \(task.name)")

                    Button {
                        confirmDelete = true
                    } label: {
                        Group {
                            if isDeleting {
                                ProgressView()
                                    .controlSize(.small)
                                    .frame(width: 14, height: 14)
                            } else {
                                Image(systemName: "trash")
                                    .font(.system(size: 11))
                                    .foregroundStyle(.red.opacity(0.8))
                            }
                        }
                        .frame(width: 24, height: 28)
                    }
                    .buttonStyle(.borderless)
                    .disabled(isDeleting)
                    .help("Delete")
                    .accessibilityLabel("Delete \(task.name)")
                }
            }

            // Confirm delete
            if confirmDelete {
                HStack {
                    Text("Delete?")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.red)
                    Spacer()
                    Button("Cancel") { confirmDelete = false }
                        .buttonStyle(.borderless)
                        .font(.system(size: 12))
                    Button("Delete") {
                        Task { await handleDelete() }
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.red)
                    .controlSize(.small)
                    .font(.system(size: 12))
                }
                .padding(8)
                .background(Color.red.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            // Error
            if let itemError {
                Text(itemError)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }

    // MARK: - Helpers

    /// Actions that require calendar event context cannot be triggered manually.
    private var canRunManually: Bool {
        switch task.action {
        case .openEventLinks: return false
        default: return true
        }
    }

    private var actionLabel: String {
        switch task.action {
        case .runCommand: return "shell"
        case .openUrl: return "url"
        case .notify: return "notify"
        case .openFile: return "file"
        case .openApp: return "app"
        case .webhook: return "webhook"
        case .settings: return "settings"
        case .openEventLinks: return "event"
        }
    }

    private var actionSymbol: String {
        switch task.action {
        case .runCommand: return "terminal"
        case .openUrl: return "link"
        case .notify: return "bell"
        case .openFile: return "doc"
        case .openApp: return "app"
        case .webhook: return "network"
        case .settings: return "gearshape"
        case .openEventLinks: return "calendar"
        }
    }

    private var actionColor: Color {
        switch task.action {
        case .openUrl: return .blue
        case .runCommand: return .purple
        case .notify: return .orange
        case .openFile: return .green
        case .openApp: return .cyan
        case .webhook: return .teal
        case .settings: return .gray
        case .openEventLinks: return .teal
        }
    }

    private func healthBadgeText(_ health: TaskHealth) -> String {
        switch health {
        case .healthy: return ""
        case .calendarNotFound: return "Calendar missing"
        case .calendarAccessDenied: return "Access denied"
        case .calendarAccessNotDetermined: return "Grant access"
        }
    }

    private func healthBadgeIcon(_ health: TaskHealth) -> String {
        switch health {
        case .healthy: return ""
        case .calendarNotFound: return "calendar.badge.exclamationmark"
        case .calendarAccessDenied: return "lock.fill"
        case .calendarAccessNotDetermined: return "hand.raised.fill"
        }
    }

    private func healthBadgeColor(_ health: TaskHealth) -> Color {
        switch health {
        case .healthy: return .primary
        case .calendarNotFound: return .red
        case .calendarAccessDenied: return .orange
        case .calendarAccessNotDetermined: return .yellow
        }
    }

    private func healthBadgeTooltip(_ health: TaskHealth) -> String {
        switch health {
        case .healthy: return ""
        case .calendarNotFound:
            return "The calendar referenced by this task no longer exists. Click to edit or delete."
        case .calendarAccessDenied:
            return "Calendar access is denied. Click to edit, or grant access in System Settings."
        case .calendarAccessNotDetermined:
            return "Calendar access has not been granted yet. Click to edit and grant access."
        }
    }

    // MARK: - Actions

    private func toggleEnabled() async {
        isToggling = true
        defer { isToggling = false }
        itemError = nil
        await vm.toggleEnabled(task)
    }

    private func handleRun() async {
        isRunning = true
        defer { isRunning = false }
        itemError = nil
        await vm.runNow(task.id)
    }

    private func handleDelete() async {
        isDeleting = true
        confirmDelete = false
        itemError = nil
        await vm.deleteTask(task.id)
        isDeleting = false
    }
}
