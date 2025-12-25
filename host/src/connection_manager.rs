//! Connection Manager for SSH and remote hosts
//!
//! Manages SSH connections that mount remote filesystems into the VFS layer.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Result};

/// SSH connection configuration
#[derive(Debug, Clone)]
pub struct SSHConnection {
    /// Connection name (user-friendly identifier)
    pub name: String,
    /// SSH URI: user@host or user@host:port
    pub uri: String,
    /// Path to SSH private key (optional, uses default if None)
    pub key_path: Option<PathBuf>,
    /// Mount point in VFS: vfs://ssh/<name>/
    pub mount_point: String,
    /// Connection status
    pub connected: bool,
}

impl SSHConnection {
    /// Create a new SSH connection config
    pub fn new(name: &str, uri: &str) -> Self {
        Self {
            name: name.to_string(),
            uri: uri.to_string(),
            key_path: None,
            mount_point: format!("vfs://ssh/{}/", name),
            connected: false,
        }
    }

    /// Set SSH key path
    pub fn with_key(mut self, key_path: PathBuf) -> Self {
        self.key_path = Some(key_path);
        self
    }

    /// Parse user and host from URI
    pub fn parse_uri(&self) -> Result<(String, String, u16)> {
        let uri = &self.uri;

        // Parse user@host:port format
        let (user_host, port) = if uri.contains(':') {
            let parts: Vec<&str> = uri.rsplitn(2, ':').collect();
            let port_str = parts[0];
            let user_host = parts[1];
            let port: u16 = port_str.parse().unwrap_or(22);
            (user_host.to_string(), port)
        } else {
            (uri.to_string(), 22)
        };

        // Parse user@host
        if let Some(at_pos) = user_host.find('@') {
            let user = user_host[..at_pos].to_string();
            let host = user_host[at_pos + 1..].to_string();
            Ok((user, host, port))
        } else {
            bail!("Invalid SSH URI: expected user@host format")
        }
    }

    /// Convert to MessagePack value for RPC
    pub fn to_value(&self) -> rmpv::Value {
        rmpv::Value::Map(vec![
            (
                rmpv::Value::String("name".into()),
                rmpv::Value::String(self.name.clone().into()),
            ),
            (
                rmpv::Value::String("uri".into()),
                rmpv::Value::String(self.uri.clone().into()),
            ),
            (
                rmpv::Value::String("mount_point".into()),
                rmpv::Value::String(self.mount_point.clone().into()),
            ),
            (
                rmpv::Value::String("connected".into()),
                rmpv::Value::Boolean(self.connected),
            ),
        ])
    }
}

/// Manages SSH and remote connections
#[derive(Default)]
pub struct ConnectionManager {
    /// Active SSH connections
    ssh_connections: HashMap<String, SSHConnection>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            ssh_connections: HashMap::new(),
        }
    }

    /// Add an SSH connection (does not connect yet)
    pub fn add_ssh(&mut self, name: &str, uri: &str) -> Result<&SSHConnection> {
        if self.ssh_connections.contains_key(name) {
            bail!("Connection '{}' already exists", name);
        }

        let conn = SSHConnection::new(name, uri);
        // Validate URI format
        conn.parse_uri()?;

        self.ssh_connections.insert(name.to_string(), conn);
        Ok(self.ssh_connections.get(name).unwrap())
    }

    /// Connect to an SSH host (placeholder - actual connection in VFS)
    pub fn connect_ssh(&mut self, name: &str) -> Result<()> {
        let conn = self
            .ssh_connections
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Connection '{}' not found", name))?;

        // Mark as connected (actual SSH session handled by SshFsBackend)
        conn.connected = true;
        eprintln!("SSH: Marked '{}' as connected (URI: {})", name, conn.uri);
        Ok(())
    }

    /// Disconnect from an SSH host
    pub fn disconnect_ssh(&mut self, name: &str) -> Result<()> {
        let conn = self
            .ssh_connections
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Connection '{}' not found", name))?;

        conn.connected = false;
        eprintln!("SSH: Disconnected '{}'", name);
        Ok(())
    }

    /// Remove an SSH connection
    pub fn remove_ssh(&mut self, name: &str) -> Option<SSHConnection> {
        self.ssh_connections.remove(name)
    }

    /// List all SSH connections
    pub fn list_ssh(&self) -> Vec<&SSHConnection> {
        self.ssh_connections.values().collect()
    }

    /// Get a specific connection
    pub fn get_ssh(&self, name: &str) -> Option<&SSHConnection> {
        self.ssh_connections.get(name)
    }

    /// Check if a connection exists
    pub fn has_ssh(&self, name: &str) -> bool {
        self.ssh_connections.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri_basic() {
        let conn = SSHConnection::new("test", "user@host.com");
        let (user, host, port) = conn.parse_uri().unwrap();
        assert_eq!(user, "user");
        assert_eq!(host, "host.com");
        assert_eq!(port, 22);
    }

    #[test]
    fn test_parse_uri_with_port() {
        let conn = SSHConnection::new("test", "user@host.com:2222");
        let (user, host, port) = conn.parse_uri().unwrap();
        assert_eq!(user, "user");
        assert_eq!(host, "host.com");
        assert_eq!(port, 2222);
    }

    #[test]
    fn test_connection_manager() {
        let mut mgr = ConnectionManager::new();

        // Add connection
        mgr.add_ssh("work", "alice@work.example.com").unwrap();
        assert!(mgr.has_ssh("work"));

        // Connect
        mgr.connect_ssh("work").unwrap();
        assert!(mgr.get_ssh("work").unwrap().connected);

        // Disconnect
        mgr.disconnect_ssh("work").unwrap();
        assert!(!mgr.get_ssh("work").unwrap().connected);

        // Remove
        mgr.remove_ssh("work");
        assert!(!mgr.has_ssh("work"));
    }
}
