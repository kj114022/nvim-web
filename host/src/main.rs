use std::sync::Arc;
use tokio::sync::RwLock;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::ws;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting nvim-web host (async mode with nvim-rs)...");
    
    // Create async session manager
    let session_manager = Arc::new(RwLock::new(AsyncSessionManager::new()));
    
    // Start async WebSocket server
    ws::serve_multi_async(session_manager).await?;
    
    Ok(())
}
