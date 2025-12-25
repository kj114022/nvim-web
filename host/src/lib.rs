// nvim-web-host library
// Async Neovim GUI host using nvim-rs and tokio

// Core async modules
pub mod session;
pub mod ws;

// Virtual filesystem layer
pub mod vfs;
pub mod vfs_handlers;

// Connection management
pub mod connection_manager;

// Configuration
pub mod config;

// REST API
pub mod api;

// Multi-seat collaboration
pub mod collaboration;

// Security hardening
pub mod security;

// Utilities
pub mod debug;
