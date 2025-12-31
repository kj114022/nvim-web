//! SSH tunnel management for remote connections
//!
//! Uses system `ssh` command for reliable tunnel establishment

use std::process::{Child, Command, Stdio};

use crate::config::SshTunnel;

/// Active SSH tunnel with background process
pub struct ActiveTunnel {
    pub config: SshTunnel,
    process: Child,
}

impl ActiveTunnel {
    /// Establish a new SSH tunnel
    pub fn establish(config: &SshTunnel) -> Result<Self, String> {
        let user_host = match &config.user {
            Some(user) => format!("{}@{}", user, config.host),
            None => config.host.clone(),
        };

        // Build ssh command: ssh -N -L local_port:localhost:remote_port user@host -p port
        let mut cmd = Command::new("ssh");
        cmd.arg("-N") // No remote command
            .arg("-L")
            .arg(format!(
                "{}:localhost:{}",
                config.local_port, config.remote_port
            ))
            .arg(&user_host)
            .arg("-p")
            .arg(config.port.to_string())
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("BatchMode=yes") // Non-interactive
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let process = cmd
            .spawn()
            .map_err(|e| format!("Failed to start SSH tunnel: {e}"))?;

        // Give tunnel a moment to establish
        std::thread::sleep(std::time::Duration::from_millis(500));

        Ok(Self {
            config: config.clone(),
            process,
        })
    }

    /// Check if tunnel is still running
    pub fn is_alive(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,  // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,
        }
    }

    /// Get local port for connection
    pub const fn local_port(&self) -> u16 {
        self.config.local_port
    }

    /// Stop the tunnel
    pub fn stop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Tunnel manager for multiple connections
#[derive(Default)]
pub struct TunnelManager {
    tunnels: Vec<ActiveTunnel>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self { tunnels: Vec::new() }
    }

    /// Establish a tunnel and return the local port
    pub fn connect(&mut self, config: &SshTunnel) -> Result<u16, String> {
        // Check if tunnel already exists
        for tunnel in &self.tunnels {
            if tunnel.config.host == config.host
                && tunnel.config.remote_port == config.remote_port
            {
                return Ok(tunnel.config.local_port);
            }
        }

        let tunnel = ActiveTunnel::establish(config)?;
        let port = tunnel.local_port();
        self.tunnels.push(tunnel);
        Ok(port)
    }

    /// Disconnect a specific tunnel by local port
    pub fn disconnect(&mut self, local_port: u16) {
        self.tunnels.retain(|t| t.config.local_port != local_port);
    }

    /// Cleanup dead tunnels
    pub fn cleanup(&mut self) {
        self.tunnels.retain_mut(|t| t.is_alive());
    }

    /// List active tunnels
    pub fn list(&self) -> Vec<&SshTunnel> {
        self.tunnels.iter().map(|t| &t.config).collect()
    }
}
