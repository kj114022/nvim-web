use std::fs;
use std::path::PathBuf;
use anyhow::{Result, bail};

use super::backend::{VfsBackend, FileStat};

/// Local filesystem backend - maps vfs://local/... to real filesystem
pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    /// Create new local FS backend with specified root directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root_path = root.into();
        // Ensure root exists and canonicalize it
        let _ = fs::create_dir_all(&root_path);
        Self { 
            root: root_path.canonicalize().unwrap_or(root_path)
        }
    }
    
    /// Resolve VFS path to absolute filesystem path with security checks
    /// 
    /// SECURITY: Prevents path traversal attacks by:
    /// 1. Canonicalizing the resolved path
    /// 2. Verifying it stays within the sandbox root
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        // Build the target path
        let target = self.root.join(path.trim_start_matches('/'));
        
        // For read/stat operations, canonicalize to get real path
        // For write operations, parent must exist and be within sandbox
        let resolved = if target.exists() {
            target.canonicalize()?
        } else {
            // For non-existent files, check parent directory
            let parent = target.parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: no parent"))?;
            
            // Create parent if needed, then canonicalize
            fs::create_dir_all(parent)?;
            let canonical_parent = parent.canonicalize()?;
            
            // Reconstruct path with canonical parent
            canonical_parent.join(target.file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: no filename"))?)
        };
        
        // SECURITY CHECK: Verify path is within sandbox
        if !resolved.starts_with(&self.root) {
            bail!("Path traversal blocked: {} escapes sandbox {}", 
                  path, self.root.display());
        }
        
        Ok(resolved)
    }
}

impl VfsBackend for LocalFs {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve(path)?;
        Ok(fs::read(resolved)?)
    }
    
    fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let resolved = self.resolve(path)?;
        // Parent directories are created by resolve() for non-existent paths
        fs::write(resolved, data)?;
        Ok(())
    }
    
    fn stat(&self, path: &str) -> Result<FileStat> {
        let resolved = self.resolve(path)?;
        let meta = fs::metadata(resolved)?;
        Ok(FileStat {
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            size: meta.len(),
        })
    }
    
    fn list(&self, path: &str) -> Result<Vec<String>> {
        let resolved = self.resolve(path)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(resolved)? {
            let entry = entry?;
            entries.push(entry.file_name().to_string_lossy().into_owned());
        }
        Ok(entries)
    }
}
