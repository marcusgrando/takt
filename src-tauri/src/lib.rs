mod db;
mod executor;
mod models;
mod scheduler;
mod store;

use std::sync::Arc;

use crate::scheduler::AppScheduler;
use crate::store::TaskStore;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub struct AppState {
    pub store: Arc<TaskStore>,
    pub scheduler: Arc<AppScheduler>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Hide from dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Create the main popover window programmatically
            let _window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title("cronmac")
                    .inner_size(280.0, 400.0)
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .visible(false)
                    .skip_taskbar(true)
                    .build()?;

            setup_tray(app.handle())?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let pool = crate::db::connect()
                    .await
                    .expect("DB connect failed");
                let store = Arc::new(TaskStore::new(pool));
                let scheduler = Arc::new(
                    AppScheduler::new(Arc::clone(&store))
                        .await
                        .expect("Scheduler init failed"),
                );
                scheduler.start().await.expect("Scheduler start failed");
                scheduler
                    .load_all_tasks()
                    .await
                    .expect("Load tasks failed");
                handle.manage(AppState { store, scheduler });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit cronmac", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::new()
        .icon(tauri::include_image!("icons/tray-icon.png"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: TrayIconEvent| {
            if let TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle();
                toggle_main_window(app);
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

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
