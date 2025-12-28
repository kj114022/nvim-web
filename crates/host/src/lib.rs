// nvim-web-host library
// Async Neovim GUI host using nvim-rs and tokio

// Core async modules
pub mod session;
pub mod ws;

// Virtual filesystem layer
pub mod vfs;
pub mod vfs_handlers;

// Configuration
pub mod config;

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
