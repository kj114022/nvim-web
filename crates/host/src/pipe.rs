//! Universal Tool Pipe
//!
//! Generic CLI spawner for user-configurable tools (LLMs, formatters, etc).
//! Replaces hardcoded LLM providers with a flexible pipe mechanism.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Configuration for a tool invocation
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// Command to execute (e.g., "claude", "gemini-cli", "prettier")
    pub command: String,
    /// Arguments to pass
    pub args: Vec<String>,
    /// Working directory (optional)
    pub cwd: Option<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
}

/// Result of a tool execution
#[derive(Debug)]
pub struct ToolResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run a tool with input, returning full output
pub async fn run_pipe(
    command: &str,
    args: &[String],
    input: &str,
    cwd: Option<&str>,
) -> Result<ToolResult> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = cmd.spawn().context("Failed to spawn tool process")?;

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .context("Failed to write to tool stdin")?;
    }

    // Wait for completion and capture output
    let output = child
        .wait_with_output()
        .await
        .context("Failed to wait for tool")?;

    Ok(ToolResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run a tool with streaming output
pub async fn run_pipe_streaming(
    command: &str,
    args: &[String],
    input: &str,
    output_tx: mpsc::Sender<String>,
) -> Result<i32> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn tool process")?;

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .context("Failed to write to tool stdin")?;
    }

    // Stream stdout
    if let Some(mut stdout) = child.stdout.take() {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Wait for completion
    let status = child.wait().await.context("Failed to wait for tool")?;
    Ok(status.code().unwrap_or(-1))
}

/// Validate that a command exists and is executable
pub async fn validate_tool(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_pipe_echo() {
        let result = run_pipe("echo", &["hello".to_string()], "", None).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.stdout.contains("hello"));
        assert_eq!(res.exit_code, 0);
    }

    #[tokio::test]
    async fn test_run_pipe_with_input() {
        let result = run_pipe("cat", &[], "test input", None).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.stdout.trim(), "test input");
    }

    #[tokio::test]
    async fn test_validate_tool() {
        assert!(validate_tool("echo").await);
        assert!(!validate_tool("nonexistent_command_xyz").await);
    }
}
