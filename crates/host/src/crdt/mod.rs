//! CRDT (Conflict-free Replicated Data Type) support for real-time collaboration
//!
//! Uses y-crdt (Yjs in Rust) for conflict-free concurrent editing.
//! Each buffer gets its own Y.Doc with text content that syncs
//! bidirectionally between Neovim and connected clients.

mod buffer;
mod sync;

pub use buffer::BufferCrdt;
pub use sync::CrdtSync;
pub use nvim_web_protocol::crdt::SyncMessage;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// CRDT document manager for a session
///
/// Manages CRDT documents for all buffers in a session.
#[derive(Debug)]
pub struct CrdtManager {
    /// Buffer ID -> CRDT document
    buffers: HashMap<u64, BufferCrdt>,
    /// Session ID for logging
    session_id: String,
}

impl CrdtManager {
    /// Create a new CRDT manager for a session
    pub fn new(session_id: String) -> Self {
        Self {
            buffers: HashMap::new(),
            session_id,
        }
    }

    /// Get or create a CRDT document for a buffer
    pub fn get_or_create(&mut self, buffer_id: u64) -> &mut BufferCrdt {
        self.buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferCrdt::new(buffer_id))
    }

    /// Get a CRDT document for a buffer (if exists)
    pub fn get(&self, buffer_id: u64) -> Option<&BufferCrdt> {
        self.buffers.get(&buffer_id)
    }

    /// Get mutable CRDT document for a buffer (if exists)
    pub fn get_mut(&mut self, buffer_id: u64) -> Option<&mut BufferCrdt> {
        self.buffers.get_mut(&buffer_id)
    }

    /// Remove a buffer's CRDT document (e.g., when buffer is closed)
    pub fn remove(&mut self, buffer_id: u64) -> Option<BufferCrdt> {
        self.buffers.remove(&buffer_id)
    }

    /// Get all buffer IDs with CRDT documents
    pub fn buffer_ids(&self) -> Vec<u64> {
        self.buffers.keys().copied().collect()
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Shared CRDT manager wrapped in Arc<RwLock>
pub type SharedCrdtManager = Arc<RwLock<CrdtManager>>;

/// Create a new shared CRDT manager
pub fn create_manager(session_id: String) -> SharedCrdtManager {
    Arc::new(RwLock::new(CrdtManager::new(session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_get_or_create() {
        let mut mgr = CrdtManager::new("test".to_string());

        // First access creates
        let crdt = mgr.get_or_create(1);
        assert_eq!(crdt.buffer_id(), 1);

        // Second access returns same
        let crdt2 = mgr.get_or_create(1);
        assert_eq!(crdt2.buffer_id(), 1);

        // Different buffer creates new
        let crdt3 = mgr.get_or_create(2);
        assert_eq!(crdt3.buffer_id(), 2);

        assert_eq!(mgr.buffer_ids().len(), 2);
    }

    #[test]
    fn test_manager_remove() {
        let mut mgr = CrdtManager::new("test".to_string());
        mgr.get_or_create(1);
        mgr.get_or_create(2);

        assert!(mgr.remove(1).is_some());
        assert!(mgr.get(1).is_none());
        assert!(mgr.get(2).is_some());
    }
}
