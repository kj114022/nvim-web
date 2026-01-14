//! Terminal PTY management using portable-pty.
//!
//! Provides a terminal backend that spawns a PTY process and streams
//! I/O over WebSocket to the browser's xterm.js instance.

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Terminal session managing a PTY process
pub struct TerminalSession {
    /// Sender for input from browser to PTY
    input_tx: mpsc::Sender<Vec<u8>>,
    /// Receiver for output from PTY to browser
    output_rx: Option<mpsc::Receiver<Vec<u8>>>,
}

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalSession")
            .field("has_output_rx", &self.output_rx.is_some())
            .finish_non_exhaustive()
    }
}

impl TerminalSession {
    /// Spawn a new terminal session with the default shell
    pub fn spawn(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = NativePtySystem::default();

        // Determine shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        let cmd = CommandBuilder::new(&shell);
        let _child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;

        // Drop slave - we only need the master
        drop(pair.slave);

        // Get reader and writer from master
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone reader")?;
        let mut writer = pair.master.take_writer().context("Failed to take writer")?;

        // Channels for I/O
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(256);
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);

        // Spawn reader task (PTY -> Browser)
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        // Spawn writer task (Browser -> PTY)
        tokio::spawn(async move {
            while let Some(data) = input_rx.recv().await {
                if writer.write_all(&data).is_err() {
                    break;
                }
                let _ = writer.flush();
            }
        });

        Ok(Self {
            input_tx,
            output_rx: Some(output_rx),
        })
    }

    /// Send input to the PTY (from browser)
    pub async fn send_input(&self, data: Vec<u8>) -> Result<()> {
        self.input_tx
            .send(data)
            .await
            .context("Failed to send input to PTY")
    }

    /// Take the output receiver (to stream to browser)
    /// This can only be called once
    pub fn take_output_rx(&mut self) -> Option<mpsc::Receiver<Vec<u8>>> {
        self.output_rx.take()
    }
}

/// Terminal session manager - maps session IDs to terminal sessions
pub struct TerminalManager {
    sessions: std::collections::HashMap<String, Arc<tokio::sync::Mutex<TerminalSession>>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: std::collections::HashMap::new(),
        }
    }

    /// Create a new terminal session
    pub fn create_session(&mut self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let session = TerminalSession::spawn(cols, rows)?;
        self.sessions.insert(
            session_id.to_string(),
            Arc::new(tokio::sync::Mutex::new(session)),
        );
        tracing::info!("Terminal session created: {}", session_id);
        Ok(())
    }

    /// Get a terminal session by ID
    pub fn get_session(
        &self,
        session_id: &str,
    ) -> Option<Arc<tokio::sync::Mutex<TerminalSession>>> {
        self.sessions.get(session_id).cloned()
    }

    /// Remove a terminal session
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        tracing::info!("Terminal session removed: {}", session_id);
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_terminal_spawn() {
        // Skip on CI where PTY may not work
        if std::env::var("CI").is_ok() {
            return;
        }

        let session = TerminalSession::spawn(80, 24);
        assert!(session.is_ok(), "Should spawn terminal: {:?}", session);
    }
}
