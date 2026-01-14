//! Buffer-level CRDT document
//!
//! Wraps a Y.Doc with text content representing a single Neovim buffer.
//! Provides methods to apply Neovim buffer changes and extract content.

use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, Text, Transact, Update};

/// CRDT document for a single buffer
#[derive(Debug)]
pub struct BufferCrdt {
    /// The Y.Doc containing the buffer content
    doc: Doc,
    /// Buffer ID (for logging)
    buffer_id: u64,
    /// Version counter for change tracking
    version: u64,
}

impl BufferCrdt {
    /// Create a new CRDT document for a buffer
    pub fn new(buffer_id: u64) -> Self {
        let doc = Doc::new();
        // Pre-create the text type
        let _ = doc.get_or_insert_text("content");

        Self {
            doc,
            buffer_id,
            version: 0,
        }
    }

    /// Get the buffer ID
    pub fn buffer_id(&self) -> u64 {
        self.buffer_id
    }

    /// Get current version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get the text reference (internal helper)
    fn text(&self) -> yrs::TextRef {
        self.doc.get_or_insert_text("content")
    }

    /// Initialize with content (for new buffers)
    pub fn set_content(&mut self, content: &str) {
        let text = self.text();
        let mut txn = self.doc.transact_mut();
        // Clear existing content
        let len = text.get_string(&txn).len() as u32;
        if len > 0 {
            text.remove_range(&mut txn, 0, len);
        }
        // Insert new content
        text.insert(&mut txn, 0, content);
        self.version += 1;
    }

    /// Get current content as a string
    pub fn get_content(&self) -> String {
        let text = self.text();
        let txn = self.doc.transact();
        text.get_string(&txn)
    }

    /// Get current content as lines
    pub fn get_lines(&self) -> Vec<String> {
        self.get_content().lines().map(String::from).collect()
    }

    /// Apply a line-based delta from Neovim
    ///
    /// This is called when Neovim notifies us of buffer changes via `nvim_buf_attach`.
    ///
    /// # Arguments
    /// * `start_line` - First line affected (0-indexed)
    /// * `end_line` - Last line affected (exclusive, before change)
    /// * `new_lines` - New content for the affected range
    pub fn apply_nvim_delta(
        &mut self,
        start_line: u32,
        end_line: u32,
        new_lines: Vec<String>,
    ) -> Vec<u8> {
        let text = self.text();
        let mut txn = self.doc.transact_mut();
        let content = text.get_string(&txn);

        // Calculate character offsets from line numbers
        let (start_offset, end_offset) =
            Self::line_range_to_offsets(&content, start_line, end_line);

        // Remove old content
        let delete_len = (end_offset - start_offset) as u32;
        if delete_len > 0 {
            text.remove_range(&mut txn, start_offset as u32, delete_len);
        }

        // Insert new content
        let new_content = new_lines.join("\n");
        if !new_content.is_empty() {
            text.insert(&mut txn, start_offset as u32, &new_content);
            // Add trailing newline if this wasn't the last line
            if !new_lines.is_empty() {
                text.insert(&mut txn, (start_offset + new_content.len()) as u32, "\n");
            }
        }

        self.version += 1;

        // Return the update for syncing
        txn.encode_update_v1()
    }

    /// Apply an update from a remote client
    pub fn apply_update(&mut self, update: &[u8]) -> anyhow::Result<()> {
        let update = Update::decode_v1(update)?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update)?;
        self.version += 1;
        Ok(())
    }

    /// Get the current state vector for sync
    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// Compute diff from a state vector (for sync step 2)
    pub fn encode_diff(&self, state_vector: &[u8]) -> anyhow::Result<Vec<u8>> {
        let sv = yrs::StateVector::decode_v1(state_vector)?;
        let txn = self.doc.transact();
        Ok(txn.encode_diff_v1(&sv))
    }

    /// Encode full state as update
    pub fn encode_state(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    }

    /// Convert line range to character offsets
    fn line_range_to_offsets(content: &str, start_line: u32, end_line: u32) -> (usize, usize) {
        let mut start_offset = 0;
        let mut end_offset = content.len();
        let mut current_line = 0;

        for (idx, ch) in content.char_indices() {
            if current_line == start_line && start_offset == 0 {
                start_offset = idx;
            }
            if ch == '\n' {
                current_line += 1;
                if current_line == end_line {
                    end_offset = idx + 1; // Include the newline
                    break;
                }
            }
        }

        // Handle start_line == 0
        if start_line == 0 {
            start_offset = 0;
        }

        (start_offset, end_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let crdt = BufferCrdt::new(1);
        assert_eq!(crdt.buffer_id(), 1);
        assert_eq!(crdt.get_content(), "");
    }

    #[test]
    fn test_set_content() {
        let mut crdt = BufferCrdt::new(1);
        crdt.set_content("hello\nworld\n");
        assert_eq!(crdt.get_content(), "hello\nworld\n");
        assert_eq!(crdt.get_lines(), vec!["hello", "world"]);
    }

    #[test]
    fn test_apply_nvim_delta() {
        let mut crdt = BufferCrdt::new(1);
        crdt.set_content("line1\nline2\nline3\n");

        // Replace line2 with "new line"
        crdt.apply_nvim_delta(1, 2, vec!["new line".to_string()]);

        let lines = crdt.get_lines();
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "new line");
        assert_eq!(lines[2], "line3");
    }

    #[test]
    fn test_sync_roundtrip() {
        let mut crdt1 = BufferCrdt::new(1);
        crdt1.set_content("hello world");

        // Get state for syncing
        let state = crdt1.encode_state();

        // Create another CRDT and apply the state
        let mut crdt2 = BufferCrdt::new(1);
        crdt2.apply_update(&state).unwrap();

        assert_eq!(crdt2.get_content(), "hello world");
    }

    #[test]
    fn test_concurrent_edits() {
        let mut crdt1 = BufferCrdt::new(1);
        let mut crdt2 = BufferCrdt::new(1);

        // Both start with same content
        crdt1.set_content("hello");
        let state = crdt1.encode_state();
        crdt2.apply_update(&state).unwrap();

        // crdt1 appends " world"
        let update1 = crdt1.apply_nvim_delta(0, 1, vec!["hello world".to_string()]);

        // crdt2 appends "!" (before seeing crdt1's change)
        let update2 = crdt2.apply_nvim_delta(0, 1, vec!["hello!".to_string()]);

        // Apply each other's updates
        crdt1.apply_update(&update2).unwrap();
        crdt2.apply_update(&update1).unwrap();

        // Both should converge to the same content
        assert_eq!(crdt1.get_content(), crdt2.get_content());
    }
}
