use crate::models::{Schedule, Action, TaskDto, ExecutionLog};
use crate::AppState;
use tauri::State;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSURL, NSString};

const STATUS_SUCCESS: &str = "success";
const STATUS_FAILURE: &str = "failure";

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
    if task.enabled {
        state.scheduler.schedule_task(&task).await.map_err(|e| e.to_string())?;
    }
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
    let task = state.store.update_task(&id, name, description, enabled, schedule, action)
        .await.map_err(|e| e.to_string())?;
    state.scheduler.remove_task(&id).await.map_err(|e| e.to_string())?;
    if task.enabled {
        state.scheduler.schedule_task(&task).await.map_err(|e| e.to_string())?;
    }
    Ok(task)
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
    // NOTE: log_execution records Utc::now() internally for both started_at and
    // finished_at; timing is approximate (does not capture true execution duration).
    let result = state.executor.execute(&task.action).await;
    let (status, stdout, stderr, error) = match result {
        Ok(r) => (STATUS_SUCCESS, r.stdout, r.stderr, None),
        Err(e) => (STATUS_FAILURE, None, None, Some(e.to_string())),
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
    state.store.list_logs(task_id.as_deref(), limit.unwrap_or(50).min(500))
        .await.map_err(|e| e.to_string())
}

fn apps_for_url(url: &NSURL) -> Vec<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app_urls = workspace.URLsForApplicationsToOpenURL(url);
    let mut apps: Vec<String> = Vec::new();

    for app_url in app_urls.to_vec() {
        if let Some(path) = app_url.path() {
            let path_str: String = path.to_string();
            if let Some(name) = path_str.split('/').last() {
                let clean = name.trim_end_matches(".app");
                if !clean.is_empty() {
                    apps.push(clean.to_string());
                }
            }
        }
    }

    apps.sort();
    apps.dedup();
    apps
}

#[tauri::command]
pub fn list_browsers() -> Vec<String> {
    let Some(url) = NSURL::URLWithString(&NSString::from_str("https://example.com")) else {
        return vec![];
    };
    apps_for_url(&url)
}

#[tauri::command]
pub fn list_apps_for_file(path: String) -> Vec<String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
    apps_for_url(&url)
}
