use std::io::{Read, Write};
use tokio::sync::oneshot;

/// Run the Native Messaging loop (Stdin/Stdout)
///
/// This handles the lifecycle of the host when invoked by the browser extension.
/// Chrome Native Messaging uses length-prefixed JSON (u32 LE).
pub fn run(shutdown_tx: oneshot::Sender<()>) -> anyhow::Result<()> {
    eprintln!("[native] Starting native messaging loop...");

    // Spawn a thread for blocking Stdin operations
    // We cannot do this async easily because Stdin is blocking on some platforms
    std::thread::spawn(move || {
        loop {
            // 1. Read message length (4 bytes)
            let mut len_bytes = [0u8; 4];
            if std::io::stdin().read_exact(&mut len_bytes).is_err() {
                // EOF or error -> Browser disconnected
                eprintln!("[native] Stdin closed, shutting down...");
                break;
            }
            
            let len = u32::from_ne_bytes(len_bytes) as usize;
            
            // 2. Read message body
            let mut msg_bytes = vec![0u8; len];
            if std::io::stdin().read_exact(&mut msg_bytes).is_err() {
                eprintln!("[native] Failed to read message body");
                break;
            }
            
            // 3. Parse JSON (optional, but good for validation)
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&msg_bytes) {
                eprintln!("[native] Received: {json:?}");
                
                // If we receive a "ping" or specific command, we can respond
                // For now, we just echo or acknowledge
            }

            // 4. Send Keep-Alive / Heartbeat back (optional)
            // send_message(&serde_json::json!({ "status": "ok" }));
        }

        // Trigger shutdown when loop exits
        let _ = shutdown_tx.send(());
    });

    Ok(())
}

/// Send a message to Stdout (Length-prefixed JSON)
#[allow(dead_code)]
pub fn send_message(msg: &serde_json::Value) -> anyhow::Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = u32::try_from(json.len())?;
    
    let mut stdout = std::io::stdout();
    stdout.write_all(&len.to_ne_bytes())?;
    stdout.write_all(&json)?;
    stdout.flush()?;
    
    Ok(())
}
