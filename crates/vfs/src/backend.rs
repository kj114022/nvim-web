use std::time::SystemTime;

use anyhow::{bail, Result};
use async_trait::async_trait;

/// File metadata returned by stat operations
#[derive(Debug, Clone)]
pub struct FileStat {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    /// File creation time (if available)
    pub created: Option<SystemTime>,
    /// Last modification time (if available)
    pub modified: Option<SystemTime>,
    /// Read-only flag
    pub readonly: bool,
}

impl FileStat {
    /// Create a simple file stat (for backends that don't support full metadata)
    pub fn file(size: u64) -> Self {
        Self {
            is_file: true,
            is_dir: false,
            size,
            created: None,
            modified: None,
            readonly: false,
        }
    }

    /// Create a simple directory stat
    pub fn dir() -> Self {
        Self {
            is_file: false,
            is_dir: true,
            size: 0,
            created: None,
            modified: None,
            readonly: false,
        }
    }
}

/// Chunk of data from streaming read
#[derive(Debug)]
pub struct ReadChunk {
    pub data: Vec<u8>,
    pub offset: u64,
    pub is_last: bool,
}

/// Streaming read handle
///
/// Allows reading large files in chunks without loading entire file into memory.
#[async_trait]
pub trait ReadHandle: Send + Sync {
    /// Read next chunk (default chunk size is backend-dependent)
    async fn read_chunk(&mut self) -> Result<ReadChunk>;

    /// Get total file size (if known)
    fn size(&self) -> Option<u64>;

    /// Close the handle
    async fn close(&mut self) -> Result<()>;
}

/// Streaming write handle
///
/// Allows writing large files in chunks.
#[async_trait]
pub trait WriteHandle: Send + Sync {
    /// Write a chunk of data
    async fn write_chunk(&mut self, data: &[u8]) -> Result<()>;

    /// Flush and close the handle
    async fn close(&mut self) -> Result<()>;

    /// Get bytes written so far
    fn bytes_written(&self) -> u64;
}

/// VFS backend trait - all file operations go through this
///
/// This trait uses `async_trait` to support asynchronous backends like
/// `BrowserFs` which communicates over WebSocket.
///
/// Default implementations return "not supported" for optional operations,
/// allowing backends to implement only what they support.
#[async_trait]
pub trait VfsBackend: Send + Sync {
    /// Read entire file contents
    async fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Write entire file contents (create or overwrite)
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;

    /// Get file/directory metadata
    async fn stat(&self, path: &str) -> Result<FileStat>;

    /// List directory contents (basenames only)
    async fn list(&self, path: &str) -> Result<Vec<String>>;

    // ─────────────────────────────────────────────────────────────────────────
    // Optional operations with default implementations
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if a file or directory exists
    async fn exists(&self, path: &str) -> Result<bool> {
        match self.stat(path).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Create a directory (parent must exist)
    async fn create_dir(&self, _path: &str) -> Result<()> {
        bail!("create_dir not supported by this backend")
    }

    /// Create a directory and all parent directories
    async fn create_dir_all(&self, _path: &str) -> Result<()> {
        bail!("create_dir_all not supported by this backend")
    }

    /// Remove an empty directory
    async fn remove_dir(&self, _path: &str) -> Result<()> {
        bail!("remove_dir not supported by this backend")
    }

    /// Remove a file
    async fn remove_file(&self, _path: &str) -> Result<()> {
        bail!("remove_file not supported by this backend")
    }

    /// Copy a file to a new location
    async fn copy(&self, _src: &str, _dest: &str) -> Result<()> {
        bail!("copy not supported by this backend")
    }

    /// Rename/move a file or directory
    async fn rename(&self, _src: &str, _dest: &str) -> Result<()> {
        bail!("rename not supported by this backend")
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Streaming API (optional - for large file handling)
    // ─────────────────────────────────────────────────────────────────────────

    /// Open a file for streaming read
    async fn open_read(&self, _path: &str) -> Result<Box<dyn ReadHandle>> {
        bail!("streaming read not supported by this backend")
    }

    /// Open a file for streaming write (create or overwrite)
    async fn open_write(&self, _path: &str) -> Result<Box<dyn WriteHandle>> {
        bail!("streaming write not supported by this backend")
    }

    /// Check if this backend supports streaming operations
    fn supports_streaming(&self) -> bool {
        false
    }
}
