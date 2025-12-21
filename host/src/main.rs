mod nvim;
mod rpc;
mod rpc_buffers;
mod rpc_sync;
mod ws;
mod vfs;
mod vfs_handlers;

fn main() -> anyhow::Result<()> {
    println!("Starting nvim-web host...");
    
    // Set up Neovim
    let mut nvim = nvim::Nvim::spawn()?;
    
    // Start WebSocket server and bridge
    // VfsManager is now created inside bridge() function
    ws::serve(&mut nvim)?;
    
    Ok(())
}
