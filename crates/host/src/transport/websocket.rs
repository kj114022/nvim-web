//! WebSocket transport adapter
//!
//! Wraps existing WebSocket connections in the Transport trait.

use async_trait::async_trait;
use bytes::Bytes;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;

use super::{Transport, TransportMessage, TransportState};

/// WebSocket transport implementation
pub struct WebSocketTransport {
    /// Sender for outgoing messages
    tx: mpsc::Sender<Message>,
    /// Channel for incoming messages
    rx_tx: mpsc::Sender<TransportMessage>,
    /// Connection state
    connected: AtomicBool,
    /// State
    state: Mutex<TransportState>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport from a message channel
    pub fn new(tx: mpsc::Sender<Message>) -> (Self, mpsc::Receiver<TransportMessage>) {
        let (rx_tx, rx_rx) = mpsc::channel(256);

        let transport = Self {
            tx,
            rx_tx,
            connected: AtomicBool::new(true),
            state: Mutex::new(TransportState::Connected),
        };

        (transport, rx_rx)
    }

    /// Feed incoming messages from WebSocket
    pub async fn feed(&self, msg: Message) -> anyhow::Result<()> {
        match msg {
            Message::Binary(data) => {
                let _ = self
                    .rx_tx
                    .send(TransportMessage::Reliable(data.into()))
                    .await;
            }
            Message::Text(text) => {
                let _ = self
                    .rx_tx
                    .send(TransportMessage::Reliable(text.into_bytes().into()))
                    .await;
            }
            Message::Close(_) => {
                self.connected.store(false, Ordering::SeqCst);
                *self.state.lock().await = TransportState::Closed;
                let _ = self.rx_tx.send(TransportMessage::Closed).await;
            }
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                // Handled by tungstenite
            }
        }
        Ok(())
    }

    /// Mark connection as closed
    pub async fn mark_closed(&self) {
        self.connected.store(false, Ordering::SeqCst);
        *self.state.lock().await = TransportState::Closed;
        let _ = self.rx_tx.send(TransportMessage::Closed).await;
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn send_reliable(&self, data: Bytes) -> anyhow::Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            anyhow::bail!("Connection closed");
        }

        self.tx
            .send(Message::Binary(data.to_vec()))
            .await
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
        Ok(())
    }

    async fn send_datagram(&self, data: Bytes) -> anyhow::Result<()> {
        // WebSocket doesn't support datagrams, fallback to reliable
        self.send_reliable(data).await
    }

    fn subscribe(&self) -> mpsc::Receiver<TransportMessage> {
        // This is a simplified implementation - in practice you'd want
        // to support multiple subscribers or use broadcast
        let (_tx, rx) = mpsc::channel(256);
        // Note: This creates a new receiver that won't receive past messages
        // A proper implementation would use broadcast or Arc<Mutex<Vec>>
        rx
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn state(&self) -> TransportState {
        // Can't use async in this method, so we check the atomic
        if self.connected.load(Ordering::SeqCst) {
            TransportState::Connected
        } else {
            TransportState::Closed
        }
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        *self.state.lock().await = TransportState::Closing;

        // Send close frame
        let _ = self.tx.send(Message::Close(None)).await;

        *self.state.lock().await = TransportState::Closed;
        Ok(())
    }

    fn transport_type(&self) -> &'static str {
        "websocket"
    }
}
