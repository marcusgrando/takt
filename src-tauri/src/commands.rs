use crate::models::{Schedule, Action, TaskDto, ExecutionLog};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskDto>, String> {
    state.store.list_tasks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_task(id: String, state: State<'_, AppState>) -> Result<Option<TaskDto>, String> {
    state.store.get_task(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_task(
    name: String,
    description: Option<String>,
    schedule: Schedule,
    action: Action,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    let task = state.store.create_task(name, description, schedule.clone(), action.clone())
        .await.map_err(|e| e.to_string())?;
    state.scheduler.schedule_task(&task).await.map_err(|e| e.to_string())?;
    Ok(task)
}

#[tauri::command]
pub async fn update_task(
    id: String,
    name: Option<String>,
    description: Option<Option<String>>,
    enabled: Option<bool>,
    schedule: Option<Schedule>,
    action: Option<Action>,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    state.store.update_task(&id, name, description, enabled, schedule, action)
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_task(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.scheduler.remove_task(&id).await.map_err(|e| e.to_string())?;
    state.store.delete_task(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_task_now(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let task = state.store.get_task(&id).await.map_err(|e| e.to_string())?
        .ok_or_else(|| "Task not found".to_string())?;
    let executor = crate::executor::current_executor();
    let result = executor.execute(&task.action).await;
    let (status, stdout, stderr, error) = match result {
        Ok(r) => ("success", r.stdout, r.stderr, None),
        Err(e) => ("failure", None, None, Some(e.to_string())),
    };
    state.store.log_execution(&id, status, stdout, stderr, error)
        .await.map_err(|e| e.to_string())?;
    state.store.update_last_run(&id, None)
        .await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_logs(
    task_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ExecutionLog>, String> {
    state.store.list_logs(task_id.as_deref(), limit.unwrap_or(50))
        .await.map_err(|e| e.to_string())
}
