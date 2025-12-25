//! Browser-based VFS backend using OPFS (Origin Private File System)
//!
//! Status: DISABLED - Requires async migration and browser-side OPFS implementation
//!
//! This backend is designed to:
//! - Store files in browser's Origin Private File System (OPFS)
//! - Communicate with the browser via WebSocket RPC protocol
//! - Persist files across browser sessions without server storage
//!
//! The implementation requires:
//! 1. Browser-side OPFS handler (ui/fs/opfs.ts) - partially implemented
//! 2. Async WebSocket RPC for file operations - pending
//! 3. Request/response correlation - pending
//!
//! For browser storage needs, consider:
//! - localStorage for small config data
//! - IndexedDB for larger structured data
//! - Server-side LocalFs for file persistence
//!
//! The VfsBackend trait impl below returns errors to clearly indicate
//! that BrowserFs is not yet functional.

use anyhow::{Result, bail};
use super::{VfsBackend, FileStat};

/// Browser-based VFS backend placeholder
/// 
/// This struct exists to maintain the VFS architecture but is not functional.
/// All operations return errors indicating the feature is pending.
pub struct BrowserFsBackend {
    /// Namespace within OPFS (e.g., "project1", "scratch")
    pub namespace: String,
}

impl BrowserFsBackend {
    /// Create a new BrowserFs backend for the given namespace
    /// 
    /// Note: This backend is not functional - see module documentation
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
        }
    }
}

impl VfsBackend for BrowserFsBackend {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        bail!(
            "BrowserFs not implemented: cannot read '{}' from namespace '{}'. \
            Use LocalFs or SSH backends instead.",
            path, self.namespace
        )
    }

    fn write(&self, path: &str, _data: &[u8]) -> Result<()> {
        bail!(
            "BrowserFs not implemented: cannot write '{}' to namespace '{}'. \
            Use LocalFs or SSH backends instead.",
            path, self.namespace
        )
    }

    fn stat(&self, path: &str) -> Result<FileStat> {
        bail!(
            "BrowserFs not implemented: cannot stat '{}' in namespace '{}'. \
            Use LocalFs or SSH backends instead.",
            path, self.namespace
        )
    }

    fn list(&self, path: &str) -> Result<Vec<String>> {
        bail!(
            "BrowserFs not implemented: cannot list '{}' in namespace '{}'. \
            Use LocalFs or SSH backends instead.",
            path, self.namespace
        )
    }
}
