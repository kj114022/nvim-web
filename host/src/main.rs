mod nvim;
mod rpc;
mod rpc_buffers;
mod rpc_sync;
mod ws;
mod vfs;
mod vfs_handlers;

fn main() -> anyhow::Result<()> {
    let mut nvim = nvim::Nvim::spawn()?;
    
    // Initialize VFS manager with LocalFs backend
    let mut vfs_manager = vfs::VfsManager::new();
    vfs_manager.register_backend("local", Box::new(vfs::LocalFs::new("/")));
    
    ws::serve(&mut nvim, vfs_manager)?;
    Ok(())
}
