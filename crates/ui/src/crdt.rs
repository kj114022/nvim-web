use yrs::{Doc, ReadTxn, Transact, Update};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

/// Client-side CRDT document wrapper
pub struct CrdtClient {
    doc: Doc,
    text_name: String,
}

impl CrdtClient {
    pub fn new(_buffer_id: u64) -> Self {
        let doc = Doc::new();
        let text_name = "content".to_string(); // Must match host
        
        Self {
            doc,
            text_name,
        }
    }

    /// Apply binary update from host
    /// Apply binary update from host
    pub fn apply_update(&mut self, update: &[u8]) -> Result<(), String> {
        let update = Update::decode_v1(update).map_err(|e| e.to_string())?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update);
        Ok(())
    }

    /// Get current state vector for sync
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }
}
