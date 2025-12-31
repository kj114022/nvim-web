//! Configuration system for nvim-web
//!
//! Reads config from ~/.config/nvim-web/config.toml

use std::path::PathBuf;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub ws_port: u16,
    pub http_port: u16,
    pub bind: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            ws_port: 9001,
            http_port: 8080,
            bind: "127.0.0.1".to_string(),
        }
    }
}

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub timeout_secs: u64,
    pub max_sessions: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300,
            max_sessions: 10,
        }
    }
}

/// Rate limiting configuration for WebSocket connections
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum burst size (tokens)
    pub max_burst: u64,
    /// Refill rate (tokens per second)
    pub refill_rate: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_burst: 1000,
            refill_rate: 100,
        }
    }
}

/// SSH tunnel configuration for port forwarding
#[derive(Debug, Clone)]
pub struct SshTunnel {
    pub host: String,
    pub port: u16,
    pub local_port: u16,
    pub remote_port: u16,
    pub user: Option<String>,
}

/// Saved connection configuration
#[derive(Debug, Clone)]
pub struct Connection {
    pub name: String,
    pub url: String,
    pub ssh_tunnel: Option<SshTunnel>,
}

/// Full application configuration
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub session: SessionConfig,
    pub rate_limit: RateLimitConfig,
    pub connections: Vec<Connection>,
}

impl Config {
    /// Load configuration from default path
    pub fn load() -> Self {
        let config_path = Self::default_config_path();
        Self::load_from_path(&config_path).unwrap_or_default()
    }

    /// Get default config path
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nvim-web")
            .join("config.toml")
    }

    /// Load from specific path (simple key=value parsing with connection support)
    pub fn load_from_path(path: &PathBuf) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;

        let mut config = Self::default();
        let mut current_connection: Option<Connection> = None;
        let mut in_connections = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle section headers
            if line == "[[connections]]" {
                // Save previous connection if any
                if let Some(conn) = current_connection.take() {
                    config.connections.push(conn);
                }
                current_connection = Some(Connection {
                    name: String::new(),
                    url: String::new(),
                    ssh_tunnel: None,
                });
                in_connections = true;
                continue;
            }

            if line.starts_with('[') && !line.starts_with("[[") {
                // Save previous connection if any
                if let Some(conn) = current_connection.take() {
                    if !conn.name.is_empty() {
                        config.connections.push(conn);
                    }
                }
                in_connections = false;
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                if in_connections {
                    if let Some(ref mut conn) = current_connection {
                        match key {
                            "name" => conn.name = value.to_string(),
                            "url" => conn.url = value.to_string(),
                            "ssh_tunnel" => {
                                conn.ssh_tunnel = Self::parse_ssh_tunnel(value);
                            }
                            _ => {}
                        }
                    }
                } else {
                    match key {
                        "ws_port" => {
                            if let Ok(port) = value.parse() {
                                config.server.ws_port = port;
                            }
                        }
                        "http_port" => {
                            if let Ok(port) = value.parse() {
                                config.server.http_port = port;
                            }
                        }
                        "bind" => {
                            config.server.bind = value.to_string();
                        }
                        "timeout" => {
                            if let Ok(secs) = value.parse() {
                                config.session.timeout_secs = secs;
                            }
                        }
                        "max_sessions" => {
                            if let Ok(max) = value.parse() {
                                config.session.max_sessions = max;
                            }
                        }
                        "max_burst" => {
                            if let Ok(burst) = value.parse() {
                                config.rate_limit.max_burst = burst;
                            }
                        }
                        "refill_rate" => {
                            if let Ok(rate) = value.parse() {
                                config.rate_limit.refill_rate = rate;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Save final connection if any
        if let Some(conn) = current_connection {
            if !conn.name.is_empty() {
                config.connections.push(conn);
            }
        }

        Some(config)
    }

    /// Parse SSH tunnel inline table { host = "...", port = 22, ... }
    fn parse_ssh_tunnel(value: &str) -> Option<SshTunnel> {
        let value = value.trim();
        if !value.starts_with('{') || !value.ends_with('}') {
            return None;
        }

        let inner = &value[1..value.len() - 1];
        let mut tunnel = SshTunnel {
            host: String::new(),
            port: 22,
            local_port: 0,
            remote_port: 0,
            user: None,
        };

        for part in inner.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                let k = k.trim();
                let v = v.trim().trim_matches('"');
                match k {
                    "host" => tunnel.host = v.to_string(),
                    "port" => tunnel.port = v.parse().unwrap_or(22),
                    "local_port" => tunnel.local_port = v.parse().unwrap_or(0),
                    "remote_port" => tunnel.remote_port = v.parse().unwrap_or(0),
                    "user" => tunnel.user = Some(v.to_string()),
                    _ => {}
                }
            }
        }

        if tunnel.host.is_empty() || tunnel.local_port == 0 || tunnel.remote_port == 0 {
            return None;
        }

        Some(tunnel)
    }

    /// Get connection by name
    pub fn get_connection(&self, name: &str) -> Option<&Connection> {
        self.connections.iter().find(|c| c.name == name)
    }

    /// Create default config file if it doesn't exist
    pub fn create_default_if_missing() {
        let path = Self::default_config_path();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let default_config = r#"# nvim-web Configuration

[server]
ws_port = 9001
http_port = 8080
bind = "127.0.0.1"

[session]
timeout = 300
max_sessions = 10

# Example saved connections
# [[connections]]
# name = "local"
# url = "ws://127.0.0.1:9001"

# Example with SSH tunnel
# [[connections]]
# name = "remote"
# url = "ws://127.0.0.1:9002"
# ssh_tunnel = { host = "remote.example.com", port = 22, local_port = 9002, remote_port = 9001 }
"#;
            let _ = std::fs::write(&path, default_config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.ws_port, 9001);
        assert_eq!(config.server.bind, "127.0.0.1");
        assert_eq!(config.session.timeout_secs, 300);
    }

    #[test]
    fn test_parse_ssh_tunnel() {
        let tunnel_str = r#"{ host = "example.com", port = 22, local_port = 9002, remote_port = 9001 }"#;
        let tunnel = Config::parse_ssh_tunnel(tunnel_str).unwrap();
        assert_eq!(tunnel.host, "example.com");
        assert_eq!(tunnel.port, 22);
        assert_eq!(tunnel.local_port, 9002);
        assert_eq!(tunnel.remote_port, 9001);
    }
}
