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

        // Legacy string-based messages
        if arr.len() >= 2 {
            return handle_legacy_message(session_id, manager, &arr).await;
        }
    }

    Ok(None)
}

/// Handle RPC request: [0, id, method, params] -> [1, id, error, result]
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
        "vfs_list" if vfs_manager.is_some() => {
            handle_vfs_list(vfs_manager.unwrap(), &params).await
        }
        "settings_get" => handle_settings_get(&params),
        "settings_set" => handle_settings_set(&params),
        "settings_all" => handle_settings_all(),
        "get_cwd_info" => handle_get_cwd_info(session_id, manager).await,
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
        vfs_handlers::handle_open_vfs(vfs_path, session, &vfs).await.ok()
    }.await;

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
        vfs_handlers::handle_write_vfs(vfs_path, bufnr, session, &vfs).await.ok()
    }.await;

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
            let value = store.get(key).map_or(Value::Nil, |v| Value::String(v.into()));
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
    let git_branch = git_root.as_ref().and_then(|root| git::get_current_branch(root));

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
        (Value::String("file".into()), Value::String(current_file.into())),
        (Value::String("backend".into()), Value::String(backend.into())),
        (
            Value::String("git_branch".into()),
            git_branch.map_or(Value::Nil, |b| Value::String(b.into())),
        ),
    ];

    Some((Value::Nil, Value::Map(map)))
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
            registry.resolve(id, Err(anyhow::anyhow!("{err_msg}"))).await;
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
                    let _ = session.rpc_call(
                        "nvim_input_mouse",
                        vec![
                            Value::String(button.into()),
                            Value::String(action.into()),
                            Value::String(modifier.into()),
                            Value::Integer(grid.into()),
                            Value::Integer(row.into()),
                            Value::Integer(col.into()),
                        ],
                    ).await;
                }
            }
            _ => {}
        }
    }
    Ok(None)
}
