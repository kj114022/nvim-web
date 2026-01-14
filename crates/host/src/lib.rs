// nvim-web-host library
// Async Neovim GUI host using nvim-rs and tokio
pub mod native;

// Core async modules
pub mod session;
pub mod ws;

// Transport abstraction (WebSocket, WebTransport)
pub mod transport;

// Virtual filesystem layer
pub use nvim_web_vfs as vfs;
pub mod vfs_handlers;

// Configuration
pub mod config;
pub mod context;

// Authentication for remote connections
pub mod auth;

// OIDC/BeyondCorp authentication
pub mod oidc;

// SSH tunnel management
pub mod tunnel;

// REST API
pub mod api;

// Settings persistence
pub mod settings;

// Git utilities
pub mod git;

// Embedded UI assets (single-binary distribution)
pub mod embedded;

// Project configuration and magic link handling
pub mod project;

// Session sharing and snapshots
pub mod sharing;

// Multi-user collaboration (viewers, cursor sync)
pub mod collaboration;

// CRDT support for real-time collaborative editing
pub mod crdt;

// Terminal PTY management (portable-pty)
pub mod terminal;

// Universal tool pipe (replaces hardcoded LLM providers)
pub mod pipe;

// Backend swap (docker, ssh, tcp, vfs hot-swapping)
pub mod backend_swap;

// Host-side search (ripgrep-style)
pub mod search;

// End-to-end latency tracing (Dapper-style)
pub mod trace;

// Kubernetes pod-per-session scaling
pub mod k8s;
