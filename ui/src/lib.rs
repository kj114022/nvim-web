use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, WebSocket, MessageEvent, KeyboardEvent, ResizeObserver, ResizeObserverEntry};
use std::rc::Rc;
use std::cell::RefCell;

mod grid;
mod highlight;
mod renderer;
mod input;
mod render;
mod events;

use grid::GridManager;
use highlight::HighlightMap;
use renderer::Renderer;
use input::InputQueue;
use render::RenderState;
use events::apply_redraw;

// JavaScript OPFS bridge - calls handleFsRequest from opfs.ts
#[wasm_bindgen(module = "/fs/opfs.js")]
extern "C" {
    #[wasm_bindgen(js_name = handleFsRequest, catch)]
    async fn js_handle_fs_request(
        op: &str,
        ns: &str,
        path: &str,
        data: Option<js_sys::Uint8Array>,
        id: u32,
    ) -> Result<JsValue, JsValue>;
}

/// Set connection status indicator (connected/connecting/disconnected)
fn set_status(status: &str) {
    if let Some(doc) = window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("nvim-status") {
            let _ = el.set_class_name(&format!("status-{}", status));
        }
    }
}

/// Show a toast notification (auto-hides after 3 seconds)
fn show_toast(message: &str) {
    if let Some(doc) = window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("nvim-toast") {
            el.set_text_content(Some(message));
            let _ = el.set_attribute("class", "show");
            
            // Auto-hide after 3 seconds
            let el_clone = el.clone();
            let callback = Closure::once(Box::new(move || {
                let _ = el_clone.set_attribute("class", "");
            }) as Box<dyn FnOnce()>);
            let _ = window().unwrap().set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                3000,
            );
            callback.forget();
        }
    }
}

/// Set dirty state indicator (unsaved changes)
fn set_dirty(dirty: bool) {
    if let Some(doc) = window().and_then(|w| w.document()) {
        // Update dirty indicator visibility
        if let Some(el) = doc.get_element_by_id("nvim-dirty") {
            let _ = el.set_attribute("class", if dirty { "show" } else { "" });
        }
        // Update page title
        let base_title = "Neovim Web";
        let new_title = if dirty { format!("* {}", base_title) } else { base_title.to_string() };
        doc.set_title(&new_title);
    }
}

/// Focus the hidden input textarea (for IME/mobile)
fn focus_input() {
    if let Some(doc) = window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("nvim-input") {
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html_el.focus();
            }
        }
    }
}

/// Update drawer status bar with session ID
fn update_drawer_session(session_id: &str, is_reconnection: bool) {
    if let Some(win) = window() {
        // Call window.__drawer.setSession(id, isReconnect)
        if let Ok(drawer) = js_sys::Reflect::get(&win, &"__drawer".into()) {
            if !drawer.is_undefined() {
                if let Ok(set_session) = js_sys::Reflect::get(&drawer, &"setSession".into()) {
                    if let Some(func) = set_session.dyn_ref::<js_sys::Function>() {
                        let _ = func.call2(&drawer, &session_id.into(), &is_reconnection.into());
                    }
                }
            }
        }
    }
}

