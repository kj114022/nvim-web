//! VFS handlers for opening and writing virtual files
//!
//! Status: DISABLED - VFS features require async migration
//!
//! The VFS handlers were designed to intercept Neovim file operations
//! and route them through the virtual filesystem. This allows:
//!
//! - Opening files from browser OPFS storage
//! - Opening files via SSH/SFTP
//! - Seamless file access across different backends
//!
//! The handlers are currently disabled because they require integration
//! with the async nvim-rs API to forward RPC calls. The underlying VFS
//! backends (LocalFs, SSH) work correctly - only the Neovim integration
//! is pending.
//!
//! To re-enable VFS:
//! 1. Add VFS RPC message types to ws.rs handle_browser_message
//! 2. Pass the Neovim handle to VFS handlers for buffer operations
//! 3. Implement async versions of handle_open_vfs and handle_write_vfs
//!
//! For now, use Neovim's native `:edit` command for file operations.

// Note: This module is intentionally minimal.
// The VFS backends (vfs/local.rs, vfs/ssh.rs) remain fully functional
// for non-Neovim use cases (e.g., CLI tools, tests).

/// VFS handler stub - not currently implemented
/// 
/// To open VFS files, use native Neovim commands:
/// - `:edit /path/to/file` for local files
/// - Remote files are not yet supported via VFS
#[allow(dead_code)]
pub fn vfs_status() -> &'static str {
    "VFS handler integration is pending async migration"
}
