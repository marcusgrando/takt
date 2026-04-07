import SwiftUI

struct TaskEditorView: View {
    @Bindable var vm: TaskEditorViewModel
    var onSave: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var showDiscardAlert = false

    var body: some View {
        VStack(spacing: 0) {
            // Title bar
            HStack {
                Text(vm.isEdit ? "Edit Task" : "New Task")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                Button {
                    Task { await handleSave() }
                } label: {
                    if vm.saving {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text(vm.isEdit ? "Save" : "Create")
                    }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .disabled(vm.saving)
            }
            .padding(.horizontal, 20)
            .frame(height: 56)
            Divider()

            // Scrollable form
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Name
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Name")
                            .font(.system(size: 12, weight: .medium))
                        TextField("Auto-generated from action + schedule", text: Binding(
                            get: { vm.nameManual ? vm.name : vm.displayName },
                            set: { vm.handleNameChange($0) }
                        ))
                        .textFieldStyle(.roundedBorder)
                        .foregroundStyle(vm.nameManual ? .primary : .secondary)

                        if vm.nameManual {
                            Button("Reset to auto-name") {
                                vm.resetAutoName()
                            }
                            .font(.system(size: 11))
                            .buttonStyle(.borderless)
                        }
                    }

                    // Action
                    VStack(alignment: .leading, spacing: 8) {
                        Text("ACTION")
                            .font(.system(size: 10, weight: .medium))
                            .tracking(1.5)
                            .foregroundStyle(.secondary)
                        ActionBuilderView(action: $vm.action, browsers: vm.browsers, fileApps: vm.fileApps, onFilePathChanged: {
                            vm.refreshFileApps()
                        })
                    }

                    Divider()

                    // Schedule
                    VStack(alignment: .leading, spacing: 8) {
                        Text("SCHEDULE")
                            .font(.system(size: 10, weight: .medium))
                            .tracking(1.5)
                            .foregroundStyle(.secondary)
                        ScheduleBuilderView(schedule: $vm.schedule)
                    }

                    // Run if missed
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Run if missed")
                                .font(.system(size: 13))
                            Text("Execute on wake if a run was missed while inactive")
                                .font(.system(size: 11))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Toggle("", isOn: $vm.runIfMissed)
                            .toggleStyle(.switch)
                            .labelsHidden()
                    }

                    // Notify on run
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Notify on run")
                                .font(.system(size: 13))
                            Text("Show a notification when the task executes")
                                .font(.system(size: 11))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Toggle("", isOn: $vm.notifyOnRun)
                            .toggleStyle(.switch)
                            .labelsHidden()
                    }

                    Divider()

                    // Description
                    if !vm.showDescription {
                        Button("+ Add description") {
                            vm.showDescription = true
                        }
                        .font(.system(size: 13))
                        .buttonStyle(.borderless)
                    } else {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Description")
                                .font(.system(size: 12, weight: .medium))
                            TextEditor(text: $vm.description)
                                .font(.system(size: 13))
                                .frame(minHeight: 60)
                                .scrollContentBackground(.hidden)
                                .padding(4)
                                .background(Color(nsColor: .textBackgroundColor))
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                                .overlay(
                                    RoundedRectangle(cornerRadius: 6)
                                        .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                                )
                        }
                    }

                    // Error
                    if let error = vm.error {
                        Text(error)
                            .font(.system(size: 12))
                            .foregroundStyle(.red)
                            .padding(8)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Color.red.opacity(0.1))
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                    }

                    // Delete (edit mode)
                    if vm.isEdit {
                        if vm.confirmDelete {
                            HStack {
                                Text("Delete this task?")
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(.red)
                                Spacer()
                                Button("Cancel") {
                                    vm.confirmDelete = false
                                }
                                .buttonStyle(.borderless)
                                Button {
                                    Task { await handleDelete() }
                                } label: {
                                    if vm.deleting {
                                        ProgressView()
                                            .controlSize(.small)
                                    } else {
                                        Text("Delete")
                                    }
                                }
                                .buttonStyle(.borderedProminent)
                                .tint(.red)
                                .controlSize(.small)
                                .disabled(vm.deleting)
                            }
                            .padding(8)
                            .background(Color.red.opacity(0.1))
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        } else {
                            Button {
                                vm.confirmDelete = true
                            } label: {
                                Label("Delete task", systemImage: "trash")
                                    .foregroundStyle(.red)
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
                .padding(20)
                .padding(.bottom, 20)
            }
        }
        .frame(minWidth: 460, minHeight: 500)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close") { handleClose() }
                    .keyboardShortcut("w", modifiers: .command)
            }
        }
        // Intercept native window close (red X button) via NSWindow delegate
        .background(WindowCloseInterceptor(isDirty: vm.isDirty, onAttemptClose: {
            showDiscardAlert = true
        }))
        .confirmationDialog(
            "You have unsaved changes. Close without saving?",
            isPresented: $showDiscardAlert,
            titleVisibility: .visible
        ) {
            Button("Discard Changes", role: .destructive) {
                dismiss()
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func handleClose() {
        if vm.isDirty {
            showDiscardAlert = true
        } else {
            dismiss()
        }
    }

    private func handleSave() async {
        let success = await vm.save()
        if success {
            onSave()
            dismiss()
        }
    }

    private func handleDelete() async {
        let success = await vm.deleteCurrentTask()
        if success {
            onSave()
            dismiss()
        }
    }
}

/// Intercepts the native window close (red X button) to show unsaved changes dialog.
/// Uses NSWindow.delegate interposition: stores the original SwiftUI delegate, forwards
/// all calls to it, and only adds our windowShouldClose gate. Restores the original
/// delegate when the view is removed to avoid lifecycle regressions.
struct WindowCloseInterceptor: NSViewRepresentable {
    let isDirty: Bool
    let onAttemptClose: () -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            context.coordinator.attach(to: view.window)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        context.coordinator.isDirty = isDirty
        context.coordinator.onAttemptClose = onAttemptClose
    }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.detach()
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    class Coordinator: NSObject, NSWindowDelegate {
        var isDirty = false
        var onAttemptClose: () -> Void = {}
        private weak var window: NSWindow?
        private weak var originalDelegate: NSWindowDelegate?
        private var attached = false

        func attach(to window: NSWindow?) {
            guard let window, !attached else { return }
            originalDelegate = window.delegate
            window.delegate = self
            self.window = window
            attached = true
        }

        func detach() {
            guard attached, let window else { return }
            // Restore original SwiftUI delegate
            window.delegate = originalDelegate
            originalDelegate = nil
            attached = false
        }

        func windowShouldClose(_ sender: NSWindow) -> Bool {
            if isDirty {
                onAttemptClose()
                return false
            }
            // Consult original delegate too (in case SwiftUI has its own gate)
            return originalDelegate?.windowShouldClose?(sender) ?? true
        }

        // Forward all lifecycle events to the original SwiftUI delegate
        override func responds(to aSelector: Selector!) -> Bool {
            if super.responds(to: aSelector) { return true }
            return originalDelegate?.responds(to: aSelector) ?? false
        }

        override func forwardingTarget(for aSelector: Selector!) -> Any? {
            if super.responds(to: aSelector) { return nil }
            if originalDelegate?.responds(to: aSelector) == true {
                return originalDelegate
            }
            return nil
        }
    }
}
