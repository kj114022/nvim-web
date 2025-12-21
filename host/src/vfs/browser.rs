use anyhow::{Result, bail};
use super::{VfsBackend, FileStat};

/// Browser-based VFS backend using OPFS (Origin Private File System)
/// 
/// This backend delegates storage to the browser's OPFS via WebSocket RPC.
/// The host owns VFS semantics; the browser owns storage.
pub struct BrowserFsBackend {
    pub namespace: String,
    // WebSocket handle will be added in Phase 7A-2
}

impl BrowserFsBackend {
    /// Create a new BrowserFs backend for the given namespace
    /// 
    /// Namespace separates different projects/contexts in OPFS.
    /// Example: "default", "demo", "project-name"
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
        }
    }
}

impl VfsBackend for BrowserFsBackend {
    fn read(&self, _path: &str) -> Result<Vec<u8>> {
        bail!("browser fs backend not implemented yet");
    }

    fn write(&self, _path: &str, _data: &[u8]) -> Result<()> {
        bail!("browser fs backend not implemented yet");
    }

    fn stat(&self, _path: &str) -> Result<FileStat> {
        bail!("browser fs backend not implemented yet");
    }

    fn list(&self, _path: &str) -> Result<Vec<String>> {
        bail!("browser fs backend not implemented yet");
    }
}
