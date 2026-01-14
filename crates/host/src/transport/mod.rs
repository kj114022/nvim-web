//! Transport abstraction layer for WebSocket and WebTransport
//!
//! Provides a unified interface for different transport protocols,
//! enabling automatic fallback and protocol selection.

mod websocket;
mod webtransport;

use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub use websocket::WebSocketTransport;
pub use webtransport::{serve_webtransport, WebTransportConfig};

/// Message types for transport layer
#[derive(Debug, Clone)]
pub enum TransportMessage {
    /// Reliable, ordered message (for RPC, redraw events)
    Reliable(Bytes),
    /// Unreliable datagram (for cursor updates, input events)
    Datagram(Bytes),
    /// Connection closed
    Closed,
}

/// Transport state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Connecting,
    Connected,
    Closing,
    Closed,
}

/// Unified transport layer trait for WebSocket and WebTransport
///
/// This abstraction allows the session layer to work with either
/// transport protocol without protocol-specific code.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a reliable message (ordered, guaranteed delivery)
    ///
    /// Use for: RPC calls, redraw events, VFS operations
    async fn send_reliable(&self, data: Bytes) -> anyhow::Result<()>;

    /// Send an unreliable datagram (low latency, may be dropped)
    ///
    /// Use for: Cursor position updates, input events
    /// Falls back to reliable send if datagrams not supported.
    async fn send_datagram(&self, data: Bytes) -> anyhow::Result<()>;

    /// Subscribe to incoming messages
    fn subscribe(&self) -> mpsc::Receiver<TransportMessage>;

    /// Check if connection is still active
    fn is_connected(&self) -> bool;

    /// Get current transport state
    fn state(&self) -> TransportState;

    /// Close the connection gracefully
    async fn close(&self) -> anyhow::Result<()>;

    /// Get transport type identifier
    fn transport_type(&self) -> &'static str;
}

/// Shared transport reference
pub type SharedTransport = Arc<RwLock<dyn Transport>>;

/// Transport capabilities
#[derive(Debug, Clone, Copy)]
pub struct TransportCapabilities {
    /// Supports unreliable datagrams
    pub datagrams: bool,
    /// Supports multiplexed streams
    pub streams: bool,
    /// Supports 0-RTT connection
    pub zero_rtt: bool,
}

impl TransportCapabilities {
    pub const WEBSOCKET: Self = Self {
        datagrams: false,
        streams: false,
        zero_rtt: false,
    };

    pub const WEBTRANSPORT: Self = Self {
        datagrams: true,
        streams: true,
        zero_rtt: true,
    };
}
