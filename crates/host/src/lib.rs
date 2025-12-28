// nvim-web-host library
// Async Neovim GUI host using nvim-rs and tokio
pub mod native;

// Core async modules
pub mod session;
pub mod ws;

// Virtual filesystem layer
// pub mod vfs;
pub use nvim_web_vfs as vfs;
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
