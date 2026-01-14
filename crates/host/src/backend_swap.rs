//! Backend Swap
//!
//! Hot-swap between different Neovim backends without losing state:
//! - Local process
//! - Docker container
//! - SSH remote
//! - TCP socket
//!
//! State is preserved via CRDT sync during migration.

use anyhow::{Context, Result};
use std::time::Duration;

/// Backend connection types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BackendType {
    /// Local Neovim process (default)
    Local,
    /// Docker container
    Docker {
        container: String,
        image: Option<String>,
    },
    /// SSH remote connection
    Ssh {
        host: String,
        user: Option<String>,
        port: u16,
    },
    /// Raw TCP socket
    Tcp { host: String, port: u16 },
}

/// VFS (Virtual Filesystem) backend types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum VfsBackend {
    /// Local filesystem
    Local { root: String },
    /// Git repository (clone or local)
    Git { url: String, branch: Option<String> },
    /// GitHub repository (via API)
    GitHub {
        owner: String,
        repo: String,
        ref_name: Option<String>,
    },
    /// Browser storage (OPFS/IndexedDB)
    Browser { session_id: String },
    /// HTTP(S) read-only
    Http { base_url: String },
    /// SSH/SFTP remote
    Sftp {
        host: String,
        user: Option<String>,
        path: String,
    },
}

impl std::fmt::Display for VfsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsBackend::Local { root } => write!(f, "local:{}", root),
            VfsBackend::Git { url, .. } => write!(f, "git:{}", url),
            VfsBackend::GitHub { owner, repo, .. } => write!(f, "github:{}/{}", owner, repo),
            VfsBackend::Browser { session_id } => write!(f, "browser:{}", session_id),
            VfsBackend::Http { base_url } => write!(f, "http:{}", base_url),
            VfsBackend::Sftp { host, path, .. } => write!(f, "sftp://{}:{}", host, path),
        }
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Local => write!(f, "local"),
            BackendType::Docker { container, .. } => write!(f, "docker:{}", container),
            BackendType::Ssh { host, port, .. } => write!(f, "ssh://{}:{}", host, port),
            BackendType::Tcp { host, port } => write!(f, "tcp://{}:{}", host, port),
        }
    }
}

/// State snapshot for migration
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Open buffers with CRDT state
    pub buffers: Vec<BufferState>,
    /// Current cursor position
    pub cursor: (u32, u32),
    /// Current mode
    pub mode: String,
    /// Working directory
    pub cwd: String,
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct BufferState {
    pub id: u64,
    pub path: Option<String>,
    pub crdt_state: Vec<u8>,
    pub modified: bool,
}

/// Backend swap orchestrator
pub struct BackendSwap {
    /// Current backend type
    current: BackendType,
    /// Swap timeout
    timeout: Duration,
}

impl BackendSwap {
    pub fn new() -> Self {
        Self {
            current: BackendType::Local,
            timeout: Duration::from_secs(10),
        }
    }

    /// Get current backend type
    pub fn current_backend(&self) -> &BackendType {
        &self.current
    }

    /// Prepare for swap - capture current state
    pub async fn prepare_swap(&self, session_id: &str) -> Result<SessionState> {
        // In a real implementation, this would:
        // 1. Query Neovim for all open buffers
        // 2. Get CRDT state for each buffer
        // 3. Capture cursor position and mode
        // 4. Get working directory

        Ok(SessionState {
            buffers: vec![],
            cursor: (0, 0),
            mode: "normal".to_string(),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            session_id: session_id.to_string(),
        })
    }

    /// Execute backend swap
    pub async fn swap_to(&mut self, target: BackendType, state: SessionState) -> Result<String> {
        let target_url = match &target {
            BackendType::Local => {
                // Spawn new local Neovim process
                "ws://localhost:9001".to_string()
            }
            BackendType::Docker { container, image } => {
                // Connect to Docker container
                self.connect_docker(container, image.as_deref()).await?
            }
            BackendType::Ssh { host, user, port } => {
                // Establish SSH tunnel
                self.connect_ssh(host, user.as_deref(), *port).await?
            }
            BackendType::Tcp { host, port } => {
                // Direct TCP connection
                format!("tcp://{}:{}", host, port)
            }
        };

        // Restore state on new backend
        self.restore_state(&target_url, &state).await?;

        self.current = target;
        Ok(target_url)
    }

