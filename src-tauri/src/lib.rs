mod commands;
mod db;
mod executor;
mod launch_agent;
mod models;
mod scheduler;
mod store;

use std::sync::Arc;

use crate::executor::{current_executor, ActionExecutor};
use crate::scheduler::AppScheduler;
use crate::store::TaskStore;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub struct AppState {
    pub store: Arc<TaskStore>,
    pub scheduler: Arc<AppScheduler>,
    pub executor: Arc<Box<dyn ActionExecutor>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Hide from dock — must be set before any window is shown
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Build the popover window — opaque white, no decorations
            let window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title("cronmac")
                    .inner_size(280.0, 400.0)
                    .resizable(false)
                    .decorations(false)
                    .shadow(true)
                    .always_on_top(true)
                    .visible(false)
                    .skip_taskbar(true)
                    .build()?;

            // Auto-dismiss: hide the popover when it loses focus (click outside)
            let window_focus = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window_focus.hide();
                }
            });

            // Register LaunchAgent for auto-start on login (bundled .app only)
            launch_agent::ensure_registered();

            // Async init — block until AppState is ready before IPC is available
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = crate::db::connect()
                    .await
                    .expect("DB connect failed");
                let store = Arc::new(TaskStore::new(pool));
                let executor = Arc::new(current_executor(handle.clone()));
                let scheduler = Arc::new(
                    AppScheduler::new(Arc::clone(&store), handle.clone())
                        .await
                        .expect("Scheduler init failed"),
                );
                scheduler.start().await.expect("Scheduler start failed");
                scheduler
                    .load_all_tasks()
                    .await
                    .expect("Load tasks failed");
                handle.manage(AppState { store, scheduler, executor });
            });

            setup_tray(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::get_task,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::run_task_now,
            commands::list_logs,
            commands::list_browsers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit cronmac", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::new()
        .icon(tauri::include_image!("icons/tray-icon.png"))
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            // Only react to left-button release to avoid double-firing (down + up)
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                let (tray_x, tray_y) = match rect.position {
                    tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
                    tauri::Position::Logical(p) => (p.x, p.y),
                };
                let (tray_w, tray_h) = match rect.size {
                    tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
                    tauri::Size::Logical(s) => (s.width, s.height),
                };
                show_near_tray(app, tray_x, tray_y, tray_w, tray_h);
            }
        })
        .on_menu_event(|app: &AppHandle, event: tauri::menu::MenuEvent| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

/// Position the popover window centered below the tray icon and show it.
/// All coordinates from TrayIconEvent are in physical pixels.
fn show_near_tray(app: &AppHandle, tray_x: f64, tray_y: f64, tray_w: f64, tray_h: f64) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // Toggle: if already visible, hide it
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    let scale = window.scale_factor().unwrap_or(2.0);
    let win_w = 280.0 * scale; // window width in physical pixels

    // Center horizontally on the tray icon, place just below it
    let x = tray_x + (tray_w / 2.0) - (win_w / 2.0);
    let y = tray_y + tray_h + 4.0 * scale;

    let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
    let _ = window.show();
    let _ = window.set_focus();
}
