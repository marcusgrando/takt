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
                        .foregroundStyle(.primary.opacity(0.6))
                }
                .buttonStyle(.borderless)
                Spacer()
                Button {
                    vm.currentView = .history
                } label: {
                    Label("History", systemImage: "clock")
                        .font(.system(size: 12))
                        .foregroundStyle(.primary.opacity(0.6))
                }
                .buttonStyle(.borderless)
            }
            .padding(.horizontal, 16)
            .frame(height: 40)
        }
        .frame(width: 280, height: 400)
        .background(PopoverEscHandler())
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

/// Closes the MenuBarExtra popover when Escape is pressed.
/// Uses a local event monitor that works regardless of first responder.
struct PopoverEscHandler: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            context.coordinator.parentWindow = view.window
            context.coordinator.startMonitor()
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.stopMonitor()
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    class Coordinator {
        weak var parentWindow: NSWindow?
        private var monitor: Any?

        func startMonitor() {
            guard monitor == nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self, event.keyCode == 53, // Escape
                      let window = self.parentWindow, window.isKeyWindow else {
                    return event
                }
                // Defer to next run loop to avoid conflicting with current event processing.
                // The performClick must happen after the ESC event is fully consumed.
                DispatchQueue.main.async {
                    // Try native dismiss via status item button (resets highlight)
                    for w in NSApp.windows {
                        if let si = w.value(forKey: "statusItem") as? NSStatusItem,
                           let button = si.button {
                            button.performClick(nil)
                            return
                        }
                    }
                    // Fallback: brute-force hide
                    window.orderOut(nil)
                }
                return nil
            }
        }

        func stopMonitor() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        deinit { stopMonitor() }
    }
}
