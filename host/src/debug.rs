//! Debug utilities for nvim-web
//!
//! Provides conditional logging based on NVIM_WEB_DEBUG environment variable.
//! This module is intentionally kept for future debugging needs.

#![allow(dead_code)]

use std::sync::OnceLock;

static DEBUG: OnceLock<bool> = OnceLock::new();

/// Check if debug mode is enabled via NVIM_WEB_DEBUG environment variable
///
/// This is cached on first access for zero overhead in subsequent checks.
pub fn enabled() -> bool {
    *DEBUG.get_or_init(|| std::env::var("NVIM_WEB_DEBUG").is_ok())
}

/// Log a debug message with category prefix
///
/// Categories: rpc, vfs, ws, ui
pub fn log(category: &str, msg: impl std::fmt::Display) {
    if enabled() {
        eprintln!("[{}] {}", category, msg);
    }
}
