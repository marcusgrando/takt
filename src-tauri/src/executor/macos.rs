use super::{ActionExecutor, ExecutionResult, ExecutorError};
use crate::models::{Action, HttpMethod, Shell};
use async_trait::async_trait;
use std::process::Command;

pub struct MacosExecutor;

impl MacosExecutor {
    pub fn new() -> Self {
        MacosExecutor
    }
}

#[async_trait]
impl ActionExecutor for MacosExecutor {
    fn platform_name(&self) -> &'static str {
        "macos"
    }

    async fn execute(&self, action: &Action) -> Result<ExecutionResult, ExecutorError> {
        match action {
            Action::OpenFile { path } => open_file(path),
            Action::OpenUrl { url, browser } => open_url(url, browser.as_deref()),
            Action::RunCommand { command, args, shell } => run_command(command, args, shell),
            Action::Notify { title, body, sound } => send_notification(title, body, *sound),
            Action::Shortcut { keys } => send_shortcut(keys),
            Action::Webhook { url, method, headers, body } => {
                send_webhook(url, method, headers, body.as_deref()).await
            }
        }
    }
}

fn open_file(path: &str) -> Result<ExecutionResult, ExecutorError> {
    let output = Command::new("open").arg(path).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

fn open_url(url: &str, browser: Option<&str>) -> Result<ExecutionResult, ExecutorError> {
    let mut cmd = Command::new("open");
    if let Some(b) = browser {
        cmd.arg("-a").arg(b);
    }
    let output = cmd.arg(url).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

fn run_command(
    command: &str,
    args: &[String],
    shell: &Shell,
) -> Result<ExecutionResult, ExecutorError> {
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

    let output = Command::new(shell_bin)
        .arg(shell_flag)
        .arg(&full_command)
        .output()?;

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

fn send_notification(title: &str, body: &str, sound: bool) -> Result<ExecutionResult, ExecutorError> {
    let sound_part = if sound { r#" sound name "default""# } else { "" };
    let script = format!(
        r#"display notification "{}" with title "{}"{}"#,
        body.replace('"', r#"\""#),
        title.replace('"', r#"\""#),
        sound_part
    );
    let output = Command::new("osascript").arg("-e").arg(&script).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

fn send_shortcut(keys: &[String]) -> Result<ExecutionResult, ExecutorError> {
    let keystroke = keys.last().unwrap_or(&String::new()).clone();
    let modifiers: Vec<&str> = keys[..keys.len().saturating_sub(1)]
        .iter()
        .filter_map(|k| match k.as_str() {
            "cmd" | "command" => Some("command down"),
            "shift" => Some("shift down"),
            "opt" | "option" => Some("option down"),
            "ctrl" | "control" => Some("control down"),
            _ => None,
        })
        .collect();

    let using_clause = if modifiers.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", modifiers.join(", "))
    };

    let script = format!(
        r#"tell application "System Events" to keystroke "{}"{}"#,
        keystroke, using_clause
    );

    let output = Command::new("osascript").arg("-e").arg(&script).output()?;
    if output.status.success() {
        Ok(ExecutionResult { stdout: None, stderr: None })
    } else {
        Err(ExecutorError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

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
