//! Async WebSocket server using tokio-tungstenite
//!
//! Handles multiple concurrent connections with async session management.
//! Includes origin validation and session reconnection support.

use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use rmpv::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

use crate::session::AsyncSessionManager;

/// Allowed origins for WebSocket connections
/// Only localhost is allowed by default for security
const ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost",
    "http://127.0.0.1",
    "https://localhost",
    "https://127.0.0.1",
];

/// Connection metadata extracted during WebSocket handshake
#[derive(Debug, Clone, Default)]
struct ConnectionInfo {
    session_id: Option<String>,
    origin: Option<String>,
    origin_valid: bool,
}

/// Parse session ID from URI query string
/// Format: /?session=<id> or /?session=new
fn parse_session_id_from_uri(uri: &str) -> Option<String> {
    if let Some(query_start) = uri.find('?') {
        let query = &uri[query_start + 1..];
        for param in query.split('&') {
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key == "session" {
                    if value == "new" {
                        return None; // Explicit request for new session
                    }
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Validate origin header against whitelist
fn validate_origin(origin: &str) -> bool {
    ALLOWED_ORIGINS
        .iter()
        .any(|allowed| origin.starts_with(allowed))
}

/// Main async WebSocket server
pub async fn serve_multi_async(session_manager: Arc<RwLock<AsyncSessionManager>>) -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:9001").await?;
    println!("WebSocket server listening on ws://0.0.0.0:9001 (async mode)");

    // Spawn cleanup task
    let cleanup_manager = session_manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            let stale = cleanup_manager.write().await.cleanup_stale();
            if !stale.is_empty() {
                eprintln!("WS: Cleaned up {} stale sessions", stale.len());
            }
        }
    });

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                eprintln!("WS: Connection from {:?}", addr);
                let manager = session_manager.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, manager).await {
                        eprintln!("WS: Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("WS: Accept failed: {}", e);
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    manager: Arc<RwLock<AsyncSessionManager>>,
) -> Result<()> {
    // Capture connection info during handshake
    let conn_info = Arc::new(std::sync::Mutex::new(ConnectionInfo::default()));
    let conn_info_clone = conn_info.clone();

    // Accept WebSocket with header callback
    let callback = move |req: &Request,
                         response: Response|
          -> std::result::Result<Response, http::Response<Option<String>>> {
        let mut info = conn_info_clone.lock().unwrap();

        // Extract session ID from URI
        let uri = req.uri().to_string();
        info.session_id = parse_session_id_from_uri(&uri);
        eprintln!("WS: URI={}, parsed session_id={:?}", uri, info.session_id);

        // Extract and validate origin
        if let Some(origin) = req.headers().get("origin") {
            if let Ok(origin_str) = origin.to_str() {
                info.origin = Some(origin_str.to_string());
                info.origin_valid = validate_origin(origin_str);
                eprintln!("WS: Origin={}, valid={}", origin_str, info.origin_valid);
            }
        } else {
            // No origin header = same-origin request (OK)
            info.origin_valid = true;
        }

        Ok(response)
    };

    let ws = tokio_tungstenite::accept_hdr_async(stream, callback).await?;
    let (mut ws_tx, mut ws_rx) = ws.split();

    // Check origin validation result
    let info = conn_info.lock().unwrap().clone();
    if !info.origin_valid {
        eprintln!(
            "WS: Rejected connection from invalid origin: {:?}",
            info.origin
        );
        let _ = ws_tx.close().await;
        return Err(anyhow::anyhow!("Invalid origin"));
    }

    eprintln!("WS: Handshake complete");

    // Get or create session
    let session_id = {
        let mut mgr = manager.write().await;

        // Try to reconnect to existing session
        if let Some(ref existing_id) = info.session_id {
            if mgr.has_session(existing_id) {
                eprintln!("WS: Reconnecting to existing session {}", existing_id);
                if let Some(session) = mgr.get_session_mut(existing_id) {
                    session.connected = true;
                    session.touch();
                    // Request redraw to sync UI state
                    let _ = session.request_redraw().await;
                }
                existing_id.clone()
            } else {
                eprintln!("WS: Session {} not found, creating new", existing_id);
                create_new_session(&mut mgr).await?
            }
        } else {
            eprintln!("WS: Creating new session");
            create_new_session(&mut mgr).await?
        }
    };

    eprintln!("WS: Active session {}", session_id);

    // Send session ID to client
    let session_msg = Value::Array(vec![
        Value::String("session".into()),
        Value::String(session_id.clone().into()),
    ]);
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, &session_msg)?;
    ws_tx.send(Message::Binary(bytes)).await?;

    // Get redraw receiver
    let mut redraw_rx = {
        let mgr = manager.read().await;
        mgr.get_session(&session_id)
            .map(|s| s.subscribe())
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?
    };

    // Bidirectional bridge
    let session_id_clone = session_id.clone();
    let manager_clone = manager.clone();

    loop {
        tokio::select! {
            // Forward redraws to browser
            result = redraw_rx.recv() => {
                match result {
                    Ok(bytes) => {
                        if ws_tx.send(Message::Binary(bytes)).await.is_err() {
                            eprintln!("WS: Send failed, disconnecting");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("WS: Lagged {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        eprintln!("WS: Redraw channel closed");
                        break;
                    }
                }
            }

            // Forward browser input to neovim
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        match handle_browser_message(
                            &session_id_clone,
                            &manager_clone,
                            data
                        ).await {
                            Ok(Some(response_bytes)) => {
                                // Send RPC response back to browser
                                if ws_tx.send(Message::Binary(response_bytes)).await.is_err() {
                                    eprintln!("WS: Failed to send RPC response");
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Fire-and-forget message, no response needed
                            }
                            Err(e) => {
                                eprintln!("WS: Error handling message: {}", e);
                            }
                        }

                        // Touch session
                        let mut mgr = manager_clone.write().await;
                        if let Some(session) = mgr.get_session_mut(&session_id_clone) {
                            session.touch();
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        eprintln!("WS: Client sent close");
                        break;
                    }
                    Some(Err(e)) => {
                        eprintln!("WS: Error reading: {}", e);
                        break;
                    }
                    None => {
                        eprintln!("WS: Connection closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Mark session as disconnected
    {
        let mut mgr = manager.write().await;
        if let Some(session) = mgr.get_session_mut(&session_id) {
            session.connected = false;
            session.touch();
        }
    }

    eprintln!("WS: Client disconnected from session {}", session_id);
    Ok(())
}

/// Helper to create a new session
async fn create_new_session(mgr: &mut AsyncSessionManager) -> Result<String> {
    match mgr.create_session().await {
        Ok(id) => {
            if let Some(session) = mgr.get_session_mut(&id) {
                session.connected = true;
            }
            Ok(id)
        }
        Err(e) => {
            eprintln!("WS: Failed to create session: {}", e);
            Err(e)
        }
    }
}

/// Handle messages from browser
///
/// Protocol envelope: [type, ...payload]
/// - Type 0: RPC request [0, id, method, params] -> responds with [1, id, error, result]
/// - Type "input": fire-and-forget input ["input", keys]
/// - Type "resize": fire-and-forget resize ["resize", cols, rows]
///
/// Returns optional response bytes to send back to browser
async fn handle_browser_message(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
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
                // RPC request: [0, id, method, params]
                let id = arr[1].clone();
                let method = arr[2].as_str().unwrap_or("");
                let params = if let Value::Array(p) = &arr[3] {
                    p.clone()
                } else {
                    vec![]
                };

                eprintln!(
                    "WS: RPC call id={:?} method={} params={:?}",
                    id, method, params
                );

                // Execute RPC call
                let mgr = manager.read().await;
                let session = mgr
                    .get_session(session_id)
                    .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

                let (error, result) = match session.rpc_call(method, params).await {
                    Ok(value) => (Value::Nil, value),
                    Err(e) => (Value::String(e.to_string().into()), Value::Nil),
                };

                // Build response: [1, id, error, result]
                let response = Value::Array(vec![Value::Integer(1.into()), id, error, result]);

                let mut bytes = Vec::new();
                rmpv::encode::write_value(&mut bytes, &response)?;

                return Ok(Some(bytes));
            }
        }

        // Legacy string-based messages
        if arr.len() >= 2 {
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
                            if let (Value::Integer(cols), Value::Integer(rows)) = (&arr[1], &arr[2])
                            {
                                let cols = cols.as_i64().unwrap_or(80);
                                let rows = rows.as_i64().unwrap_or(24);
                                eprintln!("WS: Resize request: cols={}, rows={}", cols, rows);
                                session.resize(cols, rows).await?;
                                eprintln!("WS: Resize complete");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(None)
}
