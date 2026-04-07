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
