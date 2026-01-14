//! Browser message command handlers
//!
//! Handles RPC requests, VFS operations, settings, and legacy messages.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use rmpv::Value;
use tokio::sync::RwLock;

use crate::git;
use crate::session::AsyncSessionManager;
use crate::settings::SettingsStore;
use crate::vfs::{FsRequestRegistry, VfsManager};
use crate::vfs_handlers;

/// Handle messages from browser
///
/// Protocol envelope: [type, ...payload]
/// - Type 0: RPC request [0, id, method, params] -> responds with [1, id, error, result]
/// - Type "input": fire-and-forget input ["input", keys]
/// - Type "resize": fire-and-forget resize ["resize", cols, rows]
///
/// Returns optional response bytes to send back to browser
#[allow(clippy::too_many_lines)]
#[allow(clippy::significant_drop_tightening)]
#[tracing::instrument(skip(manager, fs_registry, vfs_manager, data), level = "debug")]
pub async fn handle_browser_message(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    fs_registry: Option<&Arc<FsRequestRegistry>>,
    vfs_manager: Option<&Arc<RwLock<VfsManager>>>,
    data: Vec<u8>,
) -> Result<Option<Vec<u8>>> {
    let mut cursor = std::io::Cursor::new(data);
    let msg = rmpv::decode::read_value(&mut cursor)?;

    if let Value::Array(arr) = msg {
        if arr.is_empty() {
            return Ok(None);
        }

        // Check for RPC request (type 0)
        if let Value::Integer(msg_type) = &arr[0] {
            if msg_type.as_i64() == Some(0) && arr.len() >= 4 {
                return handle_rpc_request(session_id, manager, vfs_manager, &arr).await;
            }

            // Type 3: FS response from browser [3, id, ok, result]
            if msg_type.as_i64() == Some(3) && arr.len() >= 4 {
                return handle_fs_response(fs_registry, &arr).await;
            }

            // Type 2: Notification (used for clipboard response)
            if msg_type.as_i64() == Some(2) && arr.len() >= 3 {
                return handle_notification(session_id, manager, &arr).await;
            }
        }

        // Legacy string-based messages (including terminal and LLM)
        if arr.len() >= 1 {
            if let Value::String(method) = &arr[0] {
                // Check for terminal messages
                if let Some(m) = method.as_str() {
                    if m.starts_with("terminal_") {
                        // Terminal messages need output channel - pass None for now
                        // Real integration would pass the WebSocket sender
                        return handle_terminal_message(session_id, &arr, None).await;
                    }
                    if m.starts_with("llm_") {
                        // LLM messages handled separately via RPC
                    }
                }
            }
            if arr.len() >= 2 {
                return handle_legacy_message(session_id, manager, &arr).await;
            }
        }
    }

    Ok(None)
}

/// Handle RPC request: [0, id, method, params] -> [1, id, error, result]
#[tracing::instrument(skip(manager, vfs_manager, arr), fields(method), level = "debug")]
async fn handle_rpc_request(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    vfs_manager: Option<&Arc<RwLock<VfsManager>>>,
    arr: &[Value],
) -> Result<Option<Vec<u8>>> {
    let id = arr[1].clone();
    let method = arr[2].as_str().unwrap_or("");
    let params = if let Value::Array(p) = &arr[3] {
        p.clone()
    } else {
        vec![]
    };

    // Check for VFS/settings methods first (handle locally)
    let vfs_result = match method {
        "vfs_open" if vfs_manager.is_some() => {
            handle_vfs_open(session_id, manager, vfs_manager.unwrap(), &params).await
        }
        "vfs_write" if vfs_manager.is_some() => {
            handle_vfs_write(session_id, manager, vfs_manager.unwrap(), &params).await
        }
        "vfs_list" if vfs_manager.is_some() => handle_vfs_list(vfs_manager.unwrap(), &params).await,
        "settings_get" => handle_settings_get(&params),
        "settings_set" => handle_settings_set(&params),
        "settings_all" => handle_settings_all(),
        "get_cwd_info" => handle_get_cwd_info(session_id, manager).await,
        "get_session_id" => Some((Value::Nil, Value::String(session_id.to_string().into()))),
        "tool_exec" => handle_tool_exec(&params).await,
        _ => None, // Not a VFS/settings method, forward to Neovim
    };

    let (error, result) = if let Some(vfs_res) = vfs_result {
        vfs_res
    } else {
        // Execute RPC call on Neovim
        let mgr = manager.read().await;
        let session = mgr
            .get_session(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        match session.rpc_call(method, params).await {
            Ok(value) => (Value::Nil, value),
            Err(e) => (Value::String(e.to_string().into()), Value::Nil),
        }
    };

    // Build response: [1, id, error, result]
    let response = Value::Array(vec![Value::Integer(1.into()), id, error, result]);

    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, &response)?;

    Ok(Some(bytes))
}

