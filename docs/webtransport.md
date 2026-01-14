# WebTransport Support

nvim-web supports WebTransport as an alternative to WebSocket for lower-latency communication between the browser and host.

## Overview

WebTransport is a modern web API built on HTTP/3 and QUIC that provides:

- **0-RTT connection establishment** - Faster initial connection compared to TCP + TLS
- **Multiplexed streams** - No head-of-line blocking
- **Unreliable datagrams** - For cursor updates and input events where low latency matters more than reliability

## Requirements

WebTransport requires TLS. You'll need:

1. A TLS certificate and private key (PEM format)
2. Browser support (Chrome 97+, Edge 97+, Firefox 114+)

## Configuration

Add to `~/.config/nvim-web/config.toml`:

```toml
[server]
http_port = 8080
bind = "127.0.0.1"

# Enable WebTransport
webtransport_port = 9002

# TLS certificates (required for WebTransport)
ssl_cert = "/path/to/cert.pem"
ssl_key = "/path/to/key.pem"
```

## Development Setup

For local development, nvim-web can generate self-signed certificates:

```rust
use nvim_web_host::transport::WebTransportConfig;

let (config, cert_der) = WebTransportConfig::generate_self_signed(9002)?;
// cert_der can be used for certificate pinning in the browser
```

The certificates are written to a temp directory and used automatically.

## Browser Connection

The browser UI auto-detects WebTransport availability:

1. If the server advertises WebTransport (via HTTPS), try WebTransport first
2. If WebTransport fails or is unavailable, fall back to WebSocket
3. Both transports use the same message format (MessagePack)

## Message Types

### Reliable Streams

Used for RPC calls and redraw events. Messages are ordered and guaranteed to be delivered.

### Unreliable Datagrams

Used for high-frequency, low-latency events:

| Type | Code | Description |
|------|------|-------------|
| Cursor | 0x01 | Cursor position updates |
| Input | 0x02 | Keyboard/mouse input events |
| Heartbeat | 0x03 | Connection keep-alive |

Datagrams may be dropped under congestion but provide lower latency than reliable streams.

## Architecture

```
Browser                          Host
   |                              |
   |---[WebTransport Connect]---->|
   |                              |
   |<--[BiDi Stream: RPC]-------->|
   |<--[BiDi Stream: Redraw]----->|
   |                              |
   |---[Datagram: Input]--------->|
   |<--[Datagram: Cursor]---------|
   |                              |
```

## Transport Trait

The `Transport` trait provides a unified interface for both WebSocket and WebTransport:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send_reliable(&self, data: Bytes) -> Result<()>;
    async fn send_datagram(&self, data: Bytes) -> Result<()>;
    fn is_connected(&self) -> bool;
    async fn close(&self) -> Result<()>;
}
```

For WebSocket, `send_datagram` falls back to `send_reliable`.

## Security

- WebTransport requires valid TLS certificates
- Self-signed certificates work for localhost development
- For production, use certificates from a trusted CA
- Certificate pinning is supported for additional security
