import SwiftUI

struct TaskItemView: View {
    let task: TaskDto
    var vm: TaskListViewModel
    var openEditor: (EditorParams) -> Void
    @Environment(\.openWindow) private var openWindow

    @State private var isRunning = false
    @State private var isToggling = false
    @State private var isDeleting = false
    @State private var confirmDelete = false
    @State private var itemError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // Row 1: name
            Text(task.name)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .truncationMode(.tail)

            // Row 2: status + badge + toggle + actions
            HStack(spacing: 8) {
                Circle()
                    .fill(task.enabled ? Color.green : Color.secondary.opacity(0.25))
                    .frame(width: 6, height: 6)

                Text(actionLabel)
                    .font(.system(size: 10))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(actionColor.opacity(0.15))
                    .foregroundStyle(actionColor)
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                Toggle("", isOn: Binding(
                    get: { task.enabled },
                    set: { _ in
                        guard !isToggling else { return }
                        Task { await toggleEnabled() }
                    }
                ))
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()

                Spacer()

                HStack(spacing: 10) {
                    Button {
                        Task { await handleRun() }
                    } label: {
                        if isRunning {
                            ProgressView()
                                .controlSize(.small)
                                .frame(width: 14, height: 14)
                        } else {
                            Image(systemName: "play.fill")
                                .font(.system(size: 11))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .buttonStyle(.borderless)
                    .disabled(isRunning)
                    .help("Run now")

                    Button {
                        openEditor(EditorParams(taskId: task.id, template: nil))
                    } label: {
                        Image(systemName: "slider.horizontal.3")
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.borderless)
                    .help("Edit")

                    Button {
                        confirmDelete = true
                    } label: {
                        if isDeleting {
                            ProgressView()
                                .controlSize(.small)
                                .frame(width: 14, height: 14)
                        } else {
                            Image(systemName: "trash")
                                .font(.system(size: 11))
                                .foregroundStyle(.red.opacity(0.7))
                        }
                    }
                    .buttonStyle(.borderless)
                    .disabled(isDeleting)
                    .help("Delete")
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
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }

    // MARK: - Helpers

    private var actionLabel: String {
        switch task.action {
        case .runCommand: return "shell"
        case .openUrl: return "url"
        case .notify: return "notify"
        case .openFile: return "file"
        case .openApp: return "app"
        case .webhook: return "webhook"
        case .settings: return "settings"
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