/// Handle VFS open: vfs_open(vfs_path) -> bufnr
async fn handle_vfs_open(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    vfs_manager: &Arc<RwLock<VfsManager>>,
    params: &[Value],
) -> Option<(Value, Value)> {
    let vfs_path = params.first().and_then(|v| v.as_str()).unwrap_or("");
    let result = async {
        let mgr = manager.read().await;
        let session = mgr.get_session(session_id)?;
        let vfs = vfs_manager.read().await;
        vfs_handlers::handle_open_vfs(vfs_path, session, &vfs)
            .await
            .ok()
    }
    .await;

    result.map(|bufnr| (Value::Nil, Value::Integer(bufnr.into())))
}

/// Handle VFS write: vfs_write(vfs_path, bufnr) -> nil
async fn handle_vfs_write(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    vfs_manager: &Arc<RwLock<VfsManager>>,
    params: &[Value],
) -> Option<(Value, Value)> {
    let vfs_path = params.first().and_then(|v| v.as_str()).unwrap_or("");
    let bufnr = u32::try_from(params.get(1).and_then(Value::as_u64).unwrap_or(0)).unwrap_or(0);

    let result = async {
        let mgr = manager.read().await;
        let session = mgr.get_session(session_id)?;
        let vfs = vfs_manager.read().await;
        vfs_handlers::handle_write_vfs(vfs_path, bufnr, session, &vfs)
            .await
            .ok()
    }
    .await;

    result.map(|()| (Value::Nil, Value::Nil))
}

/// Handle VFS list: vfs_list(path, depth) -> tree entries
async fn handle_vfs_list(
    vfs_manager: &Arc<RwLock<VfsManager>>,
    params: &[Value],
) -> Option<(Value, Value)> {
    let path = params.first().and_then(|v| v.as_str()).unwrap_or("/");
    let depth = usize::try_from(params.get(1).and_then(Value::as_u64).unwrap_or(1)).unwrap_or(1);

    let vfs = vfs_manager.read().await;
    if let Ok(backend) = vfs.get_backend("local").await {
        Some(
            match vfs_handlers::handle_list_tree(path, depth, backend.as_ref()).await {
                Ok(tree) => (Value::Nil, vfs_handlers::tree_to_value(&tree)),
                Err(e) => (Value::String(e.to_string().into()), Value::Nil),
            },
        )
    } else {
        Some((Value::String("No local backend".into()), Value::Nil))
    }
}

/// Handle settings_get(key) -> value
fn handle_settings_get(params: &[Value]) -> Option<(Value, Value)> {
    let key = params.first().and_then(|v| v.as_str()).unwrap_or("");

    Some(match SettingsStore::new() {
        Ok(store) => {
            let value = store
                .get(key)
                .map_or(Value::Nil, |v| Value::String(v.into()));
            (Value::Nil, value)
        }
        Err(e) => (Value::String(e.to_string().into()), Value::Nil),
    })
}

/// Handle settings_set(key, value) -> bool
fn handle_settings_set(params: &[Value]) -> Option<(Value, Value)> {
    let key = params.first().and_then(|v| v.as_str()).unwrap_or("");
    let value = params.get(1).and_then(|v| v.as_str()).unwrap_or("");

    Some(match SettingsStore::new() {
        Ok(store) => match store.set(key, value) {
            Ok(()) => (Value::Nil, Value::Boolean(true)),
            Err(e) => (Value::String(e.to_string().into()), Value::Nil),
        },
        Err(e) => (Value::String(e.to_string().into()), Value::Nil),
    })
}

/// Handle settings_all() -> {key: value, ...}
fn handle_settings_all() -> Option<(Value, Value)> {
    Some(match SettingsStore::new() {
        Ok(store) => {
            let all = store.get_all();
            let map: Vec<(Value, Value)> = all
                .into_iter()
                .map(|(k, v)| (Value::String(k.into()), Value::String(v.into())))
                .collect();
            (Value::Nil, Value::Map(map))
        }
        Err(e) => (Value::String(e.to_string().into()), Value::Nil),
    })
}

