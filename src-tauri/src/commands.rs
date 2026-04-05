use crate::models::{Action, ExecutionLog, Schedule, TaskDto};
use crate::AppState;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskDto>, String> {
    state.store.list_tasks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_task(id: String, state: State<'_, AppState>) -> Result<Option<TaskDto>, String> {
    state.store.get_task(&id).await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_task(
    name: String,
    description: Option<String>,
    run_if_missed: Option<bool>,
    notify_on_run: Option<bool>,
    schedule: Schedule,
    action: Action,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    let task = state
        .store
        .create_task(
            name,
            description,
            run_if_missed.unwrap_or(true),
            notify_on_run.unwrap_or(false),
            schedule.clone(),
            action.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;
    if task.enabled {
        if let Err(e) = state.scheduler.schedule_task(&task).await {
            let mut errors = vec![e.to_string()];
            if let Err(rb_err) = state.store.delete_task(&task.id).await {
                errors.push(format!("rollback delete failed: {}", rb_err));
                // Can't delete — disable so it doesn't appear as active
                if let Err(dis_err) = state
                    .store
                    .update_task(&task.id, None, None, Some(false), None, None, None, None)
                    .await
                {
                    errors.push(format!("disable failed: {} — restart app to fix", dis_err));
                }
            }
            return Err(errors.join("; "));
        }
    }
    Ok(task)
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub async fn update_task(
    id: String,
    name: Option<String>,
    description: Option<Option<String>>,
    enabled: Option<bool>,
    run_if_missed: Option<bool>,
    notify_on_run: Option<bool>,
    schedule: Option<Schedule>,
    action: Option<Action>,
    state: State<'_, AppState>,
) -> Result<TaskDto, String> {
    // Snapshot old state for rollback
    let old_task = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Task not found".to_string())?;

    // Remove old schedule
    state
        .scheduler
        .remove_task(&id)
        .await
        .map_err(|e| e.to_string())?;

    // Update DB
    let task = match state
        .store
        .update_task(
            &id,
            name,
            description,
            enabled,
            run_if_missed,
            notify_on_run,
            schedule,
            action,
        )
        .await
    {
        Ok(t) => t,
        Err(e) => {
            // DB failed — restore old schedule
            let mut errors = vec![e.to_string()];
            if old_task.enabled {
                if let Err(rb_err) = state.scheduler.schedule_task(&old_task).await {
                    errors.push(format!("rollback re-schedule failed: {}", rb_err));
                }
            }
            return Err(errors.join("; "));
        }
    };

    // Re-schedule with new config
    if task.enabled {
        if let Err(e) = state.scheduler.schedule_task(&task).await {
            let mut errors = vec![e.to_string()];
            // Try to revert DB to old state
            let db_reverted = state
                .store
                .update_task(
                    &id,
                    Some(old_task.name.clone()),
                    Some(old_task.description.clone()),
                    Some(old_task.enabled),
                    Some(old_task.run_if_missed),
                    Some(old_task.notify_on_run),
                    Some(old_task.schedule.clone()),
                    Some(old_task.action.clone()),
                )
                .await;
            match db_reverted {
                Ok(_) => {
                    // DB reverted — try to restore old schedule
                    if old_task.enabled {
                        if let Err(rb_err) = state.scheduler.schedule_task(&old_task).await {
                            errors.push(format!("rollback re-schedule failed: {}", rb_err));
                            // No job in memory — must not stay enabled
                            if let Err(dis_err) = state
                                .store
                                .update_task(&id, None, None, Some(false), None, None, None, None)
                                .await
                            {
                                errors.push(format!(
                                    "disable failed: {} — restart app to fix",
                                    dis_err
                                ));
                            }
                        }
                    }
                }
                Err(rb_err) => {
                    errors.push(format!("rollback failed: {}", rb_err));
                    // No job in memory — must not stay enabled
                    if let Err(dis_err) = state
                        .store
                        .update_task(&id, None, None, Some(false), None, None, None, None)
                        .await
                    {
                        errors.push(format!("disable failed: {} — restart app to fix", dis_err));
                    }
                }
            }
            return Err(errors.join("; "));
        }
    }
    Ok(task)
}

#[tauri::command]
pub async fn delete_task(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .scheduler
        .remove_task(&id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .delete_task(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_task_now(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let task = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Task not found".to_string())?;
    crate::scheduler::execute_and_log(
        &**state.executor,
        &state.store,
        &app,
        &id,
        &task.name,
        task.notify_on_run,
        &task.action,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_logs(
    task_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ExecutionLog>, String> {
    state
        .store
        .list_logs(task_id.as_deref(), limit.unwrap_or(50).min(500))
        .await
        .map_err(|e| e.to_string())
}

fn apps_for_url(url: &NSURL) -> Vec<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app_urls = workspace.URLsForApplicationsToOpenURL(url);
    let mut apps: Vec<String> = Vec::new();

    for app_url in app_urls.to_vec() {
        if let Some(path) = app_url.path() {
            let path_str: String = path.to_string();
            if let Some(name) = path_str.split('/').next_back() {
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

#[tauri::command]
pub fn validate_cron(expression: String) -> Result<(), String> {
    let expr = crate::scheduler::normalize_cron(&expression);
    croner::Cron::new(&expr)
        .parse()
        .map(|_| ())
        .map_err(|e| format!("Invalid cron expression: {}", e))
}

#[tauri::command]
pub fn set_activation_policy(app: AppHandle, policy: String) {
    #[cfg(target_os = "macos")]
    {
        let p = match policy.as_str() {
            "regular" => tauri::ActivationPolicy::Regular,
            _ => tauri::ActivationPolicy::Accessory,
        };
        let _ = app.set_activation_policy(p);
    }
}
