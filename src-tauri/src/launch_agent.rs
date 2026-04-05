use std::path::{Path, PathBuf};

pub fn launch_agent_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Library/LaunchAgents/com.marcusgrando.takt.plist"))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn app_bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Walk up from .app/Contents/MacOS/takt to .app
    exe.ancestors()
        .find(|a| a.extension().is_some_and(|e| e == "app"))
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
    <string>com.marcusgrando.takt</string>
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
        xml_escape(&app_path.display().to_string())
    )
}

pub fn ensure_registered() {
    // Only register if running as a bundled .app
    let Some(app_path) = app_bundle_path() else {
        return;
    };
    let Some(plist_path) = launch_agent_path() else {
        return;
    };
    if plist_path.exists() {
        return;
    } // already registered
    if let Some(parent) = plist_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&plist_path, plist_content(&app_path)).is_ok() {
        let _ = std::process::Command::new("launchctl")
            .args(["load", plist_path.to_str().unwrap_or_default()])
            .status();
    }
}
