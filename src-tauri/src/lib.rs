use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

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