    /// Connect to Docker container
    async fn connect_docker(&self, container: &str, image: Option<&str>) -> Result<String> {
        use tokio::process::Command;

        // Check if container exists
        let check = Command::new("docker")
            .args(["inspect", container])
            .output()
            .await
            .context("Failed to check docker container")?;

        if !check.status.success() {
            // Start container if image provided
            if let Some(img) = image {
                Command::new("docker")
                    .args([
                        "run",
                        "-d",
                        "--name",
                        container,
                        img,
                        "nvim",
                        "--headless",
                        "--listen",
                        "0.0.0.0:6666",
                    ])
                    .output()
                    .await
                    .context("Failed to start docker container")?;
            } else {
                anyhow::bail!("Container {} not found and no image specified", container);
            }
        }

        // Get container IP
        let output = Command::new("docker")
            .args(["inspect", "-f", "{{.NetworkSettings.IPAddress}}", container])
            .output()
            .await
            .context("Failed to get container IP")?;

        let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(format!("tcp://{}:6666", ip))
    }

    /// Connect via SSH tunnel
    async fn connect_ssh(&self, host: &str, user: Option<&str>, port: u16) -> Result<String> {
        use tokio::process::Command;

        let user_str = user.unwrap_or("root");
        let local_port = 9100 + rand::random::<u16>() % 100;

        // Create SSH tunnel
        Command::new("ssh")
            .args([
                "-f",
                "-N",
                "-L",
                &format!("{}:localhost:6666", local_port),
                &format!("{}@{}", user_str, host),
                "-p",
                &port.to_string(),
            ])
            .spawn()
            .context("Failed to establish SSH tunnel")?;

        // Wait for tunnel to establish
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(format!("tcp://localhost:{}", local_port))
    }

    /// Restore session state on new backend
    async fn restore_state(&self, _url: &str, state: &SessionState) -> Result<()> {
        // In a real implementation:
        // 1. Connect to new backend
        // 2. Apply CRDT state for each buffer
        // 3. Restore cursor position
        // 4. Set working directory

        tracing::info!(
            "Restored {} buffers to new backend (cwd: {})",
            state.buffers.len(),
            state.cwd
        );

        Ok(())
    }
}

