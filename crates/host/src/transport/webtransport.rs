//! WebTransport server implementation
//!
//! Provides HTTP/3 + QUIC based transport with support for:
//! - Reliable bidirectional streams
//! - Unreliable datagrams (for cursor/input events)
//! - 0-RTT connection establishment

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{info, warn};
use wtransport::{Connection, Endpoint, Identity, ServerConfig};

use crate::session::AsyncSessionManager;
use crate::vfs::{FsRequestRegistry, VfsManager};

/// WebTransport server configuration
#[derive(Debug, Clone)]
pub struct WebTransportConfig {
    /// Port to listen on
    pub port: u16,
    /// Path to TLS certificate (PEM format)
    pub cert_path: String,
    /// Path to TLS private key (PEM format)
    pub key_path: String,
}

impl WebTransportConfig {
    /// Generate self-signed certificate for development
    ///
    /// Returns (config, certificate_der) for browser trust
    pub fn generate_self_signed(port: u16) -> Result<(Self, Vec<u8>)> {
        use std::io::Write;

        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;

        // Write to temp files
        let temp_dir = std::env::temp_dir().join("nvim-web-certs");
        std::fs::create_dir_all(&temp_dir)?;

        let cert_path = temp_dir.join("cert.pem");
        let key_path = temp_dir.join("key.pem");

        let mut cert_file = std::fs::File::create(&cert_path)?;
        cert_file.write_all(cert.cert.pem().as_bytes())?;

        let mut key_file = std::fs::File::create(&key_path)?;
        key_file.write_all(cert.key_pair.serialize_pem().as_bytes())?;

        // Get DER for browser certificate pinning
        let cert_der = cert.cert.der().to_vec();

        Ok((
            Self {
                port,
                cert_path: cert_path.to_string_lossy().to_string(),
                key_path: key_path.to_string_lossy().to_string(),
            },
            cert_der,
        ))
    }
}

/// Start WebTransport server
///
/// Listens for incoming WebTransport connections and routes them
/// to the session manager.
pub async fn serve_webtransport(
    session_manager: Arc<RwLock<AsyncSessionManager>>,
    config: WebTransportConfig,
    _fs_registry: Option<Arc<FsRequestRegistry>>,
    _vfs_manager: Option<Arc<RwLock<VfsManager>>>,
) -> Result<()> {
    // Load identity from PEM files
    let identity = Identity::load_pemfiles(&config.cert_path, &config.key_path).await?;

    // Build server config
    let server_config = ServerConfig::builder()
        .with_bind_default(config.port)
        .with_identity(identity)
        .build();

    let server = Endpoint::server(server_config)?;

    info!(port = config.port, "WebTransport server listening");

    // Accept loop
    loop {
        let incoming_session = server.accept().await;
        let session_manager = session_manager.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_webtransport_session(incoming_session, session_manager).await {
                warn!(error = %e, "WebTransport session error");
            }
        });
    }
}

/// Handle a single WebTransport session
async fn handle_webtransport_session(
    incoming_session: wtransport::endpoint::IncomingSession,
    session_manager: Arc<RwLock<AsyncSessionManager>>,
) -> Result<()> {
    // Wait for the session request
    let incoming_request = incoming_session.await?;

    // Extract path before consuming incoming_request
    let path = incoming_request.path().to_string();
    let authority = incoming_request.authority().to_string();

    info!(
        path = %path,
        authority = %authority,
        "WebTransport session request"
    );

    // Accept the connection (consumes incoming_request)
    let connection = incoming_request.accept().await?;

    info!("WebTransport connection established");

    // Parse session ID from path (e.g., /?session=abc123)
    let session_id = parse_session_id(&path);

    // Get or create session - for now just log
    // Full integration with AsyncSessionManager will be done in a follow-up
    if let Some(ref id) = session_id {
        info!(session_id = %id, "Session ID parsed from path");
    }

    // Store reference to session manager for future use
    let _manager = session_manager;

    // Spawn tasks for handling streams and datagrams
    let conn = Arc::new(connection);

    // Handle bidirectional streams (for RPC)
    let stream_conn = conn.clone();
    let stream_task = tokio::spawn(async move { handle_bidirectional_streams(stream_conn).await });

    // Handle datagrams (for cursor/input)
    let datagram_conn = conn.clone();
    let datagram_task = tokio::spawn(async move { handle_datagrams(datagram_conn).await });

    // Wait for either task to complete (connection closed)
    tokio::select! {
        result = stream_task => {
            if let Err(e) = result {
                warn!(error = %e, "Stream task panicked");
            }
        }
        result = datagram_task => {
            if let Err(e) = result {
                warn!(error = %e, "Datagram task panicked");
            }
        }
    }

    info!("WebTransport session closed");
    Ok(())
}

