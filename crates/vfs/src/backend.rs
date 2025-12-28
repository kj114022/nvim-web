use anyhow::Result;
use async_trait::async_trait;

/// File metadata returned by stat operations
#[derive(Debug, Clone)]
pub struct FileStat {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

/// VFS backend trait - all file operations go through this
///
/// This trait uses async_trait to support asynchronous backends like
/// BrowserFs which communicates over WebSocket.
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
}