/// Handle get_cwd_info() -> {cwd, file, backend, git_branch}
async fn handle_get_cwd_info(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
) -> Option<(Value, Value)> {
    let cwd_data = {
        let mgr = manager.read().await;
        let session = mgr.get_session(session_id)?;

        let cwd_result = session
            .rpc_call(
                "nvim_call_function",
                vec![Value::String("getcwd".into()), Value::Array(vec![])],
            )
            .await;

        let buf_result = session
            .rpc_call("nvim_buf_get_name", vec![Value::Integer(0.into())])
            .await;

        (cwd_result, buf_result)
    };

    let (cwd_result, buf_result) = cwd_data;

    let cwd = cwd_result
        .ok()
        .and_then(|v| v.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "~".to_string());

    let current_file = buf_result
        .ok()
        .and_then(|v| v.as_str().map(ToString::to_string))
        .unwrap_or_default();

    // Detect git root and branch
    let git_root = git::find_git_root(Path::new(&cwd));
    let git_branch = git_root
        .as_ref()
        .and_then(|root| git::get_current_branch(root));

    // Determine backend from file path
    let backend = if current_file.starts_with("vfs://browser/") {
        "browser"
    } else if current_file.starts_with("vfs://ssh/") {
        "ssh"
    } else {
        "local"
    };

    // Build response map
    let map = vec![
        (Value::String("cwd".into()), Value::String(cwd.into())),
        (
            Value::String("file".into()),
            Value::String(current_file.into()),
        ),
        (
            Value::String("backend".into()),
            Value::String(backend.into()),
        ),
        (
            Value::String("git_branch".into()),
            git_branch.map_or(Value::Nil, |b| Value::String(b.into())),
        ),
    ];

    Some((Value::Nil, Value::Map(map)))
}

/// Handle tool_exec(command, args, input) -> {stdout, stderr, exit_code}
/// Universal pipe for executing arbitrary CLI tools
async fn handle_tool_exec(params: &[Value]) -> Option<(Value, Value)> {
    use crate::pipe;

    let command = params.first().and_then(|v| v.as_str()).unwrap_or("");
    let args: Vec<String> = params
        .get(1)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let input = params.get(2).and_then(|v| v.as_str()).unwrap_or("");

    if command.is_empty() {
        return Some((Value::String("Missing command".into()), Value::Nil));
    }

    match pipe::run_pipe(command, &args, input, None).await {
        Ok(result) => {
            let map = vec![
                (
                    Value::String("stdout".into()),
                    Value::String(result.stdout.into()),
                ),
                (
                    Value::String("stderr".into()),
                    Value::String(result.stderr.into()),
                ),
                (
                    Value::String("exit_code".into()),
                    Value::Integer(result.exit_code.into()),
                ),
            ];
            Some((Value::Nil, Value::Map(map)))
        }
        Err(e) => Some((Value::String(e.to_string().into()), Value::Nil)),
    }
}

/// Handle FS response from browser: [3, id, ok, result]
async fn handle_fs_response(
    fs_registry: Option<&Arc<FsRequestRegistry>>,
    arr: &[Value],
) -> Result<Option<Vec<u8>>> {
    if let Some(registry) = fs_registry {
        let id = arr[1].as_u64().unwrap_or(0);
        let ok = arr[2].as_bool().unwrap_or(false);
        let result = &arr[3];

        if ok {
            registry.resolve(id, Ok(result.clone())).await;
        } else {
            let err_msg = result.as_str().unwrap_or("Unknown FS error");
            registry
                .resolve(id, Err(anyhow::anyhow!("{err_msg}")))
                .await;
        }
    }
    Ok(None)
}

