use std::collections::HashMap;

use anyhow::{Context, Result};

use super::backend::VfsBackend;

/// Metadata for a VFS-managed buffer
#[derive(Debug, Clone)]
pub struct ManagedBuffer {
    pub bufnr: u32,
    pub vfs_path: String,
    pub backend: String, // backend name like "local", "browser", etc.
}

/// VFS manager - coordinates backends and tracks managed buffers
pub struct VfsManager {
    backends: HashMap<String, Box<dyn VfsBackend>>,
    managed_buffers: HashMap<u32, ManagedBuffer>,
}

impl Default for VfsManager {
    fn default() -> Self {
        Self::new()
    }
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

    /// Get a reference to a registered backend
    pub fn get_backend(&self, name: &str) -> Option<&dyn VfsBackend> {
        self.backends.get(name).map(|b| b.as_ref())
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

    /// Read file via VFS backend (async)
    pub async fn read_file(&self, vfs_path: &str) -> Result<Vec<u8>> {
        let (backend_name, path) = self.parse_vfs_path(vfs_path)?;

        // SSH backend is created dynamically because connection info is in URI
        if backend_name == "ssh" {
            use super::SshFsBackend;
            let backend = SshFsBackend::connect(vfs_path)?;
            return backend.read(&path).await;
        }

        let backend = self
            .backends
            .get(&backend_name)
            .with_context(|| format!("Unknown VFS backend: {}", backend_name))?;

        backend.read(&path).await
    }

    /// Write file via VFS backend (async)
    pub async fn write_file(&self, vfs_path: &str, data: &[u8]) -> Result<()> {
        let (backend_name, path) = self.parse_vfs_path(vfs_path)?;

        // SSH backend is created dynamically because connection info is in URI
        if backend_name == "ssh" {
            use super::SshFsBackend;
            let backend = SshFsBackend::connect(vfs_path)?;
            return backend.write(&path, data).await;
        }

        let backend = self
            .backends
            .get(&backend_name)
            .with_context(|| format!("Unknown VFS backend: {}", backend_name))?;

        backend.write(&path, data).await
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
