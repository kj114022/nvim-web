use anyhow::{Result, bail};
use super::{VfsBackend, FileStat};
use rmpv::Value;
use std::sync::mpsc::Sender;

/// Browser-based VFS backend using OPFS (Origin Private File System)
/// 
/// This backend delegates storage to the browser's OPFS via WebSocket RPC.
/// The host owns VFS semantics; the browser owns storage.
/// 
/// Communication: Uses channel-based message passing to WS thread.
/// This matches the existing Neovim RPC pattern and avoids trait object issues.
pub struct BrowserFsBackend {
    pub namespace: String,
    ws_tx: Sender<Vec<u8>>,  // Channel to WS writer thread
}

impl BrowserFsBackend {
    /// Create a new BrowserFs backend for the given namespace
    /// 
    /// Namespace separates different projects/contexts in OPFS.
    /// Example: "default", "demo", "project-name"
    pub fn new(namespace: impl Into<String>, ws_tx: Sender<Vec<u8>>) -> Self {
        Self {
            namespace: namespace.into(),
            ws_tx,
        }
    }
    
    /// Make a blocking FS call via WebSocket
    /// 
    /// Sends message through channel, waits for response via rpc_sync registry.
    fn fs_call(&self, op_type: &str, path: &str, data: Option<&[u8]>) -> Result<Value> {
        // Build FS request parameters
        let params = match op_type {
            "fs_read" => vec![
                Value::String("fs_read".into()),
                Value::String(self.namespace.clone().into()),
                Value::String(path.into()),
            ],
            "fs_write" => vec![
                Value::String("fs_write".into()),
                Value::String(self.namespace.clone().into()),
                Value::String(path.into()),
                Value::Binary(data.unwrap_or(&[]).to_vec()),
            ],
            "fs_stat" => vec![
                Value::String("fs_stat".into()),
                Value::String(self.namespace.clone().into()),
                Value::String(path.into()),
            ],
            "fs_list" => vec![
                Value::String("fs_list".into()),
                Value::String(self.namespace.clone().into()),
                Value::String(path.into()),
            ],
            _ => bail!("unknown fs operation: {}", op_type),
        };
        
        // Use rpc_sync infrastructure for ID generation and blocking
        crate::rpc_sync::fs_rpc_call_via_channel(&self.ws_tx, op_type, params)
    }
}

impl VfsBackend for BrowserFsBackend {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let response = self.fs_call("fs_read", path, None)?;
        
        // Extract binary data from response
        if let Value::Binary(bytes) = response {
            Ok(bytes)
        } else {
            bail!("Expected binary data from fs_read");
        }
    }

    fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        self.fs_call("fs_write", path, Some(data))?;
        Ok(())
    }

    fn stat(&self, path: &str) -> Result<FileStat> {
        let response = self.fs_call("fs_stat", path, None)?;
        
        // Parse stat response: { is_file: bool, is_dir: bool, size: number }
        if let Value::Map(map) = response {
            let mut is_file = false;
            let mut is_dir = false;
            let mut size = 0u64;
            
            for (k, v) in map {
                if let Value::String(key) = k {
                    match key.as_str() {
                        Some("is_file") => {
                            if let Value::Boolean(b) = v {
                                is_file = b;
                            }
                        }
                        Some("is_dir") => {
                            if let Value::Boolean(b) = v {
                                is_dir = b;
                            }
                        }
                        Some("size") => {
                            if let Value::Integer(n) = v {
                                size = n.as_u64().unwrap_or(0);
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            Ok(FileStat { is_file, is_dir, size })
        } else {
            bail!("Expected map from fs_stat");
        }
    }

    fn list(&self, path: &str) -> Result<Vec<String>> {
        let response = self.fs_call("fs_list", path, None)?;
        
        // Parse list response: array of strings
        if let Value::Array(arr) = response {
            let names = arr.into_iter()
                .filter_map(|v| {
                    if let Value::String(s) = v {
                        s.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            Ok(names)
        } else {
            bail!("Expected array from fs_list");
        }
    }
}


