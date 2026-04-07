import Foundation

@Observable
final class TaskListViewModel {
    let core: TaktCore
    var tasks: [TaskDto] = []
    var isLoading = false
    var error: String?
    var currentView: ViewMode = .tasks

    enum ViewMode { case tasks, templates, history }

    init(core: TaktCore) {
        self.core = core
        Task { await refresh() }
    }

    @MainActor
    func refresh() async {
        isLoading = true
        defer { isLoading = false }
        do {
            tasks = try await core.listTasks()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    @MainActor
    func toggleEnabled(_ task: TaskDto) async {
        do {
            let params = UpdateTaskParams(
                id: task.id, name: nil, description: nil,
                enabled: !task.enabled, runIfMissed: nil,
                notifyOnRun: nil, schedule: nil, action: nil
            )
            _ = try await core.updateTask(params: params)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }

    @MainActor
    func deleteTask(_ id: String) async {
        do {
            try await core.deleteTask(id: id)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }

    @MainActor
    func runNow(_ id: String) async {
        do {
            try await core.runTaskNow(id: id)
            await refresh()
        } catch {
            self.error = error.localizedDescription
        }
    }
}
