#![allow(clippy::non_std_lazy_statics)]
#![allow(unsafe_code)]
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use ssh2::{Session, Sftp};

use super::{FileStat, VfsBackend};
use secrecy::{ExposeSecret, SecretString};

/// Connection pool entry with last-used timestamp
struct PoolEntry {
    backend: Arc<SshFsBackend>,
    last_used: Instant,
}

// SSH connection pool: key = "user@host:port"
lazy_static::lazy_static! {
    static ref SSH_POOL: RwLock<HashMap<String, PoolEntry>> = RwLock::new(HashMap::new());
}

/// Connection pool TTL (5 minutes idle)
const POOL_TTL: Duration = Duration::from_secs(300);

/// SSH filesystem backend using SFTP
///
/// Connects to remote servers via SSH and provides file operations over SFTP.
/// Connections are pooled and reused for performance.
///
/// URI format: `vfs://ssh/<user>@<host>:<port>/<absolute-path>`
/// Example: `vfs://ssh/alice@server:22/home/alice/main.rs`
pub struct SshFsBackend {
    inner: Mutex<SshFsInner>,
    pool_key: String,
}

struct SshFsInner {
    _session: Session,
    sftp: Sftp,
}

// SshFsBackend is Send + Sync because it wraps non-Send types in Mutex
// SshFsBackend is Send + Sync because it wraps non-Send types in Mutex
unsafe impl Send for SshFsBackend {}
unsafe impl Sync for SshFsBackend {}

/// Parsed SSH connection info
#[derive(Clone)]
struct ParsedSsh {
    user: String,
    host: String,
    port: u16,
    /// Password stored securely - auto-zeroed on drop
    password: Option<SecretString>,
}

impl SshFsBackend {
    /// Get a pooled connection or create a new one
    ///
    /// Connections are cached by "user@host:port" and reused for 5 minutes.
    /// Connections are health-checked before reuse; stale connections are replaced.
    pub fn get_or_connect(uri: &str) -> Result<Arc<Self>> {
        let parsed = Self::parse_ssh_uri(uri)?;
        let pool_key = format!("{}@{}:{}", parsed.user, parsed.host, parsed.port);

        // Try to get from pool and verify it's still alive
        {
            let pool = SSH_POOL
                .read()
                .map_err(|_| anyhow::anyhow!("SSH pool lock poisoned"))?;
            if let Some(entry) = pool.get(&pool_key) {
                if entry.last_used.elapsed() < POOL_TTL {
                    // Health check: verify connection is still alive
                    if entry.backend.is_alive() {
                        eprintln!("  [ssh] Reusing pooled connection to {pool_key}");
                        return Ok(entry.backend.clone());
                    }
                    eprintln!("  [ssh] Pooled connection to {pool_key} is stale, reconnecting");
                }
            }
        }

        // Create new connection (or reconnect)
        eprintln!("  [ssh] Creating new connection to {pool_key}");
        let backend = Arc::new(Self::connect_new(&parsed)?);

        // Store in pool
        {
            let mut pool = SSH_POOL
                .write()
                .map_err(|_| anyhow::anyhow!("SSH pool lock poisoned"))?;

            // Cleanup expired entries
            pool.retain(|_, entry| entry.last_used.elapsed() < POOL_TTL);

            pool.insert(
                pool_key,
                PoolEntry {
                    backend: backend.clone(),
                    last_used: Instant::now(),
                },
            );
        }

        Ok(backend)
    }

    /// Check if the SSH connection is still alive
    pub fn is_alive(&self) -> bool {
        if let Ok(inner) = self.inner.lock() {
            // Try to stat the root directory as a health check
            inner.sftp.stat(Path::new("/")).is_ok()
        } else {
            false
        }
    }

    /// Touch connection (update last-used timestamp)
    pub fn touch(&self) {
        if let Ok(mut pool) = SSH_POOL.write() {
            if let Some(entry) = pool.get_mut(&self.pool_key) {
                entry.last_used = Instant::now();
            }
        }
    }

    /// Create a new SSH connection (internal)
    fn connect_new(parsed: &ParsedSsh) -> Result<Self> {
        let pool_key = format!("{}@{}:{}", parsed.user, parsed.host, parsed.port);
        let addr = format!("{}:{}", parsed.host, parsed.port);

        let tcp =
            TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {addr}"))?;

        // Set TCP keepalive
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        Self::authenticate(&session, parsed)?;

        let sftp = session.sftp().context("Failed to initialize SFTP")?;

        Ok(Self {
            inner: Mutex::new(SshFsInner {
                _session: session,
                sftp,
            }),
            pool_key,
        })
    }

    /// Legacy connect method (creates unpooled connection)
    /// Prefer `get_or_connect()` for pooled connections
    pub fn connect(uri: &str) -> Result<Self> {
        let parsed = Self::parse_ssh_uri(uri)?;
        Self::connect_new(&parsed)
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

        Ok(ParsedSsh {
            user,
            host,
            port,
            password: None,
        })
    }

