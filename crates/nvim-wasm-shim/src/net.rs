use crate::Result;
use std::net::SocketAddr;

/// A TCP stream between a local and a remote socket.
#[derive(Debug)]
pub struct TcpStream;

impl TcpStream {
    pub async fn connect(_addr: impl ToSocketAddrs) -> Result<TcpStream> {
        // In WASM, we might tunnel this over WebSocket or generic pipe
        // For now, stub as not implemented or return dummy
        Ok(TcpStream)
    }

    pub async fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }

    pub async fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        Ok(0)
    }
}

/// A TCP socket server, listening for connections.
#[derive(Debug)]
pub struct TcpListener;

impl TcpListener {
    pub async fn bind(_addr: impl ToSocketAddrs) -> Result<TcpListener> {
        Ok(TcpListener)
    }

    pub async fn accept(&self) -> Result<(TcpStream, SocketAddr)> {
        // Stall forever or return error
        Err(crate::Error::NotImplemented)
    }
}

pub trait ToSocketAddrs {
    // Stub
}

impl<T: ToString> ToSocketAddrs for T {}
