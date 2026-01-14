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
