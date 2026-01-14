//! Collaboration module for multi-user session support
//!
//! Tracks connected viewers per session and relays cursor events between
//! the session owner and viewers for real-time cursor synchronization.
//! Integrates with CRDTs for conflict-free collaborative editing.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::crdt::{BufferCrdt, CrdtManager, CrdtSync, SyncMessage};

/// Cursor position in the editor grid
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CursorPosition {
    pub row: u32,
    pub col: u32,
    pub grid: u32,
}

/// Information about a connected viewer
#[derive(Debug, Clone, serde::Serialize)]
pub struct ViewerInfo {
    pub id: String,
    pub name: Option<String>,
    pub color: String,
    pub cursor: Option<CursorPosition>,
    pub connected_at: u64,
}

/// Collaboration event types
#[derive(Debug, Clone)]
pub enum CollabEvent {
    /// A viewer connected
    ViewerJoined(ViewerInfo),
    /// A viewer disconnected
    ViewerLeft(String),
    /// Cursor position update
    CursorMoved {
        viewer_id: String,
        position: CursorPosition,
    },
    /// Owner cursor moved (broadcast to all viewers)
    OwnerCursorMoved(CursorPosition),
    /// Buffer content changed (CRDT update)
    BufferChanged { buffer_id: u64, update: Vec<u8> },
    /// Full buffer sync for new viewer
    BufferSync { buffer_id: u64, state: Vec<u8> },
    /// WebRTC signaling: SDP offer/answer
    WebRtcSignal {
        from: String,
        to: String,
        signal_type: SignalType,
        payload: String,
    },
    /// P2P chat message (relayed through server for offline peers)
    ChatMessage {
        from: String,
        to: Option<String>, // None = broadcast
        message: String,
        timestamp: u64,
    },
}

/// WebRTC signaling types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SignalType {
    Offer,
    Answer,
    IceCandidate,
}

/// Viewer registry for a single session
#[derive(Debug)]
pub struct SessionViewers {
    viewers: HashMap<String, ViewerInfo>,
    event_tx: broadcast::Sender<CollabEvent>,
    /// CRDT manager for buffer documents
    crdt_manager: CrdtManager,
}

impl SessionViewers {
    pub fn new(session_id: &str) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            viewers: HashMap::new(),
            event_tx,
            crdt_manager: CrdtManager::new(session_id.to_string()),
        }
    }

    /// Add a new viewer to the session
    pub fn add_viewer(&mut self, id: String, name: Option<String>) -> ViewerInfo {
        let color = Self::assign_color(self.viewers.len());
        let info = ViewerInfo {
            id: id.clone(),
            name,
            color,
            cursor: None,
            connected_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        self.viewers.insert(id.clone(), info.clone());
        let _ = self.event_tx.send(CollabEvent::ViewerJoined(info.clone()));
        info
    }

    /// Remove a viewer from the session
    pub fn remove_viewer(&mut self, id: &str) {
        if self.viewers.remove(id).is_some() {
            let _ = self.event_tx.send(CollabEvent::ViewerLeft(id.to_string()));
        }
    }

    /// Update a viewer's cursor position
    pub fn update_cursor(&mut self, viewer_id: &str, position: CursorPosition) {
        if let Some(viewer) = self.viewers.get_mut(viewer_id) {
            viewer.cursor = Some(position);
            let _ = self.event_tx.send(CollabEvent::CursorMoved {
                viewer_id: viewer_id.to_string(),
                position,
            });
        }
    }

    /// Broadcast owner's cursor position to all viewers
    pub fn broadcast_owner_cursor(&self, position: CursorPosition) {
        let _ = self.event_tx.send(CollabEvent::OwnerCursorMoved(position));
    }

    /// Get list of all viewers
    pub fn list_viewers(&self) -> Vec<ViewerInfo> {
        self.viewers.values().cloned().collect()
    }

    /// Get viewer count
    pub fn count(&self) -> usize {
        self.viewers.len()
    }

    /// Subscribe to collaboration events
    pub fn subscribe(&self) -> broadcast::Receiver<CollabEvent> {
        self.event_tx.subscribe()
    }

    /// Assign a color based on viewer index
    fn assign_color(index: usize) -> String {
        const COLORS: &[&str] = &[
            "#ff6b6b", // red
            "#4ecdc4", // teal
            "#ffe66d", // yellow
            "#95e1d3", // mint
            "#f38181", // coral
            "#aa96da", // lavender
            "#fcbad3", // pink
            "#a8d8ea", // sky blue
        ];
        COLORS[index % COLORS.len()].to_string()
    }

    // === CRDT Buffer Methods ===

    /// Get or create a CRDT document for a buffer
    pub fn get_or_create_buffer(&mut self, buffer_id: u64) -> &mut BufferCrdt {
        self.crdt_manager.get_or_create(buffer_id)
    }

    /// Apply a Neovim buffer change and broadcast to viewers
    pub fn apply_buffer_change(
        &mut self,
        buffer_id: u64,
        start_line: u32,
        end_line: u32,
        new_lines: Vec<String>,
    ) {
        let crdt = self.crdt_manager.get_or_create(buffer_id);
        let update = crdt.apply_nvim_delta(start_line, end_line, new_lines);
        let _ = self
            .event_tx
            .send(CollabEvent::BufferChanged { buffer_id, update });
    }

    /// Handle incoming sync message from a viewer
    pub fn handle_sync_message(
        &mut self,
        buffer_id: u64,
        msg: SyncMessage,
    ) -> anyhow::Result<Option<SyncMessage>> {
        let crdt = self.crdt_manager.get_or_create(buffer_id);
        let sync = CrdtSync::new(buffer_id);
        sync.handle_message(msg, crdt)
    }

    /// Get full buffer state for syncing a new viewer
    pub fn get_buffer_state(&mut self, buffer_id: u64) -> Vec<u8> {
        let crdt = self.crdt_manager.get_or_create(buffer_id);
        crdt.encode_state()
    }

    /// Sync all buffers to a new viewer
    pub fn sync_all_buffers_to_viewer(&mut self) {
        for buffer_id in self.crdt_manager.buffer_ids() {
            if let Some(crdt) = self.crdt_manager.get(buffer_id) {
                let state = crdt.encode_state();
                let _ = self
                    .event_tx
                    .send(CollabEvent::BufferSync { buffer_id, state });
            }
        }
    }

    // === P2P Signaling Methods ===

    /// Send WebRTC signal to a specific peer (unicast)
    pub fn send_signal(&self, from: &str, to: &str, signal_type: SignalType, payload: String) {
        let _ = self.event_tx.send(CollabEvent::WebRtcSignal {
            from: from.to_string(),
            to: to.to_string(),
            signal_type,
            payload,
        });
    }

    /// Send chat message (broadcast if to is None)
    pub fn send_chat(&self, from: &str, to: Option<&str>, message: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let _ = self.event_tx.send(CollabEvent::ChatMessage {
            from: from.to_string(),
            to: to.map(String::from),
            message,
            timestamp,
        });
    }

    /// Get all peer IDs for mesh connection
    pub fn get_peer_ids(&self) -> Vec<String> {
        self.viewers.keys().cloned().collect()
    }
}

