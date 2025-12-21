use anyhow::Result;

/// File metadata returned by stat operations
#[derive(Debug, Clone)]
pub struct FileStat {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

/// VFS backend trait - all file operations go through this
pub trait VfsBackend {
    /// Read entire file contents
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    
    /// Write entire file contents (create or overwrite)
    fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    
    /// Get file/directory metadata
    fn stat(&self, path: &str) -> Result<FileStat>;
    
    /// List directory contents (basenames only)
    fn list(&self, path: &str) -> Result<Vec<String>>;
}
