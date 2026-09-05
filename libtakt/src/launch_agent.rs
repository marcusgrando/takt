use std::io::Write;
use std::path::{Path, PathBuf};

pub fn launch_agent_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Library/LaunchAgents/app.takt.plist"))
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
    <string>app.takt</string>
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
        xml_escape(&app_path.join("Contents/MacOS/Takt").display().to_string())
    )
}

pub fn ensure_registered() {
    // launchd reads this registration at login without launching another instance now.
    let Some(app_path) = app_bundle_path() else {
        return;
    };
    let Some(plist_path) = launch_agent_path() else {
        return;
    };
    if let Err(error) = write_registration(&plist_path, &app_path) {
        eprintln!("[takt] failed to register launch at login: {}", error);
    }
}

fn write_registration(plist_path: &Path, app_path: &Path) -> std::io::Result<bool> {
    let contents = plist_content(app_path);
    if std::fs::read_to_string(plist_path).ok().as_deref() == Some(&contents) {
        return Ok(false);
    }
    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary_path = plist_path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary_path, plist_path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_launches_bundle_executable() {
        let plist = plist_content(Path::new("/Applications/Takt.app"));
        assert!(plist.contains("<string>/Applications/Takt.app/Contents/MacOS/Takt</string>"));
    }

    #[test]
    fn registration_escapes_executable_path() {
        let plist = plist_content(Path::new("/Applications/Work & Tools/Takt.app"));
        assert!(plist.contains(
            "<string>/Applications/Work &amp; Tools/Takt.app/Contents/MacOS/Takt</string>"
        ));
    }

    #[test]
    fn registration_replaces_stale_bundle_and_preserves_current_file() {
        let dir = std::env::temp_dir().join(format!("takt-launch-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let plist = dir.join("app.takt.plist");
        std::fs::write(&plist, plist_content(Path::new("/old/Takt.app"))).unwrap();
        let updated = write_registration(&plist, Path::new("/Applications/Takt.app")).unwrap();
        let contents = std::fs::read_to_string(&plist).unwrap();
        let unchanged = write_registration(&plist, Path::new("/Applications/Takt.app")).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(updated);
        assert!(!unchanged);
        assert!(contents.contains("/Applications/Takt.app/Contents/MacOS/Takt"));
    }
}