/// Handle bidirectional streams for RPC communication
async fn handle_bidirectional_streams(conn: Arc<Connection>) -> Result<()> {
    loop {
        // Accept incoming bidirectional stream
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(stream) => stream,
            Err(e) => {
                // Connection closed
                info!(error = %e, "BiDi stream accept ended");
                break;
            }
        };

        // Spawn handler for this stream
        tokio::spawn(async move {
            // Read request
            let mut buf = vec![0u8; 65536];
            match recv.read(&mut buf).await {
                Ok(Some(n)) => {
                    let data = &buf[..n];

                    // Process message (integration point: ws/commands.rs handlers)
                    let response = process_webtransport_message(data).await;

                    // Send response
                    if let Err(e) = send.write_all(&response).await {
                        warn!(error = %e, "Failed to send response");
                    }
                }
                Ok(None) => {
                    // Stream closed
                }
                Err(e) => {
                    warn!(error = %e, "Stream read error");
                }
            }
        });
    }

    Ok(())
}

/// Handle unreliable datagrams for low-latency events
async fn handle_datagrams(conn: Arc<Connection>) -> Result<()> {
    loop {
        match conn.receive_datagram().await {
            Ok(datagram) => {
                // Datagrams are used for cursor position updates, input events
                // These are fire-and-forget, no response needed
                let data = datagram.payload();

                // Parse and handle datagram message
                if let Err(e) = handle_datagram_message(&data).await {
                    warn!(error = %e, "Datagram processing error");
                }
            }
            Err(e) => {
                // Connection closed or error
                info!(error = %e, "Datagram receive ended");
                break;
            }
        }
    }

    Ok(())
}

/// Parse session ID from request path
fn parse_session_id(path: &str) -> Option<String> {
    // Parse query string: /?session=abc123
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.split('=');
        if parts.next() == Some("session") {
            return parts.next().map(String::from);
        }
    }
    None
}

/// Process a WebTransport message (similar to WebSocket handler)
async fn process_webtransport_message(data: &[u8]) -> Vec<u8> {
    // Decode msgpack message using rmpv directly (not serde)
    let _msg: Result<rmpv::Value, _> = rmpv::decode::read_value(&mut &data[..]);

    // Integration point: Route to ws/commands.rs handlers
    // The same handlers used by WebSocket can be reused here

    vec![]
}

/// Handle datagram message (cursor updates, input events)
async fn handle_datagram_message(_data: &[u8]) -> Result<()> {
    // Parse datagram type and payload
    // Datagram format: [type: u8, payload...]
    // Types:
    //   0x01 = Cursor position
    //   0x02 = Input event
    //   0x03 = Heartbeat

    // For ultra-low-latency events that don't need reliability
    // Integrate with collaboration.rs for cursor sync

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_id() {
        assert_eq!(parse_session_id("/?session=abc123"), Some("abc123".into()));
        assert_eq!(
            parse_session_id("/?foo=bar&session=xyz"),
            Some("xyz".into())
        );
        assert_eq!(parse_session_id("/"), None);
        assert_eq!(parse_session_id("/?other=value"), None);
    }

    #[test]
    fn test_generate_self_signed() {
        let result = WebTransportConfig::generate_self_signed(9002);
        assert!(result.is_ok());

        let (config, cert_der) = result.unwrap();
        assert_eq!(config.port, 9002);
        assert!(!cert_der.is_empty());
    }
}
