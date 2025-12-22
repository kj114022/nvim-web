// Re-export public API for tests
pub mod nvim;
pub mod rpc;
pub mod rpc_sync;
pub mod vfs;

// Debug infrastructure
pub mod debug;

// Internal modules not exposed
mod rpc_buffers;
mod ws;
mod vfs_handlers;