impl Default for SessionViewers {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Global collaboration registry managing all sessions
#[derive(Debug, Default)]
pub struct CollaborationRegistry {
    sessions: HashMap<String, SessionViewers>,
}

impl CollaborationRegistry {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Get or create viewer registry for a session
    pub fn get_or_create(&mut self, session_id: &str) -> &mut SessionViewers {
        let id = session_id.to_string();
        self.sessions
            .entry(id.clone())
            .or_insert_with(|| SessionViewers::new(&id))
    }

    /// Get viewer registry for a session (if exists)
    pub fn get(&self, session_id: &str) -> Option<&SessionViewers> {
        self.sessions.get(session_id)
    }

    /// Get mutable viewer registry for a session (if exists)
    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut SessionViewers> {
        self.sessions.get_mut(session_id)
    }

    /// Remove a session's viewer registry
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }
}

/// Thread-safe collaboration registry
pub type SharedCollaborationRegistry = Arc<RwLock<CollaborationRegistry>>;

/// Create a new shared collaboration registry
pub fn create_registry() -> SharedCollaborationRegistry {
    Arc::new(RwLock::new(CollaborationRegistry::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_list_viewers() {
        let mut session = SessionViewers::new("test");
        session.add_viewer("v1".to_string(), Some("Alice".to_string()));
        session.add_viewer("v2".to_string(), None);

        let viewers = session.list_viewers();
        assert_eq!(viewers.len(), 2);
    }

    #[test]
    fn remove_viewer() {
        let mut session = SessionViewers::new("test");
        session.add_viewer("v1".to_string(), None);
        assert_eq!(session.count(), 1);

        session.remove_viewer("v1");
        assert_eq!(session.count(), 0);
    }

    #[test]
    fn color_assignment() {
        let mut session = SessionViewers::new("test");
        let v1 = session.add_viewer("v1".to_string(), None);
        let v2 = session.add_viewer("v2".to_string(), None);

        assert_ne!(v1.color, v2.color);
    }

    #[test]
    fn test_buffer_crdt_integration() {
        let mut session = SessionViewers::new("test");

        // Get or create buffer CRDT
        let crdt = session.get_or_create_buffer(1);
        crdt.set_content("hello world");

        // Apply change
        session.apply_buffer_change(1, 0, 1, vec!["hello rust".to_string()]);

        // Get state
        let state = session.get_buffer_state(1);
        assert!(!state.is_empty());
    }
}