/// Handle notification: [2, method, params]
async fn handle_notification(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    arr: &[Value],
) -> Result<Option<Vec<u8>>> {
    if let Value::String(method) = &arr[1] {
        if method.as_str() == Some("clipboard_read_response") {
            if let Value::Array(params) = &arr[2] {
                if params.len() >= 3 {
                    let req_id = u32::try_from(params[0].as_u64().unwrap_or(0)).unwrap_or(0);
                    let content = &params[1];
                    let response_session_id = params[2].as_str();

                    if response_session_id != Some(session_id) {
                        tracing::warn!(expected = %session_id, got = ?response_session_id, "Blocked clipboard response from wrong session");
                        return Ok(None);
                    }

                    let mgr = manager.read().await;
                    if let Some(session) = mgr.get_session(session_id) {
                        session.complete_request(req_id, content.clone());
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Handle legacy string-based messages: ["input", keys] or ["resize", cols, rows]
async fn handle_legacy_message(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    arr: &[Value],
) -> Result<Option<Vec<u8>>> {
    if let Value::String(method) = &arr[0] {
        let mgr = manager.read().await;
        let session = mgr
            .get_session(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        match method.as_str() {
            Some("input") => {
                if let Value::String(keys) = &arr[1] {
                    if let Some(key_str) = keys.as_str() {
                        session.input(key_str).await?;
                    }
                }
            }
            Some("resize") => {
                if arr.len() >= 3 {
                    if let (Value::Integer(cols), Value::Integer(rows)) = (&arr[1], &arr[2]) {
                        let cols = cols.as_i64().unwrap_or(80);
                        let rows = rows.as_i64().unwrap_or(24);
                        session.resize(cols, rows).await?;
                    }
                }
            }

            Some("mouse") => {
                // ["mouse", button, action, modifier, row, col]
                if arr.len() >= 6 {
                    let button = arr[1].as_str().unwrap_or("left");
                    let action = arr[2].as_str().unwrap_or("press");
                    let modifier = arr[3].as_str().unwrap_or("");
                    let row = arr[4].as_i64().unwrap_or(0);
                    let col = arr[5].as_i64().unwrap_or(0);

                    // grid=0 for global coordinates
                    let _ = session
                        .rpc_call(
                            "nvim_input_mouse",
                            vec![
                                Value::String(button.into()),
                                Value::String(action.into()),
                                Value::String(modifier.into()),
                                Value::Integer(0.into()),
                                Value::Integer(row.into()),
                                Value::Integer(col.into()),
                            ],
                        )
                        .await;
                }
            }
            Some("scroll") => {
                // ["scroll", direction, modifier, row, col]
                if arr.len() >= 5 {
                    let direction = arr[1].as_str().unwrap_or("up");
                    let modifier = arr[2].as_str().unwrap_or("");
                    let row = arr[3].as_i64().unwrap_or(0);
                    let col = arr[4].as_i64().unwrap_or(0);

                    // button="wheel", action=direction
                    let _ = session
                        .rpc_call(
                            "nvim_input_mouse",
                            vec![
                                Value::String("wheel".into()),
                                Value::String(direction.into()),
                                Value::String(modifier.into()),
                                Value::Integer(0.into()),
                                Value::Integer(row.into()),
                                Value::Integer(col.into()),
                            ],
                        )
                        .await;
                }
            }
            Some("input_mouse") => {
                // Multigrid mouse input: ["input_mouse", button, action, modifier, grid, row, col]
                if arr.len() >= 7 {
                    let button = arr[1].as_str().unwrap_or("left");
                    let action = arr[2].as_str().unwrap_or("press");
                    let modifier = arr[3].as_str().unwrap_or("");
                    let grid = arr[4].as_i64().unwrap_or(1);
                    let row = arr[5].as_i64().unwrap_or(0);
                    let col = arr[6].as_i64().unwrap_or(0);

                    // Call nvim_input_mouse(button, action, modifier, grid, row, col)
                    let _ = session
                        .rpc_call(
                            "nvim_input_mouse",
                            vec![
                                Value::String(button.into()),
                                Value::String(action.into()),
                                Value::String(modifier.into()),
                                Value::Integer(grid.into()),
                                Value::Integer(row.into()),
                                Value::Integer(col.into()),
                            ],
                        )
                        .await;
                }
            }
            Some("file_drop") => {
                // ["file_drop", filename, data]
                if arr.len() >= 3 {
                    // 1. Parse Args
                    let filename = arr[1].as_str().unwrap_or("dropped_file");
                    let data = if let Value::Binary(bytes) = &arr[2] {
                        bytes.clone()
                    } else {
                        vec![]
                    };

                    if !data.is_empty() {
                        // 2. Get CWD from Neovim to save file in correct location
                        let cwd_res = session
                            .rpc_call(
                                "nvim_call_function",
                                vec![Value::String("getcwd".into()), Value::Array(vec![])],
                            )
                            .await;

                        let cwd = cwd_res
                            .ok()
                            .and_then(|v| v.as_str().map(ToString::to_string))
                            .unwrap_or_else(|| ".".to_string());

                        let path = std::path::Path::new(&cwd).join(filename);

                        // 3. Write file to disk
                        // Note: Using standard fs for simplicity, but in async context tokio::fs is better
                        // multithreaded runtime makes this acceptable for small files
                        if let Err(e) = std::fs::write(&path, &data) {
                            eprintln!("Failed to write dropped file: {e}");
                        } else {
                            eprintln!("Saved dropped file to: {}", path.display());

                            // 4. Open file in Neovim
                            let _ = session
                                .rpc_call(
                                    "nvim_command",
                                    vec![Value::String(format!("edit {}", path.display()).into())],
                                )
                                .await;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

// ============================================================================
// Terminal PTY message handlers
// ============================================================================

use crate::terminal::TerminalManager;
use once_cell::sync::Lazy;
use std::sync::Mutex as StdMutex;

/// Global terminal manager (terminals are tied to sessions)
static TERMINAL_MANAGER: Lazy<StdMutex<TerminalManager>> =
    Lazy::new(|| StdMutex::new(TerminalManager::new()));

/// Handle terminal-related messages from browser
/// Returns optional output message to send back, plus spawns output streaming task
pub async fn handle_terminal_message(
    session_id: &str,
    arr: &[Value],
    output_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
) -> Result<Option<Vec<u8>>> {
    if let Value::String(method) = &arr[0] {
        match method.as_str() {
            Some("terminal_spawn") => {
                // ["terminal_spawn", cols, rows]
                let cols = arr.get(1).and_then(Value::as_u64).unwrap_or(80) as u16;
                let rows = arr.get(2).and_then(Value::as_u64).unwrap_or(24) as u16;

                let result = {
                    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
                    mgr.create_session(session_id, cols, rows)
                };

                match result {
                    Ok(()) => {
                        // Start streaming output to browser
                        if let Some(tx) = output_tx {
                            let sid = session_id.to_string();
                            tokio::spawn(async move {
                                stream_terminal_output(&sid, tx).await;
                            });
                        }

                        // Send success response
                        let response = Value::Array(vec![
                            Value::String("terminal_spawned".into()),
                            Value::Boolean(true),
                        ]);
                        let mut bytes = Vec::new();
                        rmpv::encode::write_value(&mut bytes, &response)?;
                        return Ok(Some(bytes));
                    }
                    Err(e) => {
                        tracing::error!("Failed to spawn terminal: {}", e);
                        let response = Value::Array(vec![
                            Value::String("terminal_spawned".into()),
                            Value::Boolean(false),
                            Value::String(e.to_string().into()),
                        ]);
                        let mut bytes = Vec::new();
                        rmpv::encode::write_value(&mut bytes, &response)?;
                        return Ok(Some(bytes));
                    }
                }
            }
            Some("terminal_input") => {
                // [\"terminal_input\", data_bytes]
                let session_arc = {
                    let mgr = TERMINAL_MANAGER.lock().unwrap();
                    mgr.get_session(session_id)
                };

                if let Some(session) = session_arc {
                    let data_vec = if let Some(Value::Binary(data)) = arr.get(1) {
                        Some(data.clone())
                    } else if let Some(Value::String(s)) = arr.get(1) {
                        s.as_str().map(|t| t.as_bytes().to_vec())
                    } else {
                        None
                    };

                    if let Some(data) = data_vec {
                        let sess = session.lock().await;
                        let _ = sess.send_input(data).await;
                    }
                }
            }
            Some("terminal_close") => {
                // ["terminal_close"]
                let mut mgr = TERMINAL_MANAGER.lock().unwrap();
                mgr.remove_session(session_id);
                tracing::info!("Terminal closed for session: {}", session_id);
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Stream terminal output to browser via WebSocket
async fn stream_terminal_output(session_id: &str, tx: tokio::sync::mpsc::Sender<Vec<u8>>) {
    use crate::terminal::TerminalSession;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let session: Option<Arc<Mutex<TerminalSession>>> = {
        let mgr = TERMINAL_MANAGER.lock().unwrap();
        mgr.get_session(session_id)
    };

    if let Some(session) = session {
        let mut sess = session.lock().await;
        if let Some(mut rx) = sess.take_output_rx() {
            drop(sess);
            while let Some(data) = rx.recv().await {
                // Send as ["terminal_output", binary_data]
                let msg = Value::Array(vec![
                    Value::String("terminal_output".into()),
                    Value::Binary(data),
                ]);
                let mut bytes = Vec::new();
                if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                    if tx.send(bytes).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}
