use super::{ActionExecutor, ExecutionResult, ExecutorError};
use super::keymap::resolve_keycode;
use crate::models::{Action, HttpMethod, KeyCombo, Modifier, Shell};
use async_trait::async_trait;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::process::Command;
use std::time::Duration;
use tauri_plugin_notification::NotificationExt;

pub struct MacosExecutor {
    app_handle: tauri::AppHandle,
}

impl MacosExecutor {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    fn platform_name(&self) -> &'static str {
        "macos"
    }

    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile { path, app, post_shortcuts } => {
                open_with_command(path, app.as_deref(), false)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::OpenUrl { url, browser, post_shortcuts } => {
                open_with_command(url, browser.as_deref(), false)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::OpenApp { app_path, post_shortcuts } => {
                open_with_command(app_path, None, true)?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts).await?;
                }
                Ok(ExecutionResult { stdout: None, stderr: None })
            }
            Action::RunCommand { command, args, shell } => run_command(command, args, shell),
            Action::Notify { title, body, sound } => {
                send_notification(&self.app_handle, title, body, *sound)
            }
            Action::Webhook { url, method, headers, body } => {
                send_webhook(url, method, headers, body.as_deref()).await
            }
        }
    }
}

// ── Open via macOS `open` command ──────────────────────────────────
// Uses the `open` CLI which is a thin, thread-safe wrapper over Launch
// Services / NSWorkspace. Direct NSWorkspace calls require the main
// thread which is not guaranteed from Tokio async tasks.

fn open_with_command(target: &str, app: Option<&str>, is_app: bool) -> Result<(), ExecutorError> {
    // NSWorkspace calls must run on the main thread.
    // We dispatch synchronously to the main queue to ensure this.
    let target = target.to_string();
    let app = app.map(|s| s.to_string());

    // Use std::process::Command("open") which is thread-safe and invokes
    // the same NSWorkspace machinery under the hood via Launch Services.
    let mut cmd = std::process::Command::new("open");

    if is_app {
        cmd.arg("-a");
        // Extract app name from path: /Applications/Slack.app → Slack
        let app_name = target.split('/').last()
            .unwrap_or(&target)
            .trim_end_matches(".app");
        cmd.arg(app_name);
    } else if let Some(ref app_name) = app {
        cmd.arg("-a").arg(app_name).arg(&target);
    } else {
        cmd.arg(&target);
    }

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    Ok(())
}

// ── Post-shortcuts: wait + CGEvent ────────────────────────────────────

async fn wait_and_send_shortcuts(shortcuts: &[KeyCombo]) -> Result<(), ExecutorError> {
    if !accessibility_is_trusted() {
        return Err(ExecutorError::AccessibilityRequired(
            "Grant Accessibility permission in System Settings → Privacy & Security → Accessibility".into()
        ));
    }

    // Wait for the target app to become frontmost
    // NSWorkspace open is typically fast — app is frontmost within ~500ms
    tokio::time::sleep(Duration::from_millis(800)).await;

    for combo in shortcuts {
        send_key_combo(combo)?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

fn send_key_combo(combo: &KeyCombo) -> Result<(), ExecutorError> {
    let keycode = resolve_keycode(&combo.key)
        .ok_or_else(|| ExecutorError::CommandFailed(format!("Unknown key: {}", combo.key)))?;

    let mut flags = CGEventFlags::CGEventFlagNull;
    for modifier in &combo.modifiers {
        flags |= match modifier {
            Modifier::Cmd => CGEventFlags::CGEventFlagCommand,
            Modifier::Shift => CGEventFlags::CGEventFlagShift,
            Modifier::Opt => CGEventFlags::CGEventFlagAlternate,
            Modifier::Ctrl => CGEventFlags::CGEventFlagControl,
        };
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create CGEventSource".into()))?;

    // Key down
    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode as CGKeyCode, true)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-down event".into()))?;
    key_down.set_flags(flags);
    key_down.post(CGEventTapLocation::HID);

    // Key up
    let key_up = CGEvent::new_keyboard_event(source, keycode as CGKeyCode, false)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-up event".into()))?;
    key_up.set_flags(flags);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

fn accessibility_is_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

// ── RunCommand (unchanged — std::process::Command) ───────────────────

fn run_command(command: &str, args: &[String], shell: &Shell) -> Result<ExecutionResult, ExecutorError> {
    let full_command = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    let (shell_bin, shell_flag) = match shell {
        Shell::Sh => ("/bin/sh", "-c"),
        Shell::Bash => ("/bin/bash", "-c"),
        Shell::Zsh => ("/bin/zsh", "-c"),
        Shell::Python => ("/usr/bin/python3", "-c"),
        Shell::AppleScript => ("/usr/bin/osascript", "-e"),
    };

    let output = Command::new(shell_bin).arg(shell_flag).arg(&full_command).output()?;
    let stdout = if output.stdout.is_empty() { None } else { Some(String::from_utf8_lossy(&output.stdout).to_string()) };
    let stderr = if output.stderr.is_empty() { None } else { Some(String::from_utf8_lossy(&output.stderr).to_string()) };

    if output.status.success() {
        Ok(ExecutionResult { stdout, stderr })
    } else {
        Err(ExecutorError::CommandFailed(
            format!("Exit code {}: {}", output.status.code().unwrap_or(-1), stderr.as_deref().unwrap_or(""))
        ))
    }
}

// ── Notification via tauri-plugin-notification ────────────────────────

fn send_notification(app: &tauri::AppHandle, title: &str, body: &str, sound: bool) -> Result<ExecutionResult, ExecutorError> {
    let mut builder = app.notification().builder()
        .title(title)
        .body(body);
    if sound {
        builder = builder.sound("default");
    }
    builder.show()
        .map_err(|e| ExecutorError::CommandFailed(format!("Notification failed: {}", e)))?;
    Ok(ExecutionResult { stdout: None, stderr: None })
}

// ── Webhook (unchanged — reqwest) ────────────────────────────────────

async fn send_webhook(
    url: &str,
    method: &HttpMethod,
    headers: &std::collections::HashMap<String, String>,
    body: Option<&str>,
) -> Result<ExecutionResult, ExecutorError> {
    let client = reqwest::Client::new();
    let mut req = match method {
        HttpMethod::GET => client.get(url),
        HttpMethod::POST => client.post(url),
        HttpMethod::PUT => client.put(url),
        HttpMethod::PATCH => client.patch(url),
        HttpMethod::DELETE => client.delete(url),
    };
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.body(b.to_string());
    }
    let response = req.send().await.map_err(|e| ExecutorError::Http(e.to_string()))?;
    let status = response.status();
    let response_body = response.text().await.map_err(|e| ExecutorError::Http(e.to_string()))?;

    if status.is_success() {
        Ok(ExecutionResult { stdout: Some(response_body), stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(format!("HTTP {} — {}", status, response_body)))
    }
}
