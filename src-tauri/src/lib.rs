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
    // Kill any previously running instance before starting
    #[cfg(target_os = "macos")]
    kill_previous_instance();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Hide from dock — must be set before any window is shown
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Build the popover window — transparent for rounded CSS corners
            let window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title("Takt")
                    .inner_size(280.0, 400.0)
                    .resizable(false)
                    .decorations(false)
                    .shadow(true)
                    .transparent(true)
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

            // Request Accessibility permission (shows macOS prompt if not granted)
            request_accessibility_permission();

            // Request Notification permission
            {
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification().request_permission();
            }

            // Async init — block until AppState is ready before IPC is available
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = crate::db::connect().await.expect("DB connect failed");
                let store = Arc::new(TaskStore::new(pool));
                let executor = Arc::new(current_executor(handle.clone()));
                let scheduler = Arc::new(
                    AppScheduler::new(Arc::clone(&store), Arc::clone(&executor), handle.clone())
                        .await
                        .expect("Scheduler init failed"),
                );
                scheduler.start().await.expect("Scheduler start failed");
                if let Err(e) = scheduler.load_all_tasks().await {
                    eprintln!("Warning: failed to load tasks: {}", e);
                }
                handle.manage(AppState {
                    store,
                    scheduler,
                    executor,
                });
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
            commands::list_apps_for_file,
            commands::validate_cron,
            commands::set_activation_policy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // When macOS sends Reopen (user did `open takt.app` while running),
            // relaunch as a new instance so kill_previous_instance can replace us.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                relaunch_and_exit(app);
            }
        });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
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

/// Relaunch the app as a new instance and exit the current one.
/// `open -n -a` forces macOS to start a fresh process even if one is running.
/// The new process will call kill_previous_instance() to clean up.
#[cfg(target_os = "macos")]
fn relaunch_and_exit(app: &AppHandle) {
    use objc2_foundation::NSBundle;

    let bundle_path = NSBundle::mainBundle().bundlePath();
    if std::process::Command::new("open")
        .args(["-n", "-a"])
        .arg(bundle_path.to_string())
        .spawn()
        .is_ok()
    {
        app.exit(0);
        std::process::exit(0);
    }
}

/// Kill any other running instance of this app so only one is active.
/// Uses NSRunningApplication to find processes with the same bundle identifier.
#[cfg(target_os = "macos")]
fn kill_previous_instance() {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSBundle;

    let Some(bundle_id) = NSBundle::mainBundle().bundleIdentifier() else {
        return; // no bundle id (dev mode) — skip
    };

    let my_pid = std::process::id() as i32;
    let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_id);
    for app in running.to_vec() {
        let pid = app.processIdentifier();
        if pid != my_pid && pid > 0 {
            app.forceTerminate();
        }
    }
}

/// Check Accessibility permission and prompt the user if not granted.
/// Uses AXIsProcessTrustedWithOptions with kAXTrustedCheckOptionPrompt=true
/// which shows the native macOS "allow Accessibility" dialog.
#[cfg(target_os = "macos")]
fn request_accessibility_permission() {
    use std::ffi::c_void;

    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);

        static kAXTrustedCheckOptionPrompt: *const c_void;
        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
    }

    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const c_void,
        );
        let _trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
    }
}

#[cfg(not(target_os = "macos"))]
fn request_accessibility_permission() {}
