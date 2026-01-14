//! CRDT sync protocol
//!
//! Implements the y-sync protocol for synchronizing CRDT state
//! between the host and browser clients.

use serde::{Deserialize, Serialize};

/// Sync message types for CRDT collaboration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncMessage {
    /// Step 1: Client sends its state vector
    #[serde(rename = "sync1")]
    SyncStep1 { state_vector: Vec<u8> },

    /// Step 2: Server responds with missing updates
    #[serde(rename = "sync2")]
    SyncStep2 { update: Vec<u8> },

    /// Incremental update from either side
    #[serde(rename = "update")]
    Update { update: Vec<u8> },

    /// Awareness update (cursor positions, presence)
    #[serde(rename = "awareness")]
    Awareness { data: Vec<u8> },
}

/// CRDT sync handler for a session
pub struct CrdtSync {
    /// Buffer ID this sync is for
    buffer_id: u64,
}

impl CrdtSync {
    /// Create a new sync handler for a buffer
    pub fn new(buffer_id: u64) -> Self {
        Self { buffer_id }
    }

    /// Get buffer ID
    pub fn buffer_id(&self) -> u64 {
        self.buffer_id
    }

    /// Handle incoming sync message from client
    ///
    /// Returns optional response message to send back.
    pub fn handle_message(
        &self,
        msg: SyncMessage,
        crdt: &mut super::BufferCrdt,
    ) -> anyhow::Result<Option<SyncMessage>> {
        match msg {
            SyncMessage::SyncStep1 { state_vector } => {
                // Client wants to sync - send them what they're missing
                let update = crdt.encode_diff(&state_vector)?;
                Ok(Some(SyncMessage::SyncStep2 { update }))
            }
            SyncMessage::SyncStep2 { update } => {
                // We received missing updates from the server
                crdt.apply_update(&update)?;
                Ok(None)
            }
            SyncMessage::Update { update } => {
                // Incremental update - apply and optionally broadcast
                crdt.apply_update(&update)?;
                Ok(None)
            }
            SyncMessage::Awareness { .. } => {
                // Awareness updates are handled separately (cursor positions)
                // Integration point: SessionViewers.update_cursor() in collaboration.rs
                Ok(None)
            }
        }
    }

    /// Create a SyncStep1 message for initial sync
    pub fn create_sync_step1(crdt: &super::BufferCrdt) -> SyncMessage {
        SyncMessage::SyncStep1 {
            state_vector: crdt.state_vector(),
        }
    }

    /// Create an Update message from a CRDT change
    pub fn create_update(update: Vec<u8>) -> SyncMessage {
        SyncMessage::Update { update }
    }

    /// Create a full state sync message (for new clients)
    pub fn create_full_sync(crdt: &super::BufferCrdt) -> SyncMessage {
        SyncMessage::SyncStep2 {
            update: crdt.encode_state(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::BufferCrdt;
    use yrs::updates::encoder::Encode;

    #[test]
    fn test_sync_step1_response() {
        let mut crdt1 = BufferCrdt::new(1);
        crdt1.set_content("hello world");

        let sync = CrdtSync::new(1);

        // Client sends empty state vector (new client)
        let client_sv = yrs::StateVector::default().encode_v1();
        let msg = SyncMessage::SyncStep1 {
            state_vector: client_sv,
        };

        let response = sync.handle_message(msg, &mut crdt1).unwrap();
        assert!(matches!(response, Some(SyncMessage::SyncStep2 { .. })));

        // Apply the update to a new client CRDT
        if let Some(SyncMessage::SyncStep2 { update }) = response {
            let mut crdt2 = BufferCrdt::new(1);
            crdt2.apply_update(&update).unwrap();
            assert_eq!(crdt2.get_content(), "hello world");
        }
    }

    #[test]
    fn test_incremental_update() {
        let mut crdt1 = BufferCrdt::new(1);
        crdt1.set_content("hello");

        // Sync to client
        let state = crdt1.encode_state();
        let mut crdt2 = BufferCrdt::new(1);
        crdt2.apply_update(&state).unwrap();

        // Server makes a change
        let update = crdt1.apply_nvim_delta(0, 1, vec!["hello world".to_string()]);

        // Send as incremental update
        let sync = CrdtSync::new(1);
        let msg = SyncMessage::Update {
            update: update.clone(),
        };
        sync.handle_message(msg, &mut crdt2).unwrap();

        assert_eq!(crdt2.get_content(), crdt1.get_content());
    }
}
