//! Browser-based VFS backend using OPFS (Origin Private File System)
//!
//! This backend communicates with the browser via WebSocket to access OPFS storage.
//! The browser-side handler is in ui/fs/opfs.ts.
//!
//! Protocol:
//! - Request:  [2, id, [operation, namespace, path, data?]]
//! - Response: [3, id, ok, result]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;
use rmpv::Value;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time::{timeout, Duration};

use super::{FileStat, VfsBackend};

/// Request ID counter for correlating requests and responses
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique request ID
fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// Pending FS request awaiting response
type PendingRequest = oneshot::Sender<Result<Value>>;

/// Registry for pending FS requests
///
/// Shared between `BrowserFsBackend` and the WebSocket handler.
/// When a request is sent, a oneshot channel is created and stored.
/// When a response arrives, the corresponding sender is resolved.
#[derive(Default)]
pub struct FsRequestRegistry {
    pending: Mutex<HashMap<u64, PendingRequest>>,
}

impl FsRequestRegistry {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending request and return the receiver
    pub async fn register(&self, id: u64) -> oneshot::Receiver<Result<Value>> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        rx
    }

    /// Resolve a pending request with a response
    pub async fn resolve(&self, id: u64, result: Result<Value>) {
        let tx = self.pending.lock().await.remove(&id);
        if let Some(tx) = tx {
            let _ = tx.send(result);
        }
    }

    /// Cancel a pending request (e.g., on timeout)
    pub async fn cancel(&self, id: u64) {
        self.pending.lock().await.remove(&id);
    }
}

/// Browser-based VFS backend using OPFS
///
/// Communicates with the browser via WebSocket RPC to access OPFS storage.
/// Requires a connection to the WebSocket layer for sending requests.
pub struct BrowserFsBackend {
    /// Namespace within OPFS (e.g., "project1", "scratch")
    pub namespace: String,
    /// Channel to send FS requests to WebSocket
    request_tx: broadcast::Sender<Vec<u8>>,
    /// Registry for pending requests
    registry: Arc<FsRequestRegistry>,
}

impl BrowserFsBackend {
    /// Create a new `BrowserFs` backend
    ///
    /// # Arguments
    /// * `namespace` - OPFS namespace (directory root)
    /// * `request_tx` - Channel to send FS requests to WebSocket layer
    /// * `registry` - Shared registry for request/response correlation
    pub fn new(
        namespace: impl Into<String>,
        request_tx: broadcast::Sender<Vec<u8>>,
        registry: Arc<FsRequestRegistry>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            request_tx,
            registry,
        }
    }

    /// Send an FS request and wait for response
    async fn send_request(
        &self,
        operation: &str,
        path: &str,
        data: Option<&[u8]>,
    ) -> Result<Value> {
        let id = next_request_id();

        // Build request: [2, id, [operation, namespace, path, data?]]
        let mut params = vec![
            Value::String(operation.into()),
            Value::String(self.namespace.clone().into()),
            Value::String(path.into()),
        ];

        if let Some(bytes) = data {
            params.push(Value::Binary(bytes.to_vec()));
        }

        let request = Value::Array(vec![
            Value::Integer(2.into()), // FS request type
            Value::Integer(id.into()),
            Value::Array(params),
        ]);

        // Register for response
        let rx = self.registry.register(id).await;

        // Encode and send
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &request)?;

        if self.request_tx.send(bytes).is_err() {
            self.registry.cancel(id).await;
            bail!("WebSocket not connected");
        }

        // Wait for response with timeout
        match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => bail!("Request cancelled"),
            Err(_) => {
                self.registry.cancel(id).await;
                bail!("Request timed out after 30s")
            }
        }
    }
}

#[async_trait]
impl VfsBackend for BrowserFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let result = self.send_request("fs_read", path, None).await?;

        match result {
            Value::Binary(data) => Ok(data),
            _ => bail!("Unexpected response type for read"),
        }
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        self.send_request("fs_write", path, Some(data)).await?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let result = self.send_request("fs_stat", path, None).await?;

        // Parse stat result: {is_file: bool, is_dir: bool, size: u64}
        if let Value::Map(entries) = result {
            let mut is_file = false;
            let mut is_dir = false;
            let mut size = 0u64;

            for (key, value) in entries {
                if let Value::String(k) = key {
                    match k.as_str() {
                        Some("is_file") => is_file = value.as_bool().unwrap_or(false),
                        Some("is_dir") => is_dir = value.as_bool().unwrap_or(false),
                        Some("size") => size = value.as_u64().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            Ok(FileStat {
                is_file,
                is_dir,
                size,
                created: None,
                modified: None,
                readonly: false,
            })
        } else {
            bail!("Unexpected response type for stat")
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let result = self.send_request("fs_list", path, None).await?;

        if let Value::Array(items) = result {
            let names: Vec<String> = items
                .into_iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect();
            Ok(names)
        } else {
            bail!("Unexpected response type for list")
        }
    }
}
