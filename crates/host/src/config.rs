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

/// Full application configuration
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub session: SessionConfig,
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

    /// Load from specific path (simple key=value parsing)
    pub fn load_from_path(path: &PathBuf) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;

        let mut config = Self::default();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

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
                    _ => {}
                }
            }
        }

        Some(config)
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
}
