//! Browser-based VFS backend using OPFS (Origin Private File System)
//!
//! TODO: Migrate to async. Currently stubbed - BrowserFs functionality disabled.

use anyhow::{Result, bail};
use super::{VfsBackend, FileStat};
use std::sync::mpsc::Sender;

/// Browser-based VFS backend using OPFS (Origin Private File System)
/// 
/// This backend delegates storage to the browser's OPFS via WebSocket RPC.
/// The host owns VFS semantics; the browser owns storage.
/// 
/// TODO: Migrate to async using tokio channels and nvim-rs patterns.
pub struct BrowserFsBackend {
    pub namespace: String,
    #[allow(dead_code)]
    ws_tx: Sender<Vec<u8>>,
}

impl BrowserFsBackend {
    /// Create a new BrowserFs backend for the given namespace
    pub fn new(namespace: impl Into<String>, ws_tx: Sender<Vec<u8>>) -> Self {
        Self {
            namespace: namespace.into(),
            ws_tx,
        }
    }
}

impl VfsBackend for BrowserFsBackend {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        eprintln!("BrowserFs: read not implemented for path: {}", path);
        bail!("BrowserFs not yet implemented in async architecture")
    }

    fn write(&self, path: &str, _data: &[u8]) -> Result<()> {
        eprintln!("BrowserFs: write not implemented for path: {}", path);
        bail!("BrowserFs not yet implemented in async architecture")
    }

    fn stat(&self, path: &str) -> Result<FileStat> {
        eprintln!("BrowserFs: stat not implemented for path: {}", path);
        bail!("BrowserFs not yet implemented in async architecture")
    }

    fn list(&self, path: &str) -> Result<Vec<String>> {
        eprintln!("BrowserFs: list not implemented for path: {}", path);
        bail!("BrowserFs not yet implemented in async architecture")
    }
}
