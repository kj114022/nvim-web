//! WebSocket connection handling
//!
//! Manages individual WebSocket connections including handshake,
//! session management, and bidirectional message bridging.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use rmpv::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

use crate::session::AsyncSessionManager;
use crate::vfs::{FsRequestRegistry, VfsManager};

use super::commands::handle_browser_message;
use super::protocol::{
    parse_context_from_uri, parse_session_id_from_uri, parse_view_id_from_uri, validate_origin,
};
use super::rate_limit::RateLimiter;

/// Connection metadata extracted during WebSocket handshake
#[derive(Debug, Clone, Default)]
pub struct ConnectionInfo {
    pub session_id: Option<String>,
    pub view_session_id: Option<String>,
    pub origin: Option<String>,
    pub origin_valid: bool,
    pub is_viewer: bool,
    pub context: Option<String>,
}

/// Handle a single WebSocket connection
#[allow(clippy::too_many_lines)]
#[allow(clippy::significant_drop_tightening)]
pub async fn handle_connection<S>(
    stream: S,
    manager: Arc<RwLock<AsyncSessionManager>>,
    fs_registry: Option<Arc<FsRequestRegistry>>,
    vfs_manager: Option<Arc<RwLock<VfsManager>>>,
    fs_request_tx: Option<broadcast::Sender<Vec<u8>>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Capture connection info during handshake
    let conn_info = Arc::new(std::sync::Mutex::new(ConnectionInfo::default()));
    let conn_info_clone = conn_info.clone();

    // Accept WebSocket with header callback
    let callback = move |req: &Request,
                         response: Response|
          -> std::result::Result<Response, http::Response<Option<String>>> {
        let mut info = conn_info_clone.lock().unwrap();

        // Extract session ID or view ID from URI
        let uri = req.uri().to_string();

        // Check for viewer mode first (?view=session_id)
        if let Some(view_id) = parse_view_id_from_uri(&uri) {
            info.view_session_id = Some(view_id);
            info.is_viewer = true;
        } else {
            info.session_id = parse_session_id_from_uri(&uri);
        }

        // Extract context (URL)
        info.context = parse_context_from_uri(&uri);

        // Extract and validate origin
        if let Some(origin) = req.headers().get("origin") {
            if let Ok(origin_str) = origin.to_str() {
                info.origin = Some(origin_str.to_string());
                info.origin_valid = validate_origin(origin_str);
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
        tracing::warn!(
            origin = ?info.origin,
            "Rejected connection from invalid origin"
        );
        let _ = ws_tx.close().await;
        return Err(anyhow::anyhow!("Invalid origin"));
    }

    // Handle viewer mode or regular session
    let (session_id, is_viewer) = if info.is_viewer {
        // Viewer mode: join existing session in read-only mode
        let view_id = info.view_session_id.unwrap();
        let mgr = manager.read().await;
        if mgr.has_session(&view_id) {
            // Request redraw to sync viewer
            if let Some(session) = mgr.get_session(&view_id) {
                let _ = session.request_redraw().await;
            }
            (view_id, true)
        } else {
            tracing::warn!("Viewer requested non-existent session");
            let _ = ws_tx.close().await;
            return Err(anyhow::anyhow!("Session not found for viewing"));
        }
    } else {
        // Regular mode: get or create session
        let session_id = {
            let mut mgr = manager.write().await;

            // Try to reconnect to existing session
            if let Some(ref existing_id) = info.session_id {
                if mgr.has_session(existing_id) {
                    if let Some(session) = mgr.get_session_mut(existing_id) {
                        session.connected = true;
                        session.touch();
                        // Restore session state (cursor, buffers, undo)
                        let _ = session.restore_session().await;
                        // Request redraw to sync UI state
                        let _ = session.request_redraw().await;
                    }
                    existing_id.clone()
                } else {
                    create_new_session(&mut mgr, info.context.clone()).await?
                }
            } else {
                create_new_session(&mut mgr, info.context.clone()).await?
            }
        };
        (session_id, false)
    };

    tracing::info!(
        session_id = %session_id,
        is_viewer = is_viewer,
        "Session connected"
    );

    // Send session ID and viewer status to client
    let session_msg = Value::Array(vec![
        Value::String("session".into()),
        Value::String(session_id.clone().into()),
        Value::Boolean(is_viewer),
    ]);
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, &session_msg)?;
    ws_tx.send(Message::Binary(bytes)).await?;

    // Get redraw receiver
    let mut redraw_rx = {
        let mgr = manager.read().await;
        mgr.get_session(&session_id)
            .map(crate::session::AsyncSession::subscribe)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?
    };

    // VFS request receiver (for BrowserFS)
    let fs_rx = fs_request_tx
        .as_ref()
        .map(tokio::sync::broadcast::Sender::subscribe);

    // ========================================
    // SPLIT PATTERN: Dedicated sender task
    // ========================================
    // Wrap ws_tx in Arc<Mutex> for concurrent access from sender task
    let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));
    let ws_tx_sender = ws_tx.clone();
    let ws_tx_fs = ws_tx.clone();

    let sender_session_id = session_id.clone();
    let sender_manager = manager.clone();

    // Spawn dedicated task for Neovim -> Browser messages (redraws)
    let sender_handle = tokio::spawn(async move {
        let mut last_lag_recovery = std::time::Instant::now();
        const LAG_RECOVERY_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(2);

        loop {
            match redraw_rx.recv().await {
                Ok(bytes) => {
                    let mut tx = ws_tx_sender.lock().await;
                    if tx.send(Message::Binary(bytes)).await.is_err() {
                        tracing::warn!(session_id = %sender_session_id, "Send failed, stopping sender");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        session_id = %sender_session_id,
                        dropped_messages = n,
                        "Redraw messages lagged, requesting full resync"
                    );

                    if last_lag_recovery.elapsed() >= LAG_RECOVERY_DEBOUNCE {
                        last_lag_recovery = std::time::Instant::now();
                        let mgr = sender_manager.read().await;
                        if let Some(session) = mgr.get_session(&sender_session_id) {
                            if let Err(e) = session.request_redraw().await {
                                tracing::error!(
                                    session_id = %sender_session_id,
                                    error = %e,
                                    "Failed to request redraw after lag"
                                );
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Spawn dedicated task for VFS -> Browser messages (if BrowserFS enabled)
    let fs_sender_handle = if let Some(mut fs_rx) = fs_rx {
        let ws_tx_fs_clone = ws_tx_fs.clone();
        Some(tokio::spawn(async move {
            loop {
                match fs_rx.recv().await {
                    Ok(bytes) => {
                        let mut tx = ws_tx_fs_clone.lock().await;
                        let _ = tx.send(Message::Binary(bytes)).await;
                    }
                    Err(_) => break,
                }
            }
        }))
    } else {
        None
    };

    // Rate limiter: 1000 burst, 100/sec sustained
    let mut rate_limiter = RateLimiter::default_ws();

    // Clones for main loop
    let session_id_clone = session_id.clone();
    let manager_clone = manager.clone();

    // ========================================
    // Main loop: Browser -> Neovim only
    // With heartbeat to detect zombie sessions
    // ========================================

    // Heartbeat configuration
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
    const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    let mut last_activity = Instant::now();

    loop {
        tokio::select! {
            // Heartbeat tick - send ping
            _ = heartbeat_interval.tick() => {
                // Check for zombie session (no activity in 5 minutes)
                if last_activity.elapsed() > HEARTBEAT_TIMEOUT {
                    tracing::warn!(
                        session_id = %session_id_clone,
                        elapsed_secs = last_activity.elapsed().as_secs(),
                        "Session heartbeat timeout, triggering auto-save"
                    );

                    // Auto-save before disconnect
                    {
                        let mgr = manager_clone.read().await;
                        if let Some(session) = mgr.get_session(&session_id_clone) {
                            let _ = session.rpc_call("nvim_command", vec![Value::String("silent! w".into())]).await;
                            let _ = session.rpc_call("nvim_command", vec![Value::String("silent! mksession! ~/.local/state/nvim/sessions/auto.vim".into())]).await;
                        }
                    }
                    break;
                }

                // Send ping to keep connection alive
                let mut tx = ws_tx.lock().await;
                if tx.send(Message::Ping(vec![])).await.is_err() {
                    tracing::debug!(session_id = %session_id_clone, "Ping send failed");
                    break;
                }
            }

            // WebSocket message
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // Update activity timestamp
                        last_activity = Instant::now();

                        // Viewers can only receive, not send input
                        if is_viewer {
                            continue;
                        }

                        // Rate limit check
                        if !rate_limiter.try_consume() {
                            tracing::warn!(
                                session_id = %session_id_clone,
                                "Rate limit exceeded, dropping message"
                            );
                            continue;
                        }

                        match handle_browser_message(
                            &session_id_clone,
                            &manager_clone,
                            fs_registry.as_ref(),
                            vfs_manager.as_ref(),
                            data
                        ).await {
                            Ok(Some(response_bytes)) => {
                                // Send RPC response back to browser
                                let mut tx = ws_tx.lock().await;
                                if tx.send(Message::Binary(response_bytes)).await.is_err() {
                                    tracing::warn!(session_id = %session_id_clone, "Failed to send RPC response");
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Fire-and-forget message, no response needed
                            }
                            Err(_e) => {
                                // Message handling errors are routine
                            }
                        }

                        // Touch session
                        let mut mgr = manager_clone.write().await;
                        if let Some(session) = mgr.get_session_mut(&session_id_clone) {
                            session.touch();
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Pong received - connection is alive
                        last_activity = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Err(_e)) => {
                        break;
                    }
                    None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Clean up sender tasks
    sender_handle.abort();
    if let Some(handle) = fs_sender_handle {
        handle.abort();
    }

    // Mark session as disconnected
    {
        let mut mgr = manager.write().await;
        if let Some(session) = mgr.get_session_mut(&session_id) {
            session.connected = false;
            session.touch();
        }
    }

    tracing::info!(session_id = %session_id, "Client disconnected");
    Ok(())
}

/// Helper to create a new session
pub async fn create_new_session(
    mgr: &mut AsyncSessionManager,
    context: Option<String>,
) -> Result<String> {
    match mgr.create_session(context).await {
        Ok(id) => {
            if let Some(session) = mgr.get_session_mut(&id) {
                session.connected = true;
            }
            Ok(id)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create session");
            Err(e)
        }
    }
}
