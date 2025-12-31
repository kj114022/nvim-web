//! WebSocket connection handling
//!
//! Manages individual WebSocket connections including handshake,
//! session management, and bidirectional message bridging.

use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use rmpv::Value;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

use crate::session::AsyncSessionManager;
use crate::vfs::{FsRequestRegistry, VfsManager};

use super::protocol::{parse_session_id_from_uri, parse_view_id_from_uri, validate_origin};
use super::commands::handle_browser_message;
use super::rate_limit::RateLimiter;

/// Connection metadata extracted during WebSocket handshake
#[derive(Debug, Clone, Default)]
pub struct ConnectionInfo {
    pub session_id: Option<String>,
    pub view_session_id: Option<String>,
    pub origin: Option<String>,
    pub origin_valid: bool,
    pub is_viewer: bool,
}

/// Handle a single WebSocket connection
#[allow(clippy::too_many_lines)]
#[allow(clippy::significant_drop_tightening)]
pub async fn handle_connection(
    stream: TcpStream,
    manager: Arc<RwLock<AsyncSessionManager>>,
    fs_registry: Option<Arc<FsRequestRegistry>>,
    vfs_manager: Option<Arc<RwLock<VfsManager>>>,
) -> Result<()> {
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
                        // Request redraw to sync UI state
                        let _ = session.request_redraw().await;
                    }
                    existing_id.clone()
                } else {
                    create_new_session(&mut mgr).await?
                }
            } else {
                create_new_session(&mut mgr).await?
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

    // Bidirectional bridge
    let session_id_clone = session_id.clone();
    let manager_clone = manager.clone();
    
    // Rate limiter: 1000 burst, 100/sec sustained
    let mut rate_limiter = RateLimiter::default_ws();

    loop {
        tokio::select! {
            // Forward redraws to browser
            result = redraw_rx.recv() => {
                match result {
                    Ok(bytes) => {
                        if ws_tx.send(Message::Binary(bytes)).await.is_err() {
                            tracing::warn!(session_id = %session_id_clone, "Send failed, disconnecting");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_n)) => {
                        // Lagged messages - normal under load
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Forward browser input to neovim (blocked for viewers)
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // Viewers can only receive, not send input
                        if is_viewer {
                            // Ignore input from viewers, just keep connection alive
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
                                if ws_tx.send(Message::Binary(response_bytes)).await.is_err() {
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
pub async fn create_new_session(mgr: &mut AsyncSessionManager) -> Result<String> {
    match mgr.create_session().await {
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
