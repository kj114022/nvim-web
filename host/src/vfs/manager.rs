use std::collections::HashMap;
use anyhow::{Result, Context};

use super::backend::VfsBackend;

/// Metadata for a VFS-managed buffer
#[derive(Debug, Clone)]
pub struct ManagedBuffer {
    pub bufnr: u32,
    pub vfs_path: String,
    pub backend: String,  // backend name like "local", "browser", etc.
}

/// VFS manager - coordinates backends and tracks managed buffers
pub struct VfsManager {
    backends: HashMap<String, Box<dyn VfsBackend>>,
    managed_buffers: HashMap<u32, ManagedBuffer>,
}

impl VfsManager {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            managed_buffers: HashMap::new(),
        }
    }
    
    /// Register a VFS backend
    pub fn register_backend(&mut self, name: impl Into<String>, backend: Box<dyn VfsBackend>) {
        self.backends.insert(name.into(), backend);
    }
    
    /// Parse VFS path: vfs://backend/path -> (backend, path)
    pub fn parse_vfs_path(&self, vfs_path: &str) -> Result<(String, String)> {
        if !vfs_path.starts_with("vfs://") {
            anyhow::bail!("Invalid VFS path: must start with vfs://");
        }
        
        let path = &vfs_path[6..]; // strip "vfs://"
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        
        if parts.len() < 2 {
            anyhow::bail!("Invalid VFS path format: vfs://backend/path");
        }
        
        Ok((parts[0].to_string(), parts[1].to_string()))
    }
    
    /// Read file via VFS backend
    pub fn read_file(&self, vfs_path: &str) -> Result<Vec<u8>> {
        let (backend_name, path) = self.parse_vfs_path(vfs_path)?;
        
        let backend = self.backends.get(&backend_name)
            .with_context(|| format!("Unknown VFS backend: {}", backend_name))?;
        
        backend.read(&path)
    }
    
    /// Write file via VFS backend
    pub fn write_file(&self, vfs_path: &str, data: &[u8]) -> Result<()> {
        let (backend_name, path) = self.parse_vfs_path(vfs_path)?;
        
        let backend = self.backends.get(&backend_name)
            .with_context(|| format!("Unknown VFS backend: {}", backend_name))?;
        
        backend.write(&path, data)
    }
    
    /// Register a buffer as VFS-managed
    pub fn register_buffer(&mut self, bufnr: u32, vfs_path: String) -> Result<()> {
        let (backend_name, _) = self.parse_vfs_path(&vfs_path)?;
        
        let managed = ManagedBuffer {
            bufnr,
            vfs_path,
            backend: backend_name.to_string(),
        };
        
        self.managed_buffers.insert(bufnr, managed);
        Ok(())
    }
    
    /// Get managed buffer metadata
    pub fn get_managed_buffer(&self, bufnr: u32) -> Option<&ManagedBuffer> {
        self.managed_buffers.get(&bufnr)
    }
    
    /// Unregister a buffer (on close)
    pub fn unregister_buffer(&mut self, bufnr: u32) {
        self.managed_buffers.remove(&bufnr);
    }
}
