//! Multi-seat collaboration support
//!
//! Allows multiple clients to share the same Neovim session.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

/// Unique identifier for a connected client
pub type ClientId = String;

/// Collaboration metadata for a session
#[derive(Debug, Clone)]
pub struct CollaborationInfo {
    /// Session ID
    pub session_id: String,
    /// Connected client IDs
    pub clients: HashSet<ClientId>,
    /// Session owner (first client to join)
    pub owner: Option<ClientId>,
    /// Whether new clients can join
    pub open: bool,
}

impl CollaborationInfo {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            clients: HashSet::new(),
            owner: None,
            open: true,
        }
    }

    pub fn add_client(&mut self, client_id: ClientId) {
        if self.owner.is_none() {
            self.owner = Some(client_id.clone());
        }
        self.clients.insert(client_id);
    }

    pub fn remove_client(&mut self, client_id: &str) {
        self.clients.remove(client_id);
        // Transfer ownership if owner leaves
        if self.owner.as_deref() == Some(client_id) {
            self.owner = self.clients.iter().next().cloned();
        }
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// Manager for multi-seat collaboration
pub struct CollaborationManager {
    sessions: HashMap<String, CollaborationInfo>,
}

impl CollaborationManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Register a client joining a session
    pub fn join_session(&mut self, session_id: &str, client_id: ClientId) {
        let info = self
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| CollaborationInfo::new(session_id));

        info.add_client(client_id.clone());
        eprintln!(
            "COLLAB: Client {} joined session {} (total: {})",
            client_id,
            session_id,
            info.client_count()
        );
    }

    /// Register a client leaving a session
    pub fn leave_session(&mut self, session_id: &str, client_id: &str) {
        if let Some(info) = self.sessions.get_mut(session_id) {
            info.remove_client(client_id);
            eprintln!(
                "COLLAB: Client {} left session {} (remaining: {})",
                client_id,
                session_id,
                info.client_count()
            );

            // Clean up empty sessions
            if info.is_empty() {
                self.sessions.remove(session_id);
            }
        }
    }

    /// Get collaboration info for a session
    pub fn get_info(&self, session_id: &str) -> Option<&CollaborationInfo> {
        self.sessions.get(session_id)
    }

    /// List all collaborative sessions
    pub fn list_sessions(&self) -> Vec<&CollaborationInfo> {
        self.sessions.values().collect()
    }

    /// Generate a unique client ID
    pub fn generate_client_id() -> ClientId {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("client-{:x}", ts % 0xFFFFFFFF)
    }
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for CollaborationManager
pub type SharedCollaborationManager = Arc<RwLock<CollaborationManager>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_leave() {
        let mut mgr = CollaborationManager::new();

        mgr.join_session("sess1", "client-a".to_string());
        mgr.join_session("sess1", "client-b".to_string());

        let info = mgr.get_info("sess1").unwrap();
        assert_eq!(info.client_count(), 2);
        assert_eq!(info.owner, Some("client-a".to_string()));

        mgr.leave_session("sess1", "client-a");
        let info = mgr.get_info("sess1").unwrap();
        assert_eq!(info.client_count(), 1);
        assert_eq!(info.owner, Some("client-b".to_string()));
    }

    #[test]
    fn test_empty_cleanup() {
        let mut mgr = CollaborationManager::new();

        mgr.join_session("sess1", "client-a".to_string());
        mgr.leave_session("sess1", "client-a");

        assert!(mgr.get_info("sess1").is_none());
    }
}
