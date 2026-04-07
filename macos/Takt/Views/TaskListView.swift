import SwiftUI

struct TaskListView: View {
    var vm: TaskListViewModel
    var openEditor: (EditorParams) -> Void
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Takt")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                Button {
                    vm.currentView = .templates
                } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.borderless)
                .keyboardShortcut("n", modifiers: .command)
            }
            .padding(.horizontal, 16)
            .frame(height: 48)

            Divider()

            // Content
            Group {
                switch vm.currentView {
                case .tasks:
                    taskListContent
                case .templates:
                    TemplateGridView(
                        onSelect: { template in
                            openEditor(EditorParams(taskId: nil, template: template))
                            vm.currentView = .tasks
                        },
                        onBack: { vm.currentView = .tasks }
                    )
                case .history:
                    HistoryView(
                        core: vm.core,
                        onBack: { vm.currentView = .tasks }
                    )
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            Divider()

            // Footer
            HStack {
                Button {
                    NSApplication.shared.terminate(nil)
                } label: {
                    Label("Quit", systemImage: "power")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
                Spacer()
                Button {
                    vm.currentView = .history
                } label: {
                    Label("History", systemImage: "clock")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
            }
            .padding(.horizontal, 16)
            .frame(height: 40)
        }
        .frame(width: 280, height: 400)
    }

    @ViewBuilder
    private var taskListContent: some View {
        if vm.isLoading {
            VStack(spacing: 8) {
                ProgressView()
                Text("Loading tasks...")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error = vm.error {
            VStack(spacing: 12) {
                Text(error)
                    .font(.system(size: 12))
                    .foregroundStyle(.red)
                Button("Retry") {
                    Task { await vm.refresh() }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if vm.tasks.isEmpty {
            VStack(spacing: 16) {
                Image(systemName: "calendar.badge.clock")
                    .font(.system(size: 36))
                    .foregroundStyle(.tertiary)
                VStack(spacing: 4) {
                    Text("No tasks yet")
                        .font(.system(size: 13, weight: .medium))
                    Text("Schedule your first automation.")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Button("New Task") {
                    vm.currentView = .templates
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(vm.tasks, id: \.id) { task in
                        TaskItemView(task: task, vm: vm, openEditor: openEditor)
                        Divider()
                    }
                }
            }
        }
    }
}
