//! nvim-wasm-shim
//!
//! Provides a `libuv`-like interface for WASM environments, allowing Neovim's code
//! (or Rust ports of it) to access system resources via browser APIs.

pub mod fs;
pub mod net;
pub mod process;
pub mod time;

/// Initialize the shim layer (sets up logging, panic hooks, etc.)
pub fn init() {
    #[cfg(target_arch = "wasm32")]
    {
        use std::panic;
        panic::set_hook(Box::new(console_error_panic_hook::hook));
        tracing_wasm::set_as_global_default();
    }
}

/// Platform-agnostic error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("System error: {0}")]
    System(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not implemented on this platform")]
    NotImplemented,
}

pub type Result<T> = std::result::Result<T, Error>;
