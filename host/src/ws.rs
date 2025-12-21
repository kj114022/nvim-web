use std::net::{TcpListener, TcpStream};
use std::io::Write;
use std::sync::mpsc;
use std::thread;
use tungstenite::{accept, Message, WebSocket};
use anyhow::Result;
use rmpv::Value;

use crate::nvim::Nvim;
use crate::vfs::{VfsManager, LocalFs, BrowserFsBackend};

pub fn serve(nvim: &mut Nvim) -> Result<()> {
    let server = TcpListener::bind("0.0.0.0:9001")?;
    println!("WebSocket server listening on ws://0.0.0.0:9001");

    let (stream, _) = server.accept()?;
    println!("Client connected");

    let mut websocket = accept(stream)?;
    
    bridge(nvim, &mut websocket)?;
    
    Ok(())
}

fn bridge(nvim: &mut Nvim, ws: &mut WebSocket<TcpStream>) -> Result<()> {
    // Send initial UI attach
    crate::rpc::attach_ui(&mut nvim.stdin)?;
    
    // Wait for UI attach response to ensure Neovim is ready
    // Read and discard the response message
    eprintln!("Waiting for UI attach response...");
    let _response = crate::rpc::read_message(&mut nvim.stdout)?;
    eprintln!("UI attach complete, starting bridge");
    
    // Register VFS autocmd for BufWriteCmd
    eprintln!("Registering VFS BufWriteCmd autocmd...");
    let autocmd_script = r#"
augroup NvimWebVFS
  autocmd!
  autocmd BufWriteCmd vfs://* call rpcnotify(0, "write_vfs", bufnr())
augroup END
"#;
    
    crate::rpc_sync::rpc_call(
        &mut nvim.stdin,
        "nvim_command",
        vec![rmpv::Value::String(autocmd_script.into())],
    )?;
    eprintln!("VFS autocmd registered successfully");
    
    // Initialize VFS Manager with backends
    let mut vfs_manager = VfsManager::new();
    
    // Register LocalFs backend for vfs://local/...
    let local_backend = Box::new(LocalFs::new("/tmp/nvim-web"));
    vfs_manager.register_backend("local", local_backend);
    
    // Create channels for thread communication
    let (to_ws_tx, to_ws_rx) = mpsc::channel::<Vec<u8>>();
    let (to_nvim_tx, to_nvim_rx) = mpsc::channel::<Vec<u8>>();
    
    // Register BrowserFs backend for vfs://browser/...
    // Pass channel sender so backend can send FS requests to WS
    let browser_backend = Box::new(BrowserFsBackend::new("default", to_ws_tx.clone()));
    vfs_manager.register_backend("browser", browser_backend);
    
    // Note: SSH backend (vfs://ssh/...) is created dynamically on-demand
    // because it requires connection info in the URI. See VfsManager::read_file/write_file
    eprintln!("VFS backends registered: local, browser (ssh on-demand)");
    
    // Thread 1: Read from Neovim stdout, forward redraw events to WebSocket
    let mut nvim_stdout = unsafe { 
        // SAFETY: We're moving ownership of stdout to this thread
        // The main thread will not touch it again
        std::ptr::read(&nvim.stdout as *const _)
    };
    
    thread::spawn(move || {
        loop {
            match crate::rpc::read_message(&mut nvim_stdout) {
                Ok(msg) => {
                    // Handle RPC responses (type 1)
                    if let rmpv::Value::Array(ref arr) = msg {
                        if !arr.is_empty() {
                            if let rmpv::Value::Integer(msg_type) = &arr[0] {
                                if msg_type.as_u64() == Some(1) {
                                    // RPC response - route to rpc_sync
                                    if let Err(e) = crate::rpc_sync::handle_rpc_response(&msg) {
                                        eprintln!("Error handling RPC response: {}", e);
                                    }
                                    continue; // Don't send to WebSocket
                                }
                            }
                        }
                    }
                    
                    // Handle redraw notifications (type 2)
                    if crate::rpc::is_redraw(&msg) {
                        // Debug: print raw redraw event to stderr
                        eprintln!("RAW_REDRAW: {:?}", msg);
                        
                        let mut bytes = Vec::new();
                        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                            if to_ws_tx.send(bytes).is_err() {
                                break; // Channel closed, exit thread
                            }
                        }
                    }
                }
                Err(_) => break, // Neovim exited
            }
        }
    });
    
    // Thread 2: Read from WebSocket, forward input to Neovim
    let ws_stream = ws.get_ref().try_clone()?;
    let mut ws_read = WebSocket::from_raw_socket(ws_stream, tungstenite::protocol::Role::Server, None);
    
    thread::spawn(move || {
        loop {
            match ws_read.read() {
                Ok(Message::Binary(data)) => {
                    if to_nvim_tx.send(data).is_err() {
                        break; // Channel closed, exit thread
                    }
                }
                Ok(Message::Close(_)) | Err(_) => {
                    break; // WebSocket closed or error
                }
                _ => {} // Ignore other message types
            }
        }
    });
    
    // Main loop: Forward messages between channels and endpoints
    loop {
        // Forward Neovim messages to WebSocket
        if let Ok(bytes) = to_ws_rx.try_recv() {
            if ws.send(Message::Binary(bytes)).is_err() {
                break;
            }
        }
        
        // Forward WebSocket messages to Neovim/VFS
        if let Ok(data) = to_nvim_rx.try_recv() {
            let mut cursor = std::io::Cursor::new(data);
            if let Ok(value) = rmpv::decode::read_value(&mut cursor) {
                // Check if this is an FS response (type 3)
                if let Value::Array(ref arr) = value {
                    if !arr.is_empty() {
                        if let Value::Integer(msg_type) = &arr[0] {
                            if msg_type.as_u64() == Some(3) {
                                // FS response from browser - route to rpc_sync
                                if let Err(e) = crate::rpc_sync::handle_fs_response(&value) {
                                    eprintln!("Error handling FS response: {}", e);
                                }
                                continue; // Don't process as browser message
                            }
                        }
                    }
                }
                
                if handle_browser_message(&value, &mut nvim.stdin, &mut vfs_manager).is_err() {
                    break;
                }
            }
        }
        
        // Small sleep to avoid busy loop
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
                        // Handle VFS file open: ["open_vfs", path]
                        if let Value::String(path) = &arr[1] {
                            if let Some(path_str) = path.as_str() {
                                if let Err(e) = crate::vfs_handlers::handle_open_vfs(
                                    vfs_manager,
                                    nvim_stdin,
                                    path_str.to_string(),
                                ) {
                                    eprintln!("VFS open error: {}", e);
                                    // Notify Neovim of the error
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
                        // Handle VFS buffer write: ["write_vfs", bufnr]
                        if let Value::Integer(bufnr) = &arr[1] {
                            if let Some(bufnr_u64) = bufnr.as_u64() {
                                if let Err(e) = crate::vfs_handlers::handle_write_vfs(
                                    vfs_manager,
                                    nvim_stdin,
                                    bufnr_u64 as u32,
                                ) {
                                    eprintln!("VFS write error: {}", e);
                                    // Notify Neovim of the error
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
                        // arr[1] should be the key string
                        if let Value::String(keys) = &arr[1] {
                            if let Some(key_str) = keys.as_str() {
                                crate::rpc::send_input(nvim_stdin, key_str)?;
                            }
                        }
                    }
                    Some("resize") => {
                        // arr[1] = rows, arr[2] = cols
                        if arr.len() >= 3 {
                            if let (Value::Integer(rows), Value::Integer(cols)) = (&arr[1], &arr[2]) {
                                crate::rpc::send_resize(nvim_stdin, rows.as_u64().unwrap_or(24), cols.as_u64().unwrap_or(80))?;
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
