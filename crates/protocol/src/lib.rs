//! Shared protocol types for nvim-web
//!
//! Defines the MessagePack-RPC structures used between Host and UI.

pub mod messages;
pub mod rpc;

pub use messages::*;
pub use rpc::*;
