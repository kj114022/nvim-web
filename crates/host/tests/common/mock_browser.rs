//! Mock browser filesystem for testing
#![allow(dead_code)] // Test utilities may not all be used in every test

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use rmpv::Value;

/// Mock browser filesystem that simulates OPFS over WebSocket
///
/// This runs in a separate thread and responds to FS requests with in-memory storage.
/// It decodes incoming msgpack FS requests, performs operations on a HashMap,
/// and sends back properly formatted responses.
///
/// This is test-only infrastructure to prove BrowserFsBackend works without
/// requiring a real browser or OPFS implementation.
pub struct MockBrowserFs {
    storage: HashMap<String, Vec<u8>>,
}

impl MockBrowserFs {
    /// Spawn a mock browser FS service that responds to FS requests
    ///
    /// Receives FS request messages from host, executes them on in-memory storage,
    /// sends responses back with correct message IDs for condvar wakeup.
    pub fn spawn(rx: Receiver<Vec<u8>>, tx: Sender<Vec<u8>>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut fs = MockBrowserFs {
                storage: HashMap::new(),
            };

            for msg_bytes in rx {
                if let Ok(response) = fs.handle_request(&msg_bytes) {
                    if tx.send(response).is_err() {
                        break; // Channel closed
                    }
                }
            }
        })
    }

    fn handle_request(&mut self, msg_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        // Decode msgpack request: [2, id, [op_type, ns, path, data?]]
        let mut cursor = std::io::Cursor::new(msg_bytes);
        let msg = rmpv::decode::read_value(&mut cursor)?;

        if let Value::Array(arr) = msg {
            if arr.len() >= 3 {
                // Extract message ID
                let id = if let Value::Integer(id_val) = &arr[1] {
                    id_val.as_u64().unwrap_or(0)
                } else {
                    0
                };

                // Extract parameters
                if let Value::Array(params) = &arr[2] {
                    if params.is_empty() {
                        return self.error_response(id, "empty params");
                    }

                    // Extract operation type
                    let op_type = if let Value::String(s) = &params[0] {
                        s.as_str().unwrap_or("")
                    } else {
                        ""
                    };

                    // Extract namespace and path
                    let ns = if params.len() > 1 {
                        if let Value::String(s) = &params[1] {
                            s.as_str().unwrap_or("")
                        } else {
                            ""
                        }
                    } else {
                        ""
                    };

                    let path = if params.len() > 2 {
                        if let Value::String(s) = &params[2] {
                            s.as_str().unwrap_or("")
                        } else {
                            ""
                        }
                    } else {
                        ""
                    };

                    // Build full key: namespace/path
                    let key = format!("{}/{}", ns, path);

                    // Execute operation
                    return match op_type {
                        "fs_read" => self.handle_read(id, &key),
                        "fs_write" => {
                            if params.len() > 3 {
                                if let Value::Binary(data) = &params[3] {
                                    self.handle_write(id, &key, data)
                                } else {
                                    self.error_response(id, "invalid data")
                                }
                            } else {
                                self.error_response(id, "missing data")
                            }
                        }
                        "fs_stat" => self.handle_stat(id, &key),
                        "fs_list" => self.handle_list(id, ns),
                        _ => self.error_response(id, "unknown operation"),
                    };
                }
            }
        }

        self.error_response(0, "invalid request")
    }

    fn handle_read(&self, id: u64, key: &str) -> anyhow::Result<Vec<u8>> {
        if let Some(data) = self.storage.get(key) {
            self.success_response(id, Value::Binary(data.clone()))
        } else {
            self.error_response(id, "file not found")
        }
    }

    fn handle_write(&mut self, id: u64, key: &str, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.storage.insert(key.to_string(), data.to_vec());
        self.success_response(id, Value::Nil)
    }

    fn handle_stat(&self, id: u64, key: &str) -> anyhow::Result<Vec<u8>> {
        if let Some(data) = self.storage.get(key) {
            let stat = Value::Map(vec![
                (Value::String("is_file".into()), Value::Boolean(true)),
                (Value::String("is_dir".into()), Value::Boolean(false)),
                (
                    Value::String("size".into()),
                    Value::Integer((data.len() as u64).into()),
                ),
            ]);
            self.success_response(id, stat)
        } else {
            self.error_response(id, "not found")
        }
    }

    fn handle_list(&self, id: u64, ns: &str) -> anyhow::Result<Vec<u8>> {
        let prefix = format!("{}/", ns);
        let names: Vec<Value> = self
            .storage
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| {
                let name = k.strip_prefix(&prefix).unwrap_or(k);
                Value::String(name.into())
            })
            .collect();

        self.success_response(id, Value::Array(names))
    }

    fn success_response(&self, id: u64, result: Value) -> anyhow::Result<Vec<u8>> {
        // Format: [3, id, true, result]
        let response = Value::Array(vec![
            Value::Integer(3.into()),
            Value::Integer(id.into()),
            Value::Boolean(true),
            result,
        ]);

        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &response)?;
        Ok(buf)
    }

    fn error_response(&self, id: u64, error: &str) -> anyhow::Result<Vec<u8>> {
        // Format: [3, id, false, error]
        let response = Value::Array(vec![
            Value::Integer(3.into()),
            Value::Integer(id.into()),
            Value::Boolean(false),
            Value::String(error.into()),
        ]);

        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &response)?;
        Ok(buf)
    }
}
