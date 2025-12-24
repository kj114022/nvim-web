use std::net::{TcpListener, TcpStream};
use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use tungstenite::{accept, Message, WebSocket};
use anyhow::Result;
use rmpv::Value;

use crate::nvim::Nvim;
use crate::vfs::{VfsManager, LocalFs, BrowserFsBackend};

pub fn serve(nvim: &mut Nvim) -> Result<()> {
    let server = TcpListener::bind("0.0.0.0:9001")?;
    println!("WebSocket server listening on ws://0.0.0.0:9001");

    // Attach UI ONCE at startup, before accepting any connections
    println!("Attaching Neovim UI...");
    let nvim_stdin = nvim.stdin.as_mut().expect("stdin not available");
    crate::rpc::attach_ui(nvim_stdin)?;
    println!("Waiting for UI attach response...");
    let nvim_stdout_ref = nvim.stdout.as_mut().expect("stdout not available");
    let _response = crate::rpc::read_message(nvim_stdout_ref)?;
    println!("UI attached successfully, ready for connections");

    // Create a persistent channel for Neovim redraw events
    // This channel lives across multiple WebSocket connections
    let (nvim_tx, nvim_rx) = mpsc::channel::<Vec<u8>>();
    let nvim_rx = Arc::new(Mutex::new(nvim_rx));
    
    // Spawn the Neovim reader thread ONCE, before accepting connections
    // This thread reads from Neovim stdout and sends to the channel
    // Take ownership of stdout for the reader thread (safe, no unsafe pointer read)
    let mut nvim_stdout = nvim.stdout.take()
        .expect("stdout already taken - only one reader thread allowed");
    
    thread::spawn(move || {
        loop {
            match crate::rpc::read_message(&mut nvim_stdout) {
                Ok(msg) => {
                    // Handle RPC responses (type 1) - route to sync module
                    if let rmpv::Value::Array(ref arr) = msg {
                        if !arr.is_empty() {
                            if let rmpv::Value::Integer(msg_type) = &arr[0] {
                                if msg_type.as_u64() == Some(1) {
                                    if let Err(e) = crate::rpc_sync::handle_rpc_response(&msg) {
                                        eprintln!("Error handling RPC response: {}", e);
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                    
                    // Handle redraw notifications (type 2) - send to channel
                    if crate::rpc::is_redraw(&msg) {
                        let mut bytes = Vec::new();
                        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                            // Channel send - if all receivers are gone, just drop the message
                            // This happens when no browser is connected, which is fine
                            let _ = nvim_tx.send(bytes);
                        }
                    }
                }
                Err(_) => {
                    eprintln!("Neovim stdout closed, exiting reader thread");
                    break;
                }
            }
        }
    });

    // Accept connections in a loop - new connections replace old ones
    loop {
        println!("Waiting for UI connection...");
        match server.accept() {
            Ok((stream, addr)) => {
                println!("UI connected from {:?}", addr);
                
                match accept(stream) {
                    Ok(mut websocket) => {
                        println!("WebSocket handshake complete");
                        
                        // Drain any stale messages from channel before starting bridge
                        // This ensures the new client gets fresh state, not cached old redraws
                        let rx_lock = nvim_rx.lock().unwrap();
                        while rx_lock.try_recv().is_ok() {}
                        drop(rx_lock);
                        
                        // Request a full redraw for the new connection
                        // Use minimal 1x1 resize to trigger Neovim state re-emit
                        // Browser sends actual viewport size immediately after WS open
                        eprintln!("Requesting full redraw for new connection...");
                        if let Some(ref mut stdin) = nvim.stdin {
                            let _ = crate::rpc::send_notification(
                                stdin,
                                "nvim_ui_try_resize",
                                vec![
                                    Value::Integer(1.into()),
                                    Value::Integer(1.into()),
                                ],
                            );
                        }
                        
                        // Run the bridge with the shared channel
                        if let Err(e) = bridge(nvim, &mut websocket, nvim_rx.clone()) {
                            eprintln!("Bridge error: {}", e);
                        }
                        
                        println!("UI disconnected, waiting for new connection...");
                    }
                    Err(e) => {
                        eprintln!("WebSocket handshake failed: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Accept failed: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn bridge(
    nvim: &mut Nvim, 
    ws: &mut WebSocket<TcpStream>,
    nvim_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>
) -> Result<()> {
    eprintln!("Starting bridge for new connection");
    
    // Initialize VFS Manager with backends
    let mut vfs_manager = VfsManager::new();
    let (to_ws_tx, _to_ws_rx) = mpsc::channel::<Vec<u8>>();
    
    let local_backend = Box::new(LocalFs::new("/tmp/nvim-web"));
    vfs_manager.register_backend("local", local_backend);
    
    let browser_backend = Box::new(BrowserFsBackend::new("default", to_ws_tx.clone()));
    vfs_manager.register_backend("browser", browser_backend);
    eprintln!("VFS backends registered: local, browser (ssh on-demand)");
    
    // Channel for browser->Neovim messages
    let (to_nvim_tx, to_nvim_rx) = mpsc::channel::<Vec<u8>>();
    
    // Channel to signal when WS reader exits
    let (ws_done_tx, ws_done_rx) = mpsc::channel::<()>();
    
    // Thread to read from WebSocket and forward to Neovim
    let ws_stream = ws.get_ref().try_clone()?;
    let mut ws_read = WebSocket::from_raw_socket(ws_stream, tungstenite::protocol::Role::Server, None);
    
    thread::spawn(move || {
        loop {
            match ws_read.read() {
                Ok(Message::Binary(data)) => {
                    if to_nvim_tx.send(data).is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(_) => {
                    break;
                }
                _ => {}
            }
        }
        let _ = ws_done_tx.send(());
    });
    
    // Main bridge loop
    loop {
        // Check if browser disconnected
        if ws_done_rx.try_recv().is_ok() {
            eprintln!("HOST: Browser disconnected, exiting bridge");
            break;
        }
        
        // Forward Neovim messages to WebSocket (from shared channel)
        if let Ok(rx) = nvim_rx.try_lock() {
            if let Ok(bytes) = rx.try_recv() {
                drop(rx); // Release lock before blocking WS send
                if ws.send(Message::Binary(bytes)).is_err() {
                    eprintln!("HOST: WS send failed!");
                    break;
                }
            }
        }
        
        // Forward WebSocket messages to Neovim
        if let Ok(data) = to_nvim_rx.try_recv() {
            let mut cursor = std::io::Cursor::new(data);
            if let Ok(value) = rmpv::decode::read_value(&mut cursor) {
                if let Value::Array(ref arr) = value {
                    if !arr.is_empty() {
                        if let Value::Integer(msg_type) = &arr[0] {
                            if msg_type.as_u64() == Some(3) {
                                if let Err(e) = crate::rpc_sync::handle_fs_response(&value) {
                                    eprintln!("Error handling FS response: {}", e);
                                }
                                continue;
                            }
                        }
                    }
                }
                
                if let Some(ref mut stdin) = nvim.stdin {
                    if handle_browser_message(&value, stdin, &mut vfs_manager).is_err() {
                        break;
                    }
                }
            }
        }
        
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    
    println!("Bridge loop exited");
    Ok(())
}

fn handle_browser_message(msg: &Value, nvim_stdin: &mut impl Write, vfs_manager: &mut VfsManager) -> Result<()> {
    if let Value::Array(arr) = msg {
        if arr.len() >= 2 {
            if let Value::String(method) = &arr[0] {
                match method.as_str() {
                    Some("open_vfs") => {
                        if let Value::String(path) = &arr[1] {
                            if let Some(path_str) = path.as_str() {
                                if let Err(e) = crate::vfs_handlers::handle_open_vfs(
                                    vfs_manager,
                                    nvim_stdin,
                                    path_str.to_string(),
                                ) {
                                    eprintln!("VFS open error: {}", e);
                                    let error_msg = format!("VFS open failed: {}", e);
                                    let _ = crate::rpc::send_notification(
                                        nvim_stdin,
                                        "nvim_err_writeln",
                                        vec![Value::String(error_msg.into())],
                                    );
                                }
                            }
                        }
                    }
                    Some("write_vfs") => {
                        if let Value::Integer(bufnr) = &arr[1] {
                            if let Some(bufnr_u64) = bufnr.as_u64() {
                                if let Err(e) = crate::vfs_handlers::handle_write_vfs(
                                    vfs_manager,
                                    nvim_stdin,
                                    bufnr_u64 as u32,
                                ) {
                                    eprintln!("VFS write error: {}", e);
                                    let error_msg = format!("VFS write failed: {}", e);
                                    let _ = crate::rpc::send_notification(
                                        nvim_stdin,
                                        "nvim_err_writeln",
                                        vec![Value::String(error_msg.into())],
                                    );
                                }
                            }
                        }
                    }
                    Some("input") => {
                        if let Value::String(keys) = &arr[1] {
                            if let Some(key_str) = keys.as_str() {
                                crate::rpc::send_input(nvim_stdin, key_str)?;
                            }
                        }
                    }
                    Some("resize") => {
                        if arr.len() >= 3 {
                            if let (Value::Integer(cols), Value::Integer(rows)) = (&arr[1], &arr[2]) {
                                let cols = cols.as_u64().unwrap_or(80);
                                let rows = rows.as_u64().unwrap_or(24);
                                crate::rpc::send_resize(nvim_stdin, cols, rows)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