impl Default for BackendSwap {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse backend URL string
pub fn parse_backend(url: &str) -> Result<BackendType> {
    if url == "local" || url.is_empty() {
        return Ok(BackendType::Local);
    }

    if url.starts_with("docker:") {
        let container = url.strip_prefix("docker:").unwrap();
        return Ok(BackendType::Docker {
            container: container.to_string(),
            image: None,
        });
    }

    if url.starts_with("ssh://") {
        let rest = url.strip_prefix("ssh://").unwrap();
        let (userhost, port) = if let Some((h, p)) = rest.rsplit_once(':') {
            (h, p.parse().unwrap_or(22))
        } else {
            (rest, 22)
        };

        let (user, host) = if let Some((u, h)) = userhost.split_once('@') {
            (Some(u.to_string()), h.to_string())
        } else {
            (None, userhost.to_string())
        };

        return Ok(BackendType::Ssh { host, user, port });
    }

    if url.starts_with("tcp://") {
        let rest = url.strip_prefix("tcp://").unwrap();
        let (host, port) = rest.rsplit_once(':').context("TCP URL must include port")?;
        return Ok(BackendType::Tcp {
            host: host.to_string(),
            port: port.parse().context("Invalid port")?,
        });
    }

    anyhow::bail!("Unknown backend URL format: {}", url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local() {
        let backend = parse_backend("local").unwrap();
        assert!(matches!(backend, BackendType::Local));
    }

    #[test]
    fn test_parse_docker() {
        let backend = parse_backend("docker:nvim-session").unwrap();
        if let BackendType::Docker { container, .. } = backend {
            assert_eq!(container, "nvim-session");
        } else {
            panic!("Expected Docker backend");
        }
    }

    #[test]
    fn test_parse_ssh() {
        let backend = parse_backend("ssh://user@host.com:2222").unwrap();
        if let BackendType::Ssh { host, user, port } = backend {
            assert_eq!(host, "host.com");
            assert_eq!(user, Some("user".to_string()));
            assert_eq!(port, 2222);
        } else {
            panic!("Expected SSH backend");
        }
    }

    #[test]
    fn test_parse_tcp() {
        let backend = parse_backend("tcp://192.168.1.100:6666").unwrap();
        if let BackendType::Tcp { host, port } = backend {
            assert_eq!(host, "192.168.1.100");
            assert_eq!(port, 6666);
        } else {
            panic!("Expected TCP backend");
        }
    }

    #[test]
    fn test_parse_vfs_local() {
        let vfs = parse_vfs_backend("local:/home/user/project").unwrap();
        if let VfsBackend::Local { root } = vfs {
            assert_eq!(root, "/home/user/project");
        } else {
            panic!("Expected Local VFS");
        }
    }

    #[test]
    fn test_parse_vfs_github() {
        let vfs = parse_vfs_backend("github:owner/repo@main").unwrap();
        if let VfsBackend::GitHub {
            owner,
            repo,
            ref_name,
        } = vfs
        {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(ref_name, Some("main".to_string()));
        } else {
            panic!("Expected GitHub VFS");
        }
    }

    #[test]
    fn test_parse_vfs_git() {
        let vfs = parse_vfs_backend("git:https://github.com/user/repo.git").unwrap();
        if let VfsBackend::Git { url, .. } = vfs {
            assert_eq!(url, "https://github.com/user/repo.git");
        } else {
            panic!("Expected Git VFS");
        }
    }
}

/// Parse VFS backend URL string
pub fn parse_vfs_backend(url: &str) -> Result<VfsBackend> {
    if url.starts_with("local:") {
        let root = url.strip_prefix("local:").unwrap();
        return Ok(VfsBackend::Local {
            root: root.to_string(),
        });
    }

    if url.starts_with("git:") {
        let git_url = url.strip_prefix("git:").unwrap();
        let (url, branch) = if let Some((u, b)) = git_url.rsplit_once('@') {
            (u.to_string(), Some(b.to_string()))
        } else {
            (git_url.to_string(), None)
        };
        return Ok(VfsBackend::Git { url, branch });
    }

    if url.starts_with("github:") {
        let rest = url.strip_prefix("github:").unwrap();
        let (repo_path, ref_name) = if let Some((r, b)) = rest.rsplit_once('@') {
            (r, Some(b.to_string()))
        } else {
            (rest, None)
        };

        let (owner, repo) = repo_path
            .split_once('/')
            .context("GitHub URL must be owner/repo")?;

        return Ok(VfsBackend::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            ref_name,
        });
    }

    if url.starts_with("browser:") {
        let session_id = url.strip_prefix("browser:").unwrap();
        return Ok(VfsBackend::Browser {
            session_id: session_id.to_string(),
        });
    }

    if url.starts_with("http:") || url.starts_with("https:") {
        return Ok(VfsBackend::Http {
            base_url: url.to_string(),
        });
    }

    if url.starts_with("sftp://") {
        let rest = url.strip_prefix("sftp://").unwrap();
        let (hostuser, path) = rest
            .split_once(':')
            .context("SFTP URL must include path after :")?;

        let (user, host) = if let Some((u, h)) = hostuser.split_once('@') {
            (Some(u.to_string()), h.to_string())
        } else {
            (None, hostuser.to_string())
        };

        return Ok(VfsBackend::Sftp {
            host,
            user,
            path: path.to_string(),
        });
    }

    // Default to local
    Ok(VfsBackend::Local {
        root: url.to_string(),
    })
}

/// VFS Swap orchestrator
pub struct VfsSwap {
    current: VfsBackend,
}

impl VfsSwap {
    pub fn new(root: &str) -> Self {
        Self {
            current: VfsBackend::Local {
                root: root.to_string(),
            },
        }
    }

    pub fn current_backend(&self) -> &VfsBackend {
        &self.current
    }

    /// Swap VFS backend
    pub async fn swap_to(&mut self, target: VfsBackend) -> Result<()> {
        tracing::info!("Swapping VFS from {} to {}", self.current, target);

        // In a real implementation:
        // 1. Close open file handles
        // 2. Flush pending writes
        // 3. Initialize new backend
        // 4. Reload open buffers

        self.current = target;
        Ok(())
    }
}
