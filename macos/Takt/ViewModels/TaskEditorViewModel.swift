import Foundation
import SwiftUI

@Observable
final class TaskEditorViewModel {
    let core: TaktCore
    let taskId: String?
    let isEdit: Bool

    // Form fields
    var name: String = ""
    var nameManual: Bool = false
    var description: String = ""
    var showDescription: Bool = false
    var action: Action
    var schedule: Schedule
    var runIfMissed: Bool = true
    var notifyOnRun: Bool = false

    // State
    var saving = false
    var deleting = false
    var confirmDelete = false
    var error: String?
    var isLoaded = false

    // Snapshot for dirty checking
    private var initialName: String = ""
    private var initialDescription: String = ""
    private var initialAction: Action
    private var initialSchedule: Schedule
    private var initialRunIfMissed: Bool = true
    private var initialNotifyOnRun: Bool = false

    // Workspace data
    var browsers: [String] = []
    var fileApps: [String] = []

    var displayName: String {
        if nameManual && !name.isEmpty {
            return name
        }
        return AutoName.generateAutoName(action: action, schedule: schedule)
    }

    var isDirty: Bool {
        let currentName = nameManual ? name : AutoName.generateAutoName(action: action, schedule: schedule)
        let origName = isEdit ? initialName : AutoName.generateAutoName(action: initialAction, schedule: initialSchedule)
        if currentName != origName { return true }
        if description != initialDescription { return true }
        if schedule != initialSchedule { return true }
        if action != initialAction { return true }
        if runIfMissed != initialRunIfMissed { return true }
        if notifyOnRun != initialNotifyOnRun { return true }
        return false
    }

    init(core: TaktCore, taskId: String?, template: ActionTemplate?) {
        self.core = core
        self.taskId = taskId
        self.isEdit = taskId != nil

        // Set defaults from template or fallback
        let tpl = template ?? .openUrl
        self.action = tpl.defaultAction
        self.schedule = tpl.defaultSchedule
        self.initialAction = tpl.defaultAction
        self.initialSchedule = tpl.defaultSchedule

        if taskId != nil {
            // Will be loaded async
            self.nameManual = true
        }

        Task { @MainActor in
            await loadData()
        }
    }

    @MainActor
    func loadData() async {
        // Load browsers
        browsers = WorkspaceHelper.listBrowsers()

        // Load existing task if editing
        if let taskId {
            do {
                if let task = try await core.getTask(id: taskId) {
                    name = task.name
                    nameManual = true
                    description = task.description ?? ""
                    showDescription = !(task.description ?? "").isEmpty
                    action = task.action
                    schedule = task.schedule
                    runIfMissed = task.runIfMissed
                    notifyOnRun = task.notifyOnRun

                    // Save snapshot
                    initialName = task.name
                    initialDescription = task.description ?? ""
                    initialAction = task.action
                    initialSchedule = task.schedule
                    initialRunIfMissed = task.runIfMissed
                    initialNotifyOnRun = task.notifyOnRun
                }
            } catch {
                self.error = error.localizedDescription
            }
        }

        isLoaded = true
        refreshFileApps()
    }

    @MainActor
    func refreshFileApps() {
        if case .openFile(let path, _, _, _) = action, !path.isEmpty {
            fileApps = WorkspaceHelper.listAppsForFile(path: path)
        } else {
            fileApps = []
        }
    }

    func handleNameChange(_ value: String) {
        nameManual = true
        name = value
    }

    func resetAutoName() {
        nameManual = false
        name = ""
    }

    @MainActor
    func save() async -> Bool {
        error = nil

        // Validation
        if case .cron(let expression) = schedule {
            let expr = expression.trimmingCharacters(in: .whitespaces)
            let parts = expr.split(separator: " ", omittingEmptySubsequences: true)
            if parts.count != 5 {
                error = "Cron expression must have exactly 5 fields (min hour dom mon dow)"
                return false
            }
            do {
                try core.validateCron(expression: expr)
            } catch {
                self.error = String(describing: error)
                return false
            }
        }

        if case .oneShot(let runAt) = schedule {
            let formatter = ISO8601DateFormatter()
            if formatter.date(from: runAt) == nil {
                error = "Invalid date"
                return false
            }
        }

        switch action {
        case .openFile(let path, _, _, _):
            if path.trimmingCharacters(in: .whitespaces).isEmpty {
                error = "File path is required"; return false
            }
        case .openUrl(let urls, _, _, _):
            let hasValidUrl = urls.contains { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
            if !hasValidUrl {
                error = "At least one URL is required"; return false
            }
        case .openApp(let appPath, _, _):
            if appPath.trimmingCharacters(in: .whitespaces).isEmpty {
                error = "Application path is required"; return false
            }
        case .runCommand(let command, _, _):
            if command.trimmingCharacters(in: .whitespaces).isEmpty {
                error = "Command is required"; return false
            }
        case .notify(let title, _, _):
            if title.trimmingCharacters(in: .whitespaces).isEmpty {
                error = "Notification title is required"; return false
            }
        case .webhook(let url, _, _, _):
            if url.trimmingCharacters(in: .whitespaces).isEmpty {
                error = "Webhook URL is required"; return false
            }
        case .settings:
            break
        // TODO: Phase 4 — validate OpenEventLinks fields here. Phase 1 accepts
        // it without validation since no UI path produces this action value.
        case .openEventLinks:
            break
        }

        saving = true
        defer { saving = false }

        let finalName = displayName.trimmingCharacters(in: .whitespaces)
        let desc = description.trimmingCharacters(in: .whitespaces)

        do {
            if isEdit, let taskId {
                let descValue: String?? = desc.isEmpty ? .some(nil) : .some(desc)
                let params = UpdateTaskParams(
                    id: taskId,
                    name: finalName,
                    description: descValue,
                    enabled: nil,
                    runIfMissed: runIfMissed,
                    notifyOnRun: notifyOnRun,
                    schedule: schedule,
                    action: action
                )
                _ = try await core.updateTask(params: params)
            } else {
                let params = CreateTaskParams(
                    name: finalName,
                    description: desc.isEmpty ? nil : desc,
                    runIfMissed: runIfMissed,
                    notifyOnRun: notifyOnRun,
                    schedule: schedule,
                    action: action
                )
                _ = try await core.createTask(params: params)
            }
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    @MainActor
    func deleteCurrentTask() async -> Bool {
        guard let taskId else { return false }
        deleting = true
        defer { deleting = false; confirmDelete = false }
        do {
            try await core.deleteTask(id: taskId)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }
}
