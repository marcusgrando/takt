use super::keymap::resolve_keycode;
use super::{ActionExecutor, ExecutionResult, ExecutorError};
use crate::models::{Action, HttpMethod, KeyCombo, Modifier, Shell};
use crate::platform::{self, PlatformBridge};
use async_trait::async_trait;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSString, NSURL};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

pub struct MacosExecutor {
    bridge: Arc<dyn PlatformBridge>,
}

impl MacosExecutor {
    pub fn new(bridge: Arc<dyn PlatformBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile {
                path,
                app,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let path = path.clone();
                let app = app.clone();
                platform::run_on_main(&*self.bridge, move || open_file(&path, app.as_deref()))
                    .await?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::OpenUrl {
                url,
                browser,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let url = url.clone();
                let browser = browser.clone();
                platform::run_on_main(&*self.bridge, move || open_url(&url, browser.as_deref()))
                    .await?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::OpenApp {
                app_path,
                post_shortcuts,
                shortcut_delay_secs,
            } => {
                let app_path = app_path.clone();
                platform::run_on_main(&*self.bridge, move || open_app(&app_path)).await?;
                if !post_shortcuts.is_empty() {
                    wait_and_send_shortcuts(post_shortcuts, *shortcut_delay_secs).await?;
                }
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::Settings { pane_url } => {
                let pane_url = pane_url.clone();
                platform::run_on_main(&*self.bridge, move || open_url(&pane_url, None)).await?;
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::RunCommand {
                command,
                args,
                shell,
            } => run_command(command, args, shell),
            Action::Notify { title, body, sound } => {
                self.bridge
                    .send_notification(title.clone(), body.clone(), *sound);
                Ok(ExecutionResult {
                    stdout: None,
                    stderr: None,
                })
            }
            Action::Webhook {
                url,
                method,
                headers,
                body,
            } => send_webhook(url, method, headers, body.as_deref()).await,
        }
    }
}

// ── Open via NSWorkspace (main thread) ────────────────────────────────

fn open_file(path: &str, app: Option<&str>) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let file_url = NSURL::fileURLWithPath(&NSString::from_str(path));

    if let Some(app_name) = app {
        let config = NSWorkspaceOpenConfiguration::configuration();
        let app_url = resolve_app_url(app_name);
        let urls = NSArray::from_retained_slice(&[file_url]);
        workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &urls, &app_url, &config, None,
        );
    } else {
        let opened = workspace.openURL(&file_url);
        if !opened {
            return Err(ExecutorError::CommandFailed(format!(
                "Failed to open file: {}",
                path
            )));
        }
    }
    Ok(())
}

fn open_url(url: &str, browser: Option<&str>) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    // Normalize: add https:// only if no scheme is present at all.
    // Schemes like x-apple.systempreferences: use ":" without "://".
    let has_scheme = url.contains("://")
        || url.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '+')
        });
    let normalized = if has_scheme {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    let ns_url = NSURL::URLWithString(&NSString::from_str(&normalized))
        .ok_or_else(|| ExecutorError::CommandFailed(format!("Invalid URL: {}", url)))?;

    if let Some(browser_name) = browser {
        let config = NSWorkspaceOpenConfiguration::configuration();
        let app_url = resolve_app_url(browser_name);
        let urls = NSArray::from_retained_slice(&[ns_url]);
        workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &urls, &app_url, &config, None,
        );
    } else {
        let opened = workspace.openURL(&ns_url);
        if !opened {
            return Err(ExecutorError::CommandFailed(format!(
                "Failed to open URL: {}",
                url
            )));
        }
    }
    Ok(())
}

fn open_app(app_path: &str) -> Result<(), ExecutorError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app_url = NSURL::fileURLWithPath(&NSString::from_str(app_path));
    let config = NSWorkspaceOpenConfiguration::configuration();
    workspace.openApplicationAtURL_configuration_completionHandler(&app_url, &config, None);
    Ok(())
}

fn resolve_app_url(app_name: &str) -> objc2::rc::Retained<NSURL> {
    let path = if app_name.ends_with(".app") || app_name.starts_with('/') {
        app_name.to_string()
    } else {
        format!("/Applications/{}.app", app_name)
    };
    NSURL::fileURLWithPath(&NSString::from_str(&path))
}

// ── Post-shortcuts: wait + CGEvent ────────────────────────────────────

async fn wait_and_send_shortcuts(
    shortcuts: &[KeyCombo],
    delay_secs: u64,
) -> Result<(), ExecutorError> {
    if !accessibility_is_trusted() {
        return Err(ExecutorError::AccessibilityRequired(
            "Grant Accessibility permission in System Settings → Privacy & Security → Accessibility"
                .into(),
        ));
    }

    let delay = Duration::from_secs(delay_secs);

    // Wait after the action (app/page load), then between each shortcut
    for combo in shortcuts {
        tokio::time::sleep(delay).await;
        send_key_combo(combo)?;
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

    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode as CGKeyCode, true)
        .map_err(|_| ExecutorError::CommandFailed("Failed to create key-down event".into()))?;
    key_down.set_flags(flags);
    key_down.post(CGEventTapLocation::HID);

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

// ── RunCommand (std::process::Command — no native API advantage) ──────

fn run_command(
    command: &str,
    args: &[String],
    shell: &Shell,
) -> Result<ExecutionResult, ExecutorError> {
    let (shell_bin, shell_flag) = match shell {
        Shell::Sh => ("/bin/sh", "-c"),
        Shell::Bash => ("/bin/bash", "-c"),
        Shell::Zsh => ("/bin/zsh", "-c"),
        Shell::Python => ("/usr/bin/python3", "-c"),
        Shell::AppleScript => ("/usr/bin/osascript", "-e"),
    };

    let mut cmd = Command::new(shell_bin);
    match shell {
        // POSIX shells: concat command + args into a single -c string.
        // Args are NOT quoted — they're part of the shell command, so the user
        // controls quoting and can use globs, variables, substitutions, etc.
        Shell::Sh | Shell::Bash | Shell::Zsh => {
            let full = if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            cmd.arg(shell_flag).arg(&full);
        }
        // Python: script via -c, then args as separate argv entries
        Shell::Python => {
            cmd.arg(shell_flag).arg(command);
            for arg in args {
                cmd.arg(arg);
            }
        }
        // AppleScript: script via -e, then args after --
        Shell::AppleScript => {
            cmd.arg(shell_flag).arg(command);
            if !args.is_empty() {
                cmd.arg("--");
                for arg in args {
                    cmd.arg(arg);
                }
            }
        }
    }

    let output = cmd.output()?;
    let stdout = if output.stdout.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    };
    let stderr = if output.stderr.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&output.stderr).to_string())
    };

    if output.status.success() {
        Ok(ExecutionResult { stdout, stderr })
    } else {
        Err(ExecutorError::CommandFailed(format!(
            "Exit code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.as_deref().unwrap_or("")
        )))
    }
}

// ── Webhook (reqwest — native Rust HTTP) ──────────────────────────────

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
    let response = req
        .send()
        .await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;
    let status = response.status();
    let response_body = response
        .text()
        .await
        .map_err(|e| ExecutorError::Http(e.to_string()))?;

    if status.is_success() {
        Ok(ExecutionResult {
            stdout: Some(response_body),
            stderr: None,
        })
    } else {
        Err(ExecutorError::CommandFailed(format!(
            "HTTP {} — {}",
            status, response_body
        )))
    }
}
