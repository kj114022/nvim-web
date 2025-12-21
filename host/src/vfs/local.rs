use std::fs;
use std::path::PathBuf;
use anyhow::Result;

use super::backend::{VfsBackend, FileStat};

/// Local filesystem backend - maps vfs://local/... to real filesystem
pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    /// Create new local FS backend with specified root directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    
    /// Resolve VFS path to absolute filesystem path
    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }
}

impl VfsBackend for LocalFs {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        Ok(fs::read(self.resolve(path))?)
    }
    
    fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let resolved = self.resolve(path);
        
        // Create parent directories if needed
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(resolved, data)?;
        Ok(())
    }
    
    fn stat(&self, path: &str) -> Result<FileStat> {
        let meta = fs::metadata(self.resolve(path))?;
        Ok(FileStat {
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            size: meta.len(),
        })
    }
    
    fn list(&self, path: &str) -> Result<Vec<String>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.resolve(path))? {
            let entry = entry?;
            entries.push(entry.file_name().to_string_lossy().into_owned());
        }
        Ok(entries)
    }
}
