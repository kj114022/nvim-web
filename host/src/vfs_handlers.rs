//! VFS handlers for opening and writing virtual files
//!
//! TODO: These handlers need to be migrated to async using nvim-rs.
//! Currently stubbed out - VFS functionality is temporarily disabled.

use anyhow::Result;
use crate::vfs::VfsManager;

/// Handle open_vfs RPC command
/// 
/// TODO: Migrate to async using nvim-rs Neovim handle
pub fn handle_open_vfs(
    _vfs_manager: &mut VfsManager,
    _vfs_path: String,
) -> Result<()> {
    eprintln!("VFS: open_vfs not yet implemented in async architecture");
    Ok(())
}

/// Handle write_vfs RPC command
///
/// TODO: Migrate to async using nvim-rs Neovim handle
pub fn handle_write_vfs(
    _vfs_manager: &mut VfsManager,
    _bufnr: u32,
) -> Result<()> {
    eprintln!("VFS: write_vfs not yet implemented in async architecture");
    Ok(())
}