/// Update drawer with CWD info (backend, cwd, git branch)
fn update_drawer_cwd_info(cwd: &str, file: &str, backend: &str, git_branch: Option<&str>) {
    if let Some(win) = window() {
        if let Ok(drawer) = js_sys::Reflect::get(&win, &"__drawer".into()) {
            if drawer.is_undefined() {
                return;
            }
            
            // Set CWD
            if let Ok(set_cwd) = js_sys::Reflect::get(&drawer, &"setCwd".into()) {
                if let Some(func) = set_cwd.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &cwd.into());
                }
            }
            
            // Set file
            if let Ok(set_file) = js_sys::Reflect::get(&drawer, &"setFile".into()) {
                if let Some(func) = set_file.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &file.into());
                }
            }
            
            // Set backend
            if let Ok(set_backend) = js_sys::Reflect::get(&drawer, &"setBackend".into()) {
                if let Some(func) = set_backend.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &backend.into());
                }
            }
            
            // Set git branch
            if let Ok(set_git) = js_sys::Reflect::get(&drawer, &"setGitBranch".into()) {
                if let Some(func) = set_git.dyn_ref::<js_sys::Function>() {
                    let branch_js: JsValue = match git_branch {
                        Some(b) => b.into(),
                        None => JsValue::NULL,
                    };
                    let _ = func.call1(&drawer, &branch_js);
                }
            }
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();
    let canvas = document
        .get_element_by_id("nvim")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    let renderer = Renderer::new(canvas.clone());
    
    // Get initial size from canvas CSS dimensions
    let (cell_w, cell_h) = renderer.cell_size();
    let css_width = canvas.client_width() as f64;
    let css_height = canvas.client_height() as f64;
    let initial_cols = (css_width / cell_w).floor() as usize;
    let initial_rows = (css_height / cell_h).floor() as usize;
    
    let grids = Rc::new(RefCell::new(GridManager::new()));
    let renderer = Rc::new(renderer);
    
    // Phase 9.2.1: Highlight storage (needed for RenderState)
    let highlights = Rc::new(RefCell::new(HighlightMap::new()));

    // Apply initial HiDPI scaling
    renderer.resize(css_width, css_height);

    // Resize main grid to match viewport
    grids.borrow_mut().resize_grid(1, initial_rows.max(24), initial_cols.max(80));

    // Create render state for batching
    let render_state = RenderState::new(grids.clone(), highlights.clone(), renderer.clone());

    // Initial render
    render_state.render_now();

    // Session management: check URL params first, then localStorage
    let win = window().unwrap();
    let search = win.location().search().unwrap_or_default();
    let storage = win.local_storage().ok().flatten();
    
    // Parse ?session= from URL (handles both ?session=x and &session=x)
    let url_session: Option<String> = if search.contains("session=") {
        let search_clean = search.trim_start_matches('?');
        search_clean.split('&')
            .find(|p| p.starts_with("session="))
            .and_then(|p| p.strip_prefix("session="))
            .map(|s| s.to_string())
    } else {
        None
    };
    
    // Parse ?open= from URL (magic link)
    let open_token: Option<String> = if search.contains("open=") {
        let search_clean = search.trim_start_matches('?');
        search_clean.split('&')
            .find(|p| p.starts_with("open="))
            .and_then(|p| p.strip_prefix("open="))
            .map(|s| s.to_string())
    } else {
        None
    };
    
    // Store project path if we have an open token (will be used after connection)
    let project_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let project_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    
    // If we have an open token, try to claim it
    if let Some(ref token) = open_token {
        web_sys::console::log_1(&format!("MAGIC LINK: Claiming token {}", token).into());
        // We'll use fetch to claim the token (synchronously via spawn_local later)
        // For now, store the token - we'll claim it after WS connects
        project_path.borrow_mut().replace(token.clone());
        show_toast(&format!("Opening project..."));
    }
    
    // Determine session ID: URL param takes priority over localStorage
    // Track if this is a reconnection to show toast
    let (ws_url, should_clear_url, _is_reconnection) = match url_session {
        Some(ref id) if id == "new" => {
            // Force new session - clear localStorage
            if let Some(ref s) = storage {
                let _ = s.remove_item("nvim_session_id");
            }
            web_sys::console::log_1(&"SESSION: Forcing new session (URL param)".into());
            ("ws://127.0.0.1:9001?session=new".to_string(), true, false)
        }
        Some(ref id) => {
            // Join specific session from URL
            web_sys::console::log_1(&format!("SESSION: Joining session {} (URL param)", id).into());
            (format!("ws://127.0.0.1:9001?session={}", id), true, true)
        }
        None if open_token.is_some() => {
            // Magic link - always create new session
            web_sys::console::log_1(&"SESSION: Creating new session for magic link".into());
            ("ws://127.0.0.1:9001?session=new".to_string(), true, false)
        }
        None => {
            // No URL param, check localStorage
            let existing_session = storage.as_ref()
                .and_then(|s| s.get_item("nvim_session_id").ok())
                .flatten();
            
            match existing_session {
                Some(ref id) => {
                    web_sys::console::log_1(&format!("SESSION: Reconnecting to session {}", id).into());
                    (format!("ws://127.0.0.1:9001?session={}", id), false, true)
                }
                None => {
                    web_sys::console::log_1(&"SESSION: Creating new session".into());
                    ("ws://127.0.0.1:9001?session=new".to_string(), false, false)
                }
            }
        }
    };
    
    // Clean URL after reading session param (removes ?session= and ?open= from address bar)
    if should_clear_url {
        if let Ok(history) = win.history() {
            let pathname = win.location().pathname().unwrap_or_default();
            let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&pathname));
        }
    }

    // Connect to WebSocket with session support
    web_sys::console::log_1(&format!("WS CREATING: {}", ws_url).into());
    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => {
            web_sys::console::log_1(&"WS CREATED".into());
            ws
        }
        Err(e) => {
            web_sys::console::error_1(&"WS CREATE FAILED".into());
            web_sys::console::error_1(&e);
            return Err(e);
        }
    };
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // Expose WS to window for debugging
    let _ = js_sys::Reflect::set(
        &window().unwrap(),
        &"__nvim_ws".into(),
        &ws.clone().into(),
    );

    // WS lifecycle: onopen - send initial resize with actual viewport size
    let ws_open = ws.clone();
    let initial_rows_send = initial_rows.max(24);
    let initial_cols_send = initial_cols.max(80);
    let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
        web_sys::console::log_1(&"WS OPEN".into());
        set_status("connected");
        
        // Send initial resize to tell Neovim the actual browser viewport size
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("resize".into()),
            rmpv::Value::Integer((initial_cols_send as i64).into()),
            rmpv::Value::Integer((initial_rows_send as i64).into()),
        ]);
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            let _ = ws_open.send_with_u8_array(&bytes);
            web_sys::console::log_1(&format!("Sent initial resize: {}x{}", initial_cols_send, initial_rows_send).into());
        }
        
        // Request settings from host (RPC call: settings_all)
        let settings_req = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),  // Type 0 = RPC request
            rmpv::Value::Integer(1.into()),  // Request ID
            rmpv::Value::String("settings_all".into()),
            rmpv::Value::Array(vec![]),      // No params
        ]);
        let mut settings_bytes = Vec::new();
        if rmpv::encode::write_value(&mut settings_bytes, &settings_req).is_ok() {
            let _ = ws_open.send_with_u8_array(&settings_bytes);
            web_sys::console::log_1(&"SETTINGS: Requested from host".into());
        }
    }) as Box<dyn FnMut(_)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // WS lifecycle: onerror
    let onerror = Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
        web_sys::console::error_1(&"WS ERROR".into());
        web_sys::console::error_1(&e);
        set_status("disconnected");
        show_toast("Connection error");
    }) as Box<dyn FnMut(_)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // WS lifecycle: onclose - clear session on abnormal close
    let onclose = Closure::wrap(Box::new(move |e: web_sys::CloseEvent| {
        web_sys::console::warn_1(&"WS CLOSE".into());
        web_sys::console::warn_1(&format!("code={}, reason={}", e.code(), e.reason()).into());
        
        // If abnormal close (not 1000), clear session ID to force new session on reconnect
        if e.code() != 1000 && e.code() != 1001 {
            set_status("disconnected");
            show_toast("Disconnected from host");
            if let Some(s) = window().unwrap().local_storage().ok().flatten() {
                let _ = s.remove_item("nvim_session_id");
                web_sys::console::warn_1(&"SESSION: Cleared due to abnormal disconnect".into());
            }
        }
    }) as Box<dyn FnMut(_)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();


    // Handle incoming redraw events with batching
    let grids_msg = grids.clone();
    let render_state_msg = render_state.clone();
    let highlights_msg = highlights.clone();
    let ws_fs = ws.clone(); // Clone for FS response sending
    let ws_rpc = ws.clone(); // Clone for RPC request sending (cwd_info)
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        web_sys::console::log_1(&"WS MESSAGE".into());
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            
            // Decode msgpack message
            let mut cursor = std::io::Cursor::new(bytes);
            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                // Check message type for routing
                if let rmpv::Value::Array(ref arr) = msg {
                    if arr.len() >= 2 {
                        // Type 2 messages: differentiate between FS requests and redraw notifications
                        // FS request: [2, <integer_id>, [op, ns, path, data?]] - arr[1] is integer
                        // Redraw:     [2, "redraw", [...events...]] - arr[1] is string
                        if let rmpv::Value::Integer(ref msg_type) = arr[0] {
                            if msg_type.as_i64() == Some(2) && arr.len() >= 3 {
                                // Check if arr[1] is an integer (FS request ID) not a string (e.g., "redraw")
                                if let rmpv::Value::Integer(ref fs_id) = arr[1] {
                                    // This is an FS request, not a redraw notification
                                    // Format: [2, id, [op, ns, path, data?]]
                                    let request_id = fs_id.as_u64().unwrap_or(0) as u32;
                                    
                                    // Parse the request payload at arr[2]
                                    if let rmpv::Value::Array(ref payload) = arr[2] {
                                        if payload.len() >= 3 {
                                            let op = payload[0].as_str().unwrap_or("").to_string();
                                            let ns = payload[1].as_str().unwrap_or("default").to_string();
                                            let path = payload[2].as_str().unwrap_or("").to_string();
                                            
                                            // Extract optional data for write operations
                                            let data: Option<Vec<u8>> = if payload.len() >= 4 {
                                                if let rmpv::Value::Binary(ref bytes) = payload[3] {
                                                    Some(bytes.clone())
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };
                                            
                                            web_sys::console::log_1(&format!(
                                                "FS: Request id={} op={} ns={} path={}", 
                                                request_id, op, ns, path
                                            ).into());
                                            
                                            // Clone WebSocket for use in async context
                                            let ws_response = ws_fs.clone();
                                            
                                            // Spawn async task to handle FS request
                                            wasm_bindgen_futures::spawn_local(async move {
                                                // Prepare data as Uint8Array for JS
                                                let js_data = data.map(|bytes| {
                                                    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                                                    arr.copy_from(&bytes);
                                                    arr
                                                });
                                                
                                                // Call the JavaScript OPFS handler
                                                let result = js_handle_fs_request(
                                                    &op, &ns, &path, js_data, request_id
                                                ).await;
                                                
                                                // Build Type 3 response: [3, id, ok, result]
                                                let response = match result {
                                                    Ok(js_result) => {
                                                        // Parse JS result object: { ok, result, error?, id }
                                                        let ok = js_sys::Reflect::get(&js_result, &"ok".into())
                                                            .ok()
                                                            .and_then(|v| v.as_bool())
                                                            .unwrap_or(false);
                                                        
                                                        if ok {
                                                            // Success - get result
                                                            let result_val = js_sys::Reflect::get(&js_result, &"result".into())
                                                                .ok();
                                                            
                                                            // Convert JS result to msgpack Value
                                                            let msgpack_result = if let Some(val) = result_val {
                                                                if val.is_null() || val.is_undefined() {
                                                                    rmpv::Value::Nil
                                                                } else if let Some(arr) = val.dyn_ref::<js_sys::Uint8Array>() {
                                                                    // Binary data (e.g., file content)
                                                                    rmpv::Value::Binary(arr.to_vec())
                                                                } else if let Some(arr) = val.dyn_ref::<js_sys::Array>() {
                                                                    // Array (e.g., file list)
                                                                    let items: Vec<rmpv::Value> = (0..arr.length())
                                                                        .filter_map(|i| {
                                                                            arr.get(i).as_string().map(|s| rmpv::Value::String(s.into()))
                                                                        })
                                                                        .collect();
                                                                    rmpv::Value::Array(items)
                                                                } else {
                                                                    rmpv::Value::Nil
                                                                }
                                                            } else {
                                                                rmpv::Value::Nil
                                                            };
                                                            
                                                            rmpv::Value::Array(vec![
                                                                rmpv::Value::Integer(3.into()),
                                                                rmpv::Value::Integer((request_id as i64).into()),
                                                                rmpv::Value::Boolean(true),
                                                                msgpack_result,
                                                            ])
                                                        } else {
                                                            // Error from JS handler
                                                            let error = js_sys::Reflect::get(&js_result, &"error".into())
                                                                .ok()
                                                                .and_then(|v| v.as_string())
                                                                .unwrap_or_else(|| "Unknown error".to_string());
                                                            
                                                            rmpv::Value::Array(vec![
                                                                rmpv::Value::Integer(3.into()),
                                                                rmpv::Value::Integer((request_id as i64).into()),
                                                                rmpv::Value::Boolean(false),
                                                                rmpv::Value::String(error.into()),
                                                            ])
                                                        }
                                                    }
                                                    Err(e) => {
                                                        // JS exception
                                                        let error = e.as_string()
                                                            .unwrap_or_else(|| "JS exception".to_string());
                                                        
                                                        rmpv::Value::Array(vec![
                                                            rmpv::Value::Integer(3.into()),
                                                            rmpv::Value::Integer((request_id as i64).into()),
                                                            rmpv::Value::Boolean(false),
                                                            rmpv::Value::String(error.into()),
                                                        ])
                                                    }
                                                };
                                                
                                                // Encode and send response
                                                let mut bytes = Vec::new();
                                                if rmpv::encode::write_value(&mut bytes, &response).is_ok() {
                                                    if let Err(e) = ws_response.send_with_u8_array(&bytes) {
                                                        web_sys::console::error_1(&format!(
                                                            "FS: Failed to send response: {:?}", e
                                                        ).into());
                                                    } else {
                                                        web_sys::console::log_1(&format!(
                                                            "FS: Sent response for id={}", request_id
                                                        ).into());
                                                    }
                                                }
                                            });
                                            
                                            return; // FS request is being handled async
                                        }
                                    }
                                    
                                    // Malformed FS request - log and return
                                    web_sys::console::warn_1(&"FS: Malformed request (missing payload)".into());
                                    return;
                                }
                                // If arr[1] is not an integer, fall through to process as redraw
                            }
                            
                            // Type 1: RPC response [1, id, error, result]
                            // Handle settings_all response (id=1 from our request)
                            if msg_type.as_i64() == Some(1) && arr.len() >= 4 {
                                let id = arr[1].as_i64().unwrap_or(0);
                                let error = &arr[2];
                                let result = &arr[3];
                                
                                web_sys::console::log_1(&format!("RPC Response id={}", id).into());
                                
                                // Check for settings_all response (id=1)
                                if id == 1 {
                                    if error.is_nil() {
                                        // Settings received - apply them
                                        web_sys::console::log_1(&"SETTINGS: Received from host".into());
                                        
                                        if let rmpv::Value::Map(ref settings_map) = result {
                                            for (key, value) in settings_map {
                                                if let (Some(k), Some(v)) = (key.as_str(), value.as_str()) {
                                                    web_sys::console::log_1(&format!("SETTING: {}={}", k, v).into());
                                                    // Settings are stored - Neovim handles actual rendering
                                                }
                                            }
                                        }
                                    } else {
                                        // Error receiving settings
                                        let error_msg = error.as_str().unwrap_or("Settings error");
                                        web_sys::console::error_1(&format!("SETTINGS ERROR: {}", error_msg).into());
                                    }
                                }
                                
                                // Check for get_cwd_info response (id=2)
                                if id == 2 {
                                    if error.is_nil() {
                                        web_sys::console::log_1(&"CWD INFO: Received from host".into());
                                        
                                        if let rmpv::Value::Map(ref info_map) = result {
                                            let mut cwd = String::new();
                                            let mut file = String::new();
                                            let mut backend = "local".to_string();
                                            let mut git_branch: Option<String> = None;
                                            
                                            for (key, value) in info_map {
                                                match key.as_str() {
                                                    Some("cwd") => cwd = value.as_str().unwrap_or("~").to_string(),
                                                    Some("file") => file = value.as_str().unwrap_or("").to_string(),
                                                    Some("backend") => backend = value.as_str().unwrap_or("local").to_string(),
                                                    Some("git_branch") => git_branch = value.as_str().map(|s| s.to_string()),
                                                    _ => {}
                                                }
                                            }
                                            
                                            web_sys::console::log_1(&format!(
                                                "CWD INFO: backend={} cwd={} file={} git={:?}",
                                                backend, cwd, file, git_branch
                                            ).into());
                                            
                                            update_drawer_cwd_info(&cwd, &file, &backend, git_branch.as_deref());
                                        }
                                    } else {
                                        let error_msg = error.as_str().unwrap_or("CWD info error");
                                        web_sys::console::error_1(&format!("CWD INFO ERROR: {}", error_msg).into());
                                    }
                                }
                                return;
                            }
                        }
                        
                        // Session message: ["session", "<id>"]
                        if let rmpv::Value::String(ref method) = arr[0] {
                            if method.as_str() == Some("session") {
                                if let rmpv::Value::String(ref session_id) = arr[1] {
                                    if let Some(id) = session_id.as_str() {
                                        web_sys::console::log_1(&format!("SESSION: Received session ID: {}", id).into());
                                        // Check if this is a reconnection (session ID already in localStorage)
                                        let is_reconnection = if let Ok(Some(storage)) = window().unwrap().local_storage() {
                                            let existing = storage.get_item("nvim_session_id").ok().flatten();
                                            let is_recon = existing.as_ref().map(|e| e == id).unwrap_or(false);
                                            // Store session ID in localStorage
                                            let _ = storage.set_item("nvim_session_id", id);
                                            web_sys::console::log_1(&"SESSION: Stored in localStorage".into());
                                            is_recon
                                        } else {
                                            false
                                        };
                                        // Update drawer status bar with session
                                        update_drawer_session(id, is_reconnection);
                                        
                                        // Request CWD info for status drawer
                                        let cwd_req = rmpv::Value::Array(vec![
                                            rmpv::Value::Integer(0.into()),  // Type 0 = RPC request
                                            rmpv::Value::Integer(2.into()),  // Request ID (2 for cwd_info)
                                            rmpv::Value::String("get_cwd_info".into()),
                                            rmpv::Value::Array(vec![]),      // No params
                                        ]);
                                        let mut cwd_bytes = Vec::new();
                                        if rmpv::encode::write_value(&mut cwd_bytes, &cwd_req).is_ok() {
                                            let _ = ws_rpc.send_with_u8_array(&cwd_bytes);
                                            web_sys::console::log_1(&"CWD INFO: Requested from host".into());
                                        }
                                        
                                        return; // Session message handled
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Check for cwd_info push message: ["cwd_info", {cwd, file, backend, git_branch}]
                if let rmpv::Value::Array(ref arr) = msg {
                    if arr.len() >= 2 {
                        if let rmpv::Value::String(ref method) = arr[0] {
                            if method.as_str() == Some("cwd_info") {
                                if let rmpv::Value::Map(ref info_map) = arr[1] {
                                    let mut cwd = String::new();
                                    let mut file = String::new();
                                    let mut backend = "local".to_string();
                                    let mut git_branch: Option<String> = None;
                                    
                                    for (key, value) in info_map {
                                        match key.as_str() {
                                            Some("cwd") => cwd = value.as_str().unwrap_or("~").to_string(),
                                            Some("file") => file = value.as_str().unwrap_or("").to_string(),
                                            Some("backend") => backend = value.as_str().unwrap_or("local").to_string(),
                                            Some("git_branch") => git_branch = value.as_str().map(|s| s.to_string()),
                                            _ => {}
                                        }
                                    }
                                    
                                    web_sys::console::log_1(&format!(
                                        "CWD PUSH: backend={} cwd={} file={} git={:?}",
                                        backend, cwd, file, git_branch
                                    ).into());
                                    
                                    update_drawer_cwd_info(&cwd, &file, &backend, git_branch.as_deref());
                                    return; // CWD info handled
                                }
                            }
                        }
                    }
                }
                
                // Not a session/cwd message, process as redraw
                apply_redraw(&mut grids_msg.borrow_mut(), &mut highlights_msg.borrow_mut(), &msg);
                
                // Schedule render via RAF (batched, at most once per frame)
                render_state_msg.request_render();
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // D1.1: ResizeObserver for window resize handling
    let grids_resize = grids.clone();
    let renderer_resize = renderer.clone();
    let render_state_resize = render_state.clone();
    let ws_resize = ws.clone();
    let resize_callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
        for i in 0..entries.length() {
            if let Ok(entry) = entries.get(i).dyn_into::<ResizeObserverEntry>() {
                let rect = entry.content_rect();
                let css_width = rect.width();
                let css_height = rect.height();

                // D1 + D2: Resize canvas with HiDPI handling
                let (new_rows, new_cols) = renderer_resize.resize(css_width, css_height);

                // Update grid dimensions
                grids_resize.borrow_mut().resize_grid(1, new_rows, new_cols);

                // D1.2: Send ui_try_resize to Neovim
                let msg = rmpv::Value::Array(vec![
                    rmpv::Value::String("resize".into()),
                    rmpv::Value::Integer((new_cols as i64).into()),
                    rmpv::Value::Integer((new_rows as i64).into()),
                ]);
                let mut bytes = Vec::new();
                if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                    let _ = ws_resize.send_with_u8_array(&bytes);
                }

                // D1.3: Immediate full redraw (resize is special)
                render_state_resize.render_now();
            }
        }
    }) as Box<dyn FnMut(_)>);

    let observer = ResizeObserver::new(resize_callback.as_ref().unchecked_ref())?;
    observer.observe(&canvas);
    resize_callback.forget();

    // Get the focusable wrapper div (canvas focus is unreliable across browsers)
    let editor_root = document
        .get_element_by_id("editor-root")
        .unwrap()
        .dyn_into::<web_sys::HtmlElement>()?;

    // Phase 9.1.2: Input queue for decoupled, FIFO input handling
    let input_queue = InputQueue::new(ws.clone());
    
    // Expose input function to JavaScript for drawer Quick Actions
    let input_queue_js = input_queue.clone();
    let nvim_input_fn = Closure::wrap(Box::new(move |cmd: String| {
        input_queue_js.send_key(&cmd);
    }) as Box<dyn FnMut(String)>);
    let _ = js_sys::Reflect::set(
        &window().unwrap(),
        &"__nvim_input".into(),
        nvim_input_fn.as_ref(),
    );
    nvim_input_fn.forget();

    // Handle keyboard input - attach to wrapper, not canvas
    let input_queue_key = input_queue.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let key = e.key();
        let ctrl = e.ctrl_key() || e.meta_key(); // Cmd on Mac
        let shift = e.shift_key();
        let alt = e.alt_key();
        
        // Build Neovim key notation with modifiers
        let nvim_key: String = if ctrl || shift || alt {
            // Handle modifier combinations
            let mut mods = String::new();
            if ctrl { mods.push('C'); mods.push('-'); }
            if shift { mods.push('S'); mods.push('-'); }
            if alt { mods.push('A'); mods.push('-'); }
            
            let base = match key.as_str() {
                "Enter" => "CR",
                "Escape" => "Esc",
                "Backspace" => "BS",
                "Tab" => "Tab",
                "ArrowUp" => "Up",
                "ArrowDown" => "Down",
                "ArrowLeft" => "Left",
                "ArrowRight" => "Right",
                "Delete" => "Del",
                "Home" => "Home",
                "End" => "End",
                "PageUp" => "PageUp",
                "PageDown" => "PageDown",
                k if k.len() == 1 => k,
                _ => return,
            };
            format!("<{}{}>", mods, base)
        } else {
            // No modifiers - simple key
            match key.as_str() {
                "Enter" => "<CR>".to_string(),
                "Escape" => "<Esc>".to_string(),
                "Backspace" => "<BS>".to_string(),
                "Tab" => "<Tab>".to_string(),
                "ArrowUp" => "<Up>".to_string(),
                "ArrowDown" => "<Down>".to_string(),
                "ArrowLeft" => "<Left>".to_string(),
                "ArrowRight" => "<Right>".to_string(),
                "Delete" => "<Del>".to_string(),
                "Home" => "<Home>".to_string(),
                "End" => "<End>".to_string(),
                "PageUp" => "<PageUp>".to_string(),
                "PageDown" => "<PageDown>".to_string(),
                k if k.len() == 1 => k.to_string(),
                _ => return,
            }
        };
        
        // Send to Neovim
        input_queue_key.send_key(&nvim_key);
        
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    
    // Attach keydown to WRAPPER (not canvas) for reliable focus handling
    editor_root.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();

    // IME composition support for CJK input
    let input_queue_compose = input_queue.clone();
    let compositionend = Closure::wrap(Box::new(move |e: web_sys::Event| {
        // Get composed text via JS reflection
        if let Ok(data) = js_sys::Reflect::get(&e, &"data".into()) {
            if let Some(text) = data.as_string() {
                if !text.is_empty() {
                    web_sys::console::log_1(&format!("IME: Composed text: {}", text).into());
                    input_queue_compose.send_key(&text);
                    set_dirty(true);
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    // Attach to hidden textarea for IME events
    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(input_el) = document.get_element_by_id("nvim-input") {
            let _ = input_el.add_event_listener_with_callback("compositionend", compositionend.as_ref().unchecked_ref());
        }
    }
    compositionend.forget();

    // Also handle regular input events from textarea (for paste, etc.)
    let input_queue_input = input_queue.clone();
    let oninput = Closure::wrap(Box::new(move |e: web_sys::Event| {
        // Check if composing via JS reflection
        let is_composing = js_sys::Reflect::get(&e, &"isComposing".into())
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false);
        
        if !is_composing {
            if let Ok(data) = js_sys::Reflect::get(&e, &"data".into()) {
                if let Some(text) = data.as_string() {
                    if !text.is_empty() {
                        input_queue_input.send_key(&text);
                        set_dirty(true);
                    }
                }
            }
        }
        // Clear the textarea via JS reflection
        if let Some(target) = e.target() {
            let _ = js_sys::Reflect::set(&target, &"value".into(), &"".into());
        }
    }) as Box<dyn FnMut(_)>);
    
    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(input_el) = document.get_element_by_id("nvim-input") {
            let _ = input_el.add_event_listener_with_callback("input", oninput.as_ref().unchecked_ref());
        }
    }
    oninput.forget();

    // Phase 9.3: Mouse support - click to position cursor
    let input_queue_mouse = input_queue.clone();
    let renderer_mouse = renderer.clone();
    let canvas_mouse = canvas.clone();
    let editor_root_click = editor_root.clone();
    let onmousedown = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        // Focus the editor and hidden textarea for IME
        let _ = editor_root_click.focus();
        focus_input();
        
        // Get click position relative to canvas (cast to Element for getBoundingClientRect)
        let canvas_element: &web_sys::Element = canvas_mouse.as_ref();
        let rect = canvas_element.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();
        
        // Convert to grid cell coordinates
        let (cell_w, cell_h) = renderer_mouse.cell_size();
        let col = (x / cell_w).floor() as i32;
        let row = (y / cell_h).floor() as i32;
        
        web_sys::console::log_1(&format!("MOUSE: ({},{}) -> cell ({},{})", x, y, col, row).into());
        
        // Send nvim_input_mouse: <button>, <action>, <mods>, <grid>, <row>, <col>
        // For left click: button="left", action="press"
        let mouse_input = format!("<LeftMouse><{},{}>", col, row);
        input_queue_mouse.send_key(&mouse_input);
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("mousedown", onmousedown.as_ref().unchecked_ref())?;
    onmousedown.forget();

    // Phase 9.3: Scroll wheel support
    let input_queue_scroll = input_queue.clone();
    let onwheel = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
        e.prevent_default();
        
        let delta_y = e.delta_y();
        let key = if delta_y > 0.0 {
            "<ScrollWheelDown>"
        } else if delta_y < 0.0 {
            "<ScrollWheelUp>"
        } else {
            return;
        };
        
        web_sys::console::log_1(&format!("SCROLL: {}", key).into());
        input_queue_scroll.send_key(key);
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();


    // Focus the wrapper on startup
    editor_root.focus()?;
    web_sys::console::log_1(&"EDITOR FOCUS INITIALIZED".into());

    // Focus/blur detection for visual feedback
    let grids_focus = grids.clone();
    let render_state_focus = render_state.clone();
    let onfocus = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        web_sys::console::log_1(&"FOCUS EVENT".into());
        if let Some(grid) = grids_focus.borrow_mut().main_grid_mut() {
            grid.is_focused = true;
        }
        render_state_focus.render_now();
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("focus", onfocus.as_ref().unchecked_ref())?;
    onfocus.forget();

    let grids_blur = grids.clone();
    let render_state_blur = render_state.clone();
    let onblur = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        web_sys::console::log_1(&"BLUR EVENT".into());
        if let Some(grid) = grids_blur.borrow_mut().main_grid_mut() {
            grid.is_focused = false;
        }
        render_state_blur.render_now();
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("blur", onblur.as_ref().unchecked_ref())?;
    onblur.forget();

    // Phase 3: Clipboard paste support
    let input_queue_paste = input_queue.clone();
    let onpaste = Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
        e.prevent_default();
        
        if let Some(data) = e.clipboard_data() {
            if let Ok(text) = data.get_data("text/plain") {
                if !text.is_empty() {
                    web_sys::console::log_1(&format!("PASTE: {} chars", text.len()).into());
                    
                    // Send each character to Neovim
                    // For multi-line paste, convert newlines properly
                    for c in text.chars() {
                        let key = match c {
                            '\n' => "<CR>".to_string(),
                            '\r' => continue, // Skip carriage returns
                            '\t' => "<Tab>".to_string(),
                            '<' => "<lt>".to_string(),
                            '\\' => "\\".to_string(),
                            _ => c.to_string(),
                        };
                        input_queue_paste.send_key(&key);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("paste", onpaste.as_ref().unchecked_ref())?;
    onpaste.forget();

    // Phase 5: Mobile touch support
    let input_queue_touch = input_queue.clone();
    let renderer_touch = renderer.clone();
    let canvas_touch = canvas.clone();
    let editor_root_touch = editor_root.clone();
    let ontouchstart = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
        e.prevent_default();
        
        // Focus editor and hidden textarea for mobile keyboard
        let _ = editor_root_touch.focus();
        focus_input();
        
        if let Some(touch) = e.touches().get(0) {
            // Get touch position relative to canvas
            let canvas_element: &web_sys::Element = canvas_touch.as_ref();
            let rect = canvas_element.get_bounding_client_rect();
            let x = touch.client_x() as f64 - rect.left();
            let y = touch.client_y() as f64 - rect.top();
            
            // Convert to grid cell
            let (cell_w, cell_h) = renderer_touch.cell_size();
            let col = (x / cell_w).floor() as i32;
            let row = (y / cell_h).floor() as i32;
            
            web_sys::console::log_1(&format!("TOUCH: ({},{}) -> cell ({},{})", x, y, col, row).into());
            
            // Send as left mouse click
            let mouse_input = format!("<LeftMouse><{},{}>", col, row);
            input_queue_touch.send_key(&mouse_input);
        }
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("touchstart", ontouchstart.as_ref().unchecked_ref())?;
    ontouchstart.forget();

    Ok(())
}
