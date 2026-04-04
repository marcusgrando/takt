use std::path::{Path, PathBuf};

pub fn launch_agent_path() -> PathBuf {
    dirs::home_dir()
        .unwrap()
        .join("Library/LaunchAgents/com.cronmac.app.plist")
}

pub fn app_bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Walk up from .app/Contents/MacOS/cronmac to .app
    exe.ancestors()
        .find(|a| a.extension().map_or(false, |e| e == "app"))
        .map(|p| p.to_path_buf())
}

pub fn plist_content(app_path: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.cronmac.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
        app_path.display()
    )
}

pub fn ensure_registered() {
    // Only register if running as a bundled .app
    let Some(app_path) = app_bundle_path() else {
        return;
    };
    let plist_path = launch_agent_path();
    if plist_path.exists() {
        return;
    } // already registered
    if let Some(parent) = plist_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&plist_path, plist_content(&app_path));
    // Load it
    let _ = std::process::Command::new("launchctl")
        .args(["load", plist_path.to_str().unwrap()])
        .status();
}