    /// Test SSH connection without storing it
    pub fn test_connection(uri: &str, password: Option<&str>) -> Result<()> {
        let mut parsed = Self::parse_ssh_uri(uri)?;
        parsed.password = password.map(|p| SecretString::new(p.to_string()));
        let _backend = Self::connect_new_with_password(&parsed)?;
        Ok(())
    }

    /// Connect with optional password and return pooled backend
    pub fn connect_with_password(uri: &str, password: Option<&str>) -> Result<Arc<Self>> {
        let mut parsed = Self::parse_ssh_uri(uri)?;
        parsed.password = password.map(|p| SecretString::new(p.to_string()));
        let pool_key = format!("{}@{}:{}", parsed.user, parsed.host, parsed.port);

        // Create new connection with password
        let backend = Arc::new(Self::connect_new_with_password(&parsed)?);

        // Store in pool
        {
            let mut pool = SSH_POOL
                .write()
                .map_err(|_| anyhow::anyhow!("SSH pool lock poisoned"))?;
            pool.retain(|_, entry| entry.last_used.elapsed() < POOL_TTL);
            pool.insert(
                pool_key,
                PoolEntry {
                    backend: backend.clone(),
                    last_used: Instant::now(),
                },
            );
        }

        Ok(backend)
    }

    /// Create connection with password support
    fn connect_new_with_password(parsed: &ParsedSsh) -> Result<Self> {
        let pool_key = format!("{}@{}:{}", parsed.user, parsed.host, parsed.port);
        let addr = format!("{}:{}", parsed.host, parsed.port);

        let tcp =
            TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {addr}"))?;
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        Self::authenticate_with_password(&session, parsed)?;

        let sftp = session.sftp().context("Failed to initialize SFTP")?;

        Ok(Self {
            inner: Mutex::new(SshFsInner {
                _session: session,
                sftp,
            }),
            pool_key,
        })
    }

    /// Authenticate using SSH agent or default key
    fn authenticate(session: &Session, parsed: &ParsedSsh) -> Result<()> {
        Self::authenticate_with_password(session, parsed)
    }

    /// Authenticate with password support
    fn authenticate_with_password(session: &Session, parsed: &ParsedSsh) -> Result<()> {
        // Try password first if provided (expose secret only at auth time)
        if let Some(ref password) = parsed.password {
            if session
                .userauth_password(&parsed.user, password.expose_secret())
                .is_ok()
            {
                eprintln!("  [ssh] Authenticated with password");
                return Ok(());
            }
        }

        // Try SSH agent
        if session.userauth_agent(&parsed.user).is_ok() {
            eprintln!("  [ssh] Authenticated via SSH agent");
            return Ok(());
        }

        // Try default key files
        let home = dirs::home_dir().context("Could not determine home directory")?;
        for key_name in ["id_rsa", "id_ed25519", "id_ecdsa"] {
            let key_path = home.join(".ssh").join(key_name);
            if key_path.exists()
                && session
                    .userauth_pubkey_file(&parsed.user, None, &key_path, None)
                    .is_ok()
            {
                eprintln!("  [ssh] Authenticated with key: {key_name}");
                return Ok(());
            }
        }

        bail!("SSH authentication failed: no valid credentials")
    }
}

#[async_trait]
#[allow(clippy::significant_drop_tightening)]
impl VfsBackend for SshFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = path.to_string();
        // Use a block to constrain the lock lifetime
        let buf = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;
            let sftp = &inner.sftp;

            let mut file = sftp
                .open(Path::new(&path))
                .with_context(|| format!("Failed to open {path}"))?;

            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .with_context(|| format!("Failed to read {path}"))?;
            buf
        };

        Ok(buf)
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        use ssh2::OpenFlags;
        use ssh2::OpenType;

        {
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
                .with_context(|| format!("Failed to open {path} for writing"))?;

            file.write_all(data)
                .with_context(|| format!("Failed to write to {path}"))?;
        }

        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let stat = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;

            inner
                .sftp
                .stat(Path::new(path))
                .with_context(|| format!("Failed to stat {path}"))?
        };

        Ok(FileStat {
            is_file: stat.is_file(),
            is_dir: stat.is_dir(),
            size: stat.size.unwrap_or(0),
            created: None, // SFTP doesn't provide creation time
            modified: stat
                .mtime
                .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t)),
            readonly: false,
        })
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let names = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("SSH mutex poisoned"))?;

            let entries = inner
                .sftp
                .readdir(Path::new(path))
                .with_context(|| format!("Failed to list directory {path}"))?;

            entries
                .into_iter()
                .filter_map(|(p, _)| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(ToString::to_string)
                })
                .collect()
        };

        Ok(names)
    }
}
