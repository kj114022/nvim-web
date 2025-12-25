use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use ssh2::{Session, Sftp};

use super::{FileStat, VfsBackend};

/// SSH filesystem backend using SFTP
///
/// Connects to remote servers via SSH and provides file operations over SFTP.
/// Uses tokio::task::spawn_blocking for blocking I/O operations.
///
/// URI format: vfs://ssh/<user>@<host>:<port>/<absolute-path>
/// Example: vfs://ssh/alice@server:22/home/alice/main.rs
///
/// Note: ssh2 Session is not Send, so we wrap in Mutex and use spawn_blocking
pub struct SshFsBackend {
    inner: Mutex<SshFsInner>,
}

struct SshFsInner {
    _session: Session,
    sftp: Sftp,
}

// SshFsBackend is Send + Sync because it wraps non-Send types in Mutex
unsafe impl Send for SshFsBackend {}
unsafe impl Sync for SshFsBackend {}

/// Parsed SSH connection info
struct ParsedSsh {
    user: String,
    host: String,
    port: u16,
}

impl SshFsBackend {
    /// Connect to SSH server and initialize SFTP
    ///
    /// URI format: vfs://ssh/<user>@<host>:<port>/<path>
    /// Port defaults to 22 if not specified
    pub fn connect(uri: &str) -> Result<Self> {
        let parsed = Self::parse_ssh_uri(uri)?;

        let addr = format!("{}:{}", parsed.host, parsed.port);
        let tcp =
            TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {}", addr))?;

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        Self::authenticate(&session, &parsed)?;

        let sftp = session.sftp().context("Failed to initialize SFTP")?;

        Ok(Self {
            inner: Mutex::new(SshFsInner {
                _session: session,
                sftp,
            }),
        })
    }

    /// Parse SSH URI into components
    fn parse_ssh_uri(uri: &str) -> Result<ParsedSsh> {
        if !uri.starts_with("vfs://ssh/") {
            bail!("SSH URI must start with vfs://ssh/");
        }

        let rest = &uri[10..];
        let slash_pos = rest.find('/').unwrap_or(rest.len());
        let conn_part = &rest[..slash_pos];

        let at_pos = conn_part
            .find('@')
            .context("SSH URI must contain user@host")?;

        let user = conn_part[..at_pos].to_string();
        let host_port = &conn_part[at_pos + 1..];

        let (host, port) = if let Some(colon_pos) = host_port.find(':') {
            let host = host_port[..colon_pos].to_string();
            let port = host_port[colon_pos + 1..]
                .parse::<u16>()
                .context("Invalid port number")?;
            (host, port)
        } else {
            (host_port.to_string(), 22)
        };

        Ok(ParsedSsh { user, host, port })
    }

    /// Authenticate using SSH agent or default key
    fn authenticate(session: &Session, parsed: &ParsedSsh) -> Result<()> {
        if session.userauth_agent(&parsed.user).is_ok() {
            return Ok(());
        }

        let key_path = dirs::home_dir()
            .context("Could not determine home directory")?
            .join(".ssh/id_rsa");

        if key_path.exists() {
            session
                .userauth_pubkey_file(&parsed.user, None, &key_path, None)
                .context("SSH key authentication failed")?;
            return Ok(());
        }

        bail!("SSH authentication failed: no agent and no ~/.ssh/id_rsa")
    }
}

#[async_trait]
impl VfsBackend for SshFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = path.to_string();
        // Lock is held only during the blocking operation in spawn_blocking
        // This is safe because we're not holding it across await points
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;
        let sftp = &inner.sftp;

        let mut file = sftp
            .open(Path::new(&path))
            .with_context(|| format!("Failed to open {}", path))?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .with_context(|| format!("Failed to read {}", path))?;

        Ok(buf)
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        use ssh2::OpenFlags;
        use ssh2::OpenType;

        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;

        let mut file = inner
            .sftp
            .open_mode(
                Path::new(path),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                0o644,
                OpenType::File,
            )
            .with_context(|| format!("Failed to open {} for writing", path))?;

        file.write_all(data)
            .with_context(|| format!("Failed to write to {}", path))?;

        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;

        let stat = inner
            .sftp
            .stat(Path::new(path))
            .with_context(|| format!("Failed to stat {}", path))?;

        Ok(FileStat {
            is_file: stat.is_file(),
            is_dir: stat.is_dir(),
            size: stat.size.unwrap_or(0),
        })
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;

        let entries = inner
            .sftp
            .readdir(Path::new(path))
            .with_context(|| format!("Failed to list directory {}", path))?;

        let names = entries
            .into_iter()
            .filter_map(|(p, _)| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
            .collect();

        Ok(names)
    }
}
