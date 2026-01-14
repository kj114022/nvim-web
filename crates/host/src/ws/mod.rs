//! Async WebSocket server using tokio-tungstenite
//!
//! Handles multiple concurrent connections with async session management.
//! Includes origin validation and session reconnection support.
//!
//! ## Module Structure
//! - `protocol`: URI parsing, origin validation
//! - `connection`: WebSocket handshake, session management
//! - `commands`: RPC handlers, VFS operations, settings

mod commands;
mod connection;
mod protocol;
mod rate_limit;

pub use rate_limit::RateLimiter;

use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::session::AsyncSessionManager;
use crate::vfs::{FsRequestRegistry, VfsManager};

// Re-export for external use
pub use connection::ConnectionInfo;
pub use protocol::ALLOWED_ORIGINS;

/// Main async WebSocket server
///
/// # Arguments
/// * `session_manager` - Session manager for Neovim sessions
/// * `port` - Port to listen on
/// * `fs_registry` - Optional `FsRequestRegistry` for `BrowserFs` support
/// * `vfs_manager` - Optional `VfsManager` for VFS operations
/// * `fs_request_tx` - Optional `broadcast::Sender` for forwarding VFS requests
pub async fn serve_multi_async(
    session_manager: Arc<RwLock<AsyncSessionManager>>,
    port: u16,
    fs_registry: Option<Arc<FsRequestRegistry>>,
    vfs_manager: Option<Arc<RwLock<VfsManager>>>,
    fs_request_tx: Option<tokio::sync::broadcast::Sender<Vec<u8>>>,
    tls_config: Option<Arc<tokio_rustls::rustls::ServerConfig>>,
) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    let protocol = if tls_config.is_some() { "wss" } else { "ws" };
    tracing::info!(addr = %addr, protocol = %protocol, "WebSocket server listening");

    let acceptor = tls_config.map(tokio_rustls::TlsAcceptor::from);

    // Spawn cleanup task
    let cleanup_manager = session_manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            let stale = cleanup_manager.write().await.cleanup_stale();
            if !stale.is_empty() {
                tracing::info!(count = stale.len(), "Cleaned up stale sessions");
            }
        }
    });

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let manager = session_manager.clone();
                let registry = fs_registry.clone();
                let vfs = vfs_manager.clone();
                let request_tx = fs_request_tx.clone();
                let acceptor = acceptor.clone();

                tokio::spawn(async move {
                    if let Some(acceptor) = acceptor {
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                if let Err(e) = connection::handle_connection(
                                    tls_stream, manager, registry, vfs, request_tx,
                                )
                                .await
                                {
                                    tracing::warn!(error = %e, "Connection error");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "TLS handshake failed");
                            }
                        }
                    } else {
                        if let Err(e) = connection::handle_connection(
                            stream, manager, registry, vfs, request_tx,
                        )
                        .await
                        {
                            tracing::warn!(error = %e, "Connection error");
                        }
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "Accept failed");
            }
        }
    }
}
