//! Async WebSocket server using tokio-tungstenite
//!
//! Handles multiple concurrent connections with async session management.

use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use rmpv::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::session::AsyncSessionManager;

/// Parse session ID from WebSocket upgrade request path
fn parse_session_id_from_path(path: &str) -> Option<String> {
    if let Some(query_start) = path.find('?') {
        let query = &path[query_start + 1..];
        for param in query.split('&') {
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key == "session" {
                    if value == "new" {
                        return None;
                    }
                    return Some(value.to_string());
                }
            }
        }
    }
    None
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
    // Accept WebSocket connection
    let ws = accept_async(stream).await?;
    let (mut ws_tx, mut ws_rx) = ws.split();
    
    eprintln!("WS: Handshake complete");

    // For now, always create a new session (session ID parsing would need HTTP headers)
    // TODO: Parse session ID from initial HTTP upgrade request
    let session_id = {
        let mut mgr = manager.write().await;
        match mgr.create_session().await {
            Ok(id) => {
                if let Some(session) = mgr.get_session_mut(&id) {
                    session.connected = true;
                }
                id
            }
            Err(e) => {
                eprintln!("WS: Failed to create session: {}", e);
                return Err(e);
            }
        }
    };

    eprintln!("WS: Created session {}", session_id);

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

    // TODO: VFS initialization - disabled for initial async migration
    // Will be re-enabled when VFS handlers are migrated to async

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
                        if let Err(e) = handle_browser_message(
                            &session_id_clone, 
                            &manager_clone, 
                            data
                        ).await {
                            eprintln!("WS: Error handling message: {}", e);
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

async fn handle_browser_message(
    session_id: &str,
    manager: &Arc<RwLock<AsyncSessionManager>>,
    data: Vec<u8>,
) -> Result<()> {
    let mut cursor = std::io::Cursor::new(data);
    let msg = rmpv::decode::read_value(&mut cursor)?;
    
    if let Value::Array(arr) = msg {
        if arr.len() >= 2 {
            if let Value::String(method) = &arr[0] {
                let mgr = manager.read().await;
                let session = mgr.get_session(session_id)
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
                    // TODO: Add VFS handlers
                    _ => {}
                }
            }
        }
    }
    
    Ok(())
}

use tokio::sync::broadcast;
