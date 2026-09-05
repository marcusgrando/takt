use super::ExecutorError;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

pub(super) const OUTPUT_LIMIT: usize = 1024 * 1024;

struct ProcessGroup(i32);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // The child leads a separate process group, including ordinary descendants.
        unsafe { kill(-self.0, 9) };
    }
}

async fn read_output(mut stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, ExecutorError> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(output);
        }
        if output.len() + read > OUTPUT_LIMIT {
            return Err(ExecutorError::CommandFailed(
                "Command output exceeded 1 MiB per stream".into(),
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

pub(super) async fn run(command: &mut Command, timeout: Duration) -> Result<Output, ExecutorError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);
    let mut child = command.spawn()?;
    let group = ProcessGroup(child.id().expect("spawned child has a pid") as i32);
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let result = tokio::time::timeout(timeout, async {
        let (status, stdout, stderr) = tokio::try_join!(
            async { child.wait().await.map_err(ExecutorError::from) },
            read_output(stdout),
            read_output(stderr)
        )?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    drop(group);
    if !matches!(result, Ok(Ok(_))) {
        let _ = child.wait().await;
    }
    result.map_err(|_| {
        ExecutorError::CommandFailed(format!(
            "Command timed out after {} seconds",
            timeout.as_secs()
        ))
    })?
}

use std::os::unix::process::CommandExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_kills_background_descendants() {
        let marker = std::env::temp_dir().join(format!("takt-timeout-{}", uuid::Uuid::new_v4()));
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("(sleep 0.3; printf escaped > \"$1\") & wait")
            .arg("takt")
            .arg(&marker);
        let result = run(&mut command, Duration::from_millis(60)).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let escaped = marker.exists();
        let _ = std::fs::remove_file(marker);
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(!escaped, "background child survived timeout");
    }

    #[tokio::test]
    async fn cancellation_kills_background_descendants() {
        let marker = std::env::temp_dir().join(format!("takt-cancel-{}", uuid::Uuid::new_v4()));
        let path = marker.clone();
        let ready = marker.with_extension("ready");
        let ready_path = ready.clone();
        let handle = tokio::spawn(async move {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg("(printf ready > \"$2\"; sleep 0.3; printf escaped > \"$1\") & wait")
                .arg("takt")
                .arg(path)
                .arg(ready_path);
            run(&mut command, Duration::from_secs(10)).await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("background child did not start");
        handle.abort();
        let _ = handle.await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let escaped = marker.exists();
        let _ = std::fs::remove_file(marker);
        let _ = std::fs::remove_file(ready);
        assert!(!escaped, "background child survived cancellation");
    }
}
