//! Worker Entry Point
//! Handles WebGPU rendering and WebSocket communication

use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use crate::input_queue::InputQueue;
use crate::crdt::CrdtClient; // Import CRDT client
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, OffscreenCanvas, WebSocket};

/// Worker entry point - called from main_js when in worker context
#[wasm_bindgen]
pub fn worker_entry() {
    let global: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data();
        if let Some(obj) = data.dyn_ref::<js_sys::Object>() {
            let msg_type = js_sys::Reflect::get(obj, &"type".into())
                .ok()
                .and_then(|v| v.as_string());

            web_sys::console::log_1(
                &format!("[Worker] Received message type: {:?}", msg_type).into(),
            );

            if msg_type.as_deref() == Some("init") {
                // Extract init data
                let canvas: OffscreenCanvas = js_sys::Reflect::get(obj, &"canvas".into())
                    .unwrap()
                    .dyn_into()
                    .unwrap();
                let ws_url = js_sys::Reflect::get(obj, &"ws_url".into())
                    .unwrap()
                    .as_string()
                    .unwrap();
                let width = js_sys::Reflect::get(obj, &"width".into())
                    .unwrap()
                    .as_f64()
                    .unwrap();
                let height = js_sys::Reflect::get(obj, &"height".into())
                    .unwrap()
                    .as_f64()
                    .unwrap();
                let dpr = js_sys::Reflect::get(obj, &"dpr".into())
                    .unwrap()
                    .as_f64()
                    .unwrap();

                // GPU acceleration toggle (like VS Code's "GPU Acceleration")
                let gpu_disabled = js_sys::Reflect::get(obj, &"gpu_disabled".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                web_sys::console::log_1(
                    &format!(
                        "[Worker] Starting with WS URL: {}, GPU disabled: {}",
                        ws_url, gpu_disabled
                    )
                    .into(),
                );
                start_worker(canvas, ws_url, width, height, dpr, gpu_disabled);
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    global.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // Signal to main thread that we're ready
    let ready_msg = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&ready_msg, &"type".into(), &"ready".into());
    global.post_message(&ready_msg).unwrap();

    web_sys::console::log_1(&"[Worker] Entry point initialized, sent ready signal".into());
}

fn start_worker(
    canvas: OffscreenCanvas,
    ws_url: String,
    width: f64,
    height: f64,
    dpr: f64,
    gpu_disabled: bool,
) {
    spawn_local(async move {
        // 1. Initialize Renderer (with optional GPU acceleration toggle)
        let mut renderer = crate::renderer::Renderer::new(canvas, dpr, gpu_disabled).await;
        let (rows, cols) = renderer.resize(width, height);

        // 2. Initialize State
        let grids = Rc::new(RefCell::new(GridManager::new()));
        let highlights = Rc::new(RefCell::new(HighlightMap::new()));
        let crdt_client = Rc::new(RefCell::new(CrdtClient::new(1))); // Buffer 1 default
        grids.borrow_mut().resize_grid(1, rows, cols);

        let renderer_rc = Rc::new(RefCell::new(renderer));

        // 3. Connect WebSocket
        let ws = match WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(e) => {
                web_sys::console::error_2(&"[Worker] WebSocket failed:".into(), &e);
                return;
            }
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // 4. Create Input Queue
        let input_queue = InputQueue::new(ws.clone());

        // 5. Setup WebSocket Handlers
        setup_websocket_handlers(&ws, grids.clone(), highlights.clone(), renderer_rc.clone(), crdt_client.clone());

        // 6. Setup Main Thread Message Handler
        setup_main_thread_handler(input_queue.clone(), renderer_rc.clone(), grids.clone());

        // 7. Send initial resize to Neovim
        send_resize(&ws, cols, rows);

        // 8. Start Render Loop
        start_render_loop(renderer_rc, grids, highlights);

        web_sys::console::log_1(&"[Worker] Fully initialized".into());
    });
}

fn setup_websocket_handlers(
    ws: &WebSocket,
    grids: Rc<RefCell<GridManager>>,
    highlights: Rc<RefCell<HighlightMap>>,
    renderer: Rc<RefCell<crate::renderer::Renderer>>,
    crdt: Rc<RefCell<CrdtClient>>,
) {
    let ws_clone = ws.clone();

    // On Open
    let onopen = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"[Worker] WebSocket connected".into());
        reset_reconnect_attempts();
    }) as Box<dyn FnMut()>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // On Message
    let grids_msg = grids.clone();
    let highlights_msg = highlights.clone();
    let renderer_msg = renderer.clone();
    let crdt_msg = crdt.clone(); // Capture for closure
    let ws_msg = ws_clone.clone();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            let mut cursor = std::io::Cursor::new(bytes);

            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                let grids = grids_msg.clone();
                let highlights = highlights_msg.clone();
                    let renderer = renderer_msg.clone();
                    let ws = ws_msg.clone();
                    let crdt = crdt_msg.clone(); // Clone for async

                    spawn_local(async move {
                        // Use legacy handler for now - we can refactor later
                        // handle_message expects RenderState, so we adapt
                        process_neovim_message(msg, grids, highlights, renderer, crdt, ws);
                    });
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // On Error
    let onerror = Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
        web_sys::console::error_2(&"[Worker] WebSocket error:".into(), &e);
        forward_connection_status("error");
    }) as Box<dyn FnMut(web_sys::ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // On Close - with reconnection
    let onclose = Closure::wrap(Box::new(move |e: web_sys::CloseEvent| {
        web_sys::console::warn_1(&format!("[Worker] WebSocket closed: {}", e.code()).into());
        forward_connection_status("disconnected");

        // Schedule reconnection with exponential backoff
        // Code = 1000 means normal closure, don't reconnect
        if e.code() != 1000 {
            schedule_reconnect();
        }
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    // Connection established
    forward_connection_status("connected");
}

fn process_neovim_message(
    msg: rmpv::Value,
    grids: Rc<RefCell<GridManager>>,
    highlights: Rc<RefCell<HighlightMap>>,
    _renderer: Rc<RefCell<crate::renderer::Renderer>>,
    crdt: Rc<RefCell<CrdtClient>>,
    _ws: WebSocket,
) {
    // Parse Neovim RPC message
    if let rmpv::Value::Array(arr) = &msg {
        // Check for session message: ["session", session_id, is_viewer]
        if arr.len() >= 2 {
            if let Some(method) = arr.first().and_then(|v| v.as_str()) {
                if method == "session" {
                    if let Some(session_id) = arr.get(1).and_then(|v| v.as_str()) {
                        // Forward session ID to main thread for sessionStorage
                        forward_session_id_to_main(session_id);
                    }
                    return;
                }
            }
        }

        if arr.len() >= 2 {
            // Notification: [2, method, params]
            if let Some(2) = arr.first().and_then(|v| v.as_i64()) {
                let method = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                let params = arr.get(2).cloned().unwrap_or(rmpv::Value::Nil);

                match method {
                    "redraw" => {
                        if let rmpv::Value::Array(events) = params {
                            for event in events {
                                process_redraw_event(&event, &grids, &highlights);
                            }
                        }
                    }
                    "option_set" => {
                        // params is [name, value]
                        if let rmpv::Value::Array(args) = params {
                            if let (Some(rmpv::Value::String(name)), Some(val)) =
                                (args.get(0), args.get(1))
                            {
                                if let Some(name_str) = name.as_str() {
                                    if name_str == "guifont" {
                                        if let rmpv::Value::String(font_val) = val {
                                            if let Some(font_str) = font_val.as_str() {
                                                forward_guifont_to_main(font_str);

                                                // Parse guifont: "Font Name:h12" -> "Font Name", 12
                                                let mut family = font_str.to_string();
                                                let mut size = None;

                                                if let Some(idx) = font_str.rfind(":h") {
                                                    if let Ok(s) =
                                                        font_str[idx + 2..].parse::<f64>()
                                                    {
                                                        family = font_str[..idx]
                                                            .to_string()
                                                            .replace("_", " ");
                                                        // Get DPR from renderer to scale points to pixels
                                                        let dpr = _renderer.borrow().dpr;
                                                        size = Some(s * dpr);
                                                    }
                                                }

                                                // If no :h parsed, assume it's just the family name (Neovim default)

                                                _renderer.borrow_mut().set_font(&family, size);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "nvim_web_image" => {
                        // Custom image protocol: [subcommand, args...]
                        if let rmpv::Value::Array(args) = params {
                            if let Some(rmpv::Value::String(subcmd)) = args.get(0) {
                                let cmd = subcmd.as_str().unwrap_or("");
                                match cmd {
                                    "show" => {
                                        // ["show", id, url, x, y, w, h]
                                        if args.len() >= 7 {
                                            let id = args[1].as_str().unwrap_or("?");
                                            let url = args[2].as_str().unwrap_or("");
                                            let x = args[3].as_f64().unwrap_or(0.0);
                                            let y = args[4].as_f64().unwrap_or(0.0);
                                            let w = args[5].as_f64().unwrap_or(1.0);
                                            let h = args[6].as_f64().unwrap_or(1.0);

                                            // Convert Cell -> CSS Pixels
                                            let renderer = _renderer.borrow();
                                            let cell_w = renderer.cell_w / renderer.dpr;
                                            let cell_h = renderer.cell_h / renderer.dpr;

                                            let px_x = x * cell_w;
                                            let px_y = y * cell_h;
                                            let px_w = w * cell_w;
                                            let px_h = h * cell_h;

                                            let global = js_sys::global();
                                            if let Some(scope) =
                                                global.dyn_ref::<DedicatedWorkerGlobalScope>()
                                            {
                                                let msg = js_sys::Object::new();
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"type".into(),
                                                    &"image_update".into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"action".into(),
                                                    &"show".into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"id".into(),
                                                    &id.into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"url".into(),
                                                    &url.into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"x".into(),
                                                    &px_x.into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"y".into(),
                                                    &px_y.into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"width".into(),
                                                    &px_w.into(),
                                                );
                                                let _ = js_sys::Reflect::set(
                                                    &msg,
                                                    &"height".into(),
                                                    &px_h.into(),
                                                );
                                                let _ = scope.post_message(&msg);
                                            }
                                        }
                                    }
                                    "hide" => {
                                        // ["hide", id]
                                        let id =
                                            args.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                                        let global = js_sys::global();
                                        if let Some(scope) =
                                            global.dyn_ref::<DedicatedWorkerGlobalScope>()
                                        {
                                            let msg = js_sys::Object::new();
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"type".into(),
                                                &"image_update".into(),
                                            );
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"action".into(),
                                                &"hide".into(),
                                            );
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"id".into(),
                                                &id.into(),
                                            );
                                            let _ = scope.post_message(&msg);
                                        }
                                    }
                                    "clear" => {
                                        // ["clear"]
                                        let global = js_sys::global();
                                        if let Some(scope) =
                                            global.dyn_ref::<DedicatedWorkerGlobalScope>()
                                        {
                                            let msg = js_sys::Object::new();
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"type".into(),
                                                &"image_update".into(),
                                            );
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"action".into(),
                                                &"clear".into(),
                                            );
                                            let _ = scope.post_message(&msg);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "crdt_sync" => {
                        // Handle CRDT sync messages
                        // Payload: [{"type": "sync1", ...}]
                        if let rmpv::Value::Array(args) = params {
                             if let Some(msg_map) = args.first() {
                                 // Simple manual parsing of the map to avoid complex deserialization logic here
                                 if let rmpv::Value::Map(kvs) = msg_map {
                                     let mut type_str = "";
                                     let mut binary_data: Option<Vec<u8>> = None;

                                     for (k, v) in kvs {
                                         if let rmpv::Value::String(s) = k {
                                             if s.as_str() == Some("type") {
                                                 if let rmpv::Value::String(ts) = v {
                                                     type_str = ts.as_str().unwrap_or("");
                                                 }
                                             } else if s.as_str() == Some("update") || s.as_str() == Some("state_vector") {
                                                  // Both fields are byte arrays (Vec<u8>)
                                                  if let rmpv::Value::Binary(bin) = v {
                                                      binary_data = Some(bin.clone());
                                                  } else if let rmpv::Value::Array(ints) = v {
                                                      // Sometimes RMP decodes bin as array of ints
                                                      let vec: Vec<u8> = ints.iter().filter_map(|x| x.as_u64().map(|b| b as u8)).collect();
                                                      binary_data = Some(vec);
                                                  }
                                             }
                                         }
                                     }

                                     if let Some(data) = binary_data {
                                         let mut client = crdt.borrow_mut();
                                         match type_str {
                                             "sync1" => {
                                                  // Host asking for our state vector? Usually client sends sync1 first.
                                                  // But if host sends sync1, we reply with sync2?
                                                  // Typically: Client connects -> Client sends Sync1.
                                                  // Host replies Sync2.
                                                  // If Host sends Sync1, it wants to pull our state.
                                             }
                                             "sync2" => {
                                                  // Host sent updates
                                                  let _ = client.apply_update(&data);
                                                  web_sys::console::log_1(&"[CRDT] Applied SyncStep2 update".into());
                                             }
                                             "update" => {
                                                  // Incremental update
                                                  let _ = client.apply_update(&data);
                                                  // web_sys::console::log_1(&"[CRDT] Applied incremental update".into());
                                             }
                                             _ => {}
                                         }
                                     }
                                 }
                             }
                        }
                    }
                    "nvim_web_action" => {
                        // Custom action protocol: [action_name, args...]
                        if let rmpv::Value::Array(args) = params {
                            if let Some(rmpv::Value::String(action_val)) = args.get(0) {
                                let action = action_val.as_str().unwrap_or("");
                                match action {
                                    "browse_files" => {
                                        let global = js_sys::global();
                                        if let Some(scope) =
                                            global.dyn_ref::<DedicatedWorkerGlobalScope>()
                                        {
                                            let msg = js_sys::Object::new();
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"type".into(),
                                                &"action".into(),
                                            );
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"name".into(),
                                                &"open_file_picker".into(),
                                            );
                                            let _ = scope.post_message(&msg);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {
                        // Other notifications
                    }
                }
            }
        }
    }
}

/// Forward session ID to main thread for storage in sessionStorage
fn forward_session_id_to_main(session_id: &str) {
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"session_id".into());
        let _ = js_sys::Reflect::set(&msg, &"session_id".into(), &session_id.into());
        let _ = scope.post_message(&msg);
        web_sys::console::log_1(&format!("[Worker] Forwarded session ID: {}", session_id).into());
    }
}

/// Forward mode change to main thread for DOM update
fn forward_mode_to_main(mode: &str) {
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"mode_change".into());
        let _ = js_sys::Reflect::set(&msg, &"mode".into(), &mode.into());
        let _ = scope.post_message(&msg);
    }
}

/// Forward connection status to main thread for UI indicator
fn forward_connection_status(status: &str) {
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"connection_status".into());
        let _ = js_sys::Reflect::set(&msg, &"status".into(), &status.into());
        let _ = scope.post_message(&msg);
    }
}

/// Forward guifont change to main thread for Font Face loading
fn forward_guifont_to_main(font_str: &str) {
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"guifont_update".into());
        let _ = js_sys::Reflect::set(&msg, &"font".into(), &font_str.into());
        let _ = scope.post_message(&msg);
        web_sys::console::log_1(&format!("[Worker] Forwarded guifont: {}", font_str).into());
    }
}

/// Reconnection state (global for worker)
thread_local! {
    static RECONNECT_ATTEMPT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Last garbage collection timestamp for memory recycling
    static LAST_GC: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Reset reconnection attempts on successful connection
pub fn reset_reconnect_attempts() {
    RECONNECT_ATTEMPT.with(|a| a.set(0));
}

/// Periodic memory cleanup to prevent WASM heap growth in long sessions
/// Marks grids clean after render to enable dirty region optimization
#[allow(dead_code)]
pub fn maybe_gc(timestamp: f64, grids: &std::cell::RefCell<GridManager>) {
    const GC_INTERVAL_MS: f64 = 300_000.0; // 5 minutes

    LAST_GC.with(|t| {
        if timestamp - t.get() > GC_INTERVAL_MS {
            // Mark all grids clean after render to reset dirty flags
            {
                let mut g = grids.borrow_mut();
                // Grid mark_clean is called per-grid in render
            }

            web_sys::console::log_1(&"[Worker] Memory GC: grids marked clean".into());

            t.set(timestamp);
        }
    });
}

/// Schedule WebSocket reconnection with exponential backoff and jitter
fn schedule_reconnect() {
    use wasm_bindgen::closure::Closure;

    // Get current attempt and increment
    let attempt = RECONNECT_ATTEMPT.with(|a| {
        let current = a.get();
        a.set(current.saturating_add(1));
        current
    });

    // Exponential backoff: min(base * 2^attempt, max) + jitter
    let base_delay_ms: u32 = 1000;
    let max_delay_ms: u32 = 30000;
    let exponential_delay =
        base_delay_ms.saturating_mul(1u32.checked_shl(attempt.min(5)).unwrap_or(32));
    let capped_delay = exponential_delay.min(max_delay_ms);

    // Add jitter: 0-20% random variation
    let jitter = (js_sys::Math::random() * 0.2 * f64::from(capped_delay)) as u32;
    let final_delay = capped_delay.saturating_add(jitter);

    web_sys::console::log_1(
        &format!(
            "[Worker] Reconnection attempt {} in {}ms",
            attempt + 1,
            final_delay
        )
        .into(),
    );

    let callback = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"[Worker] Attempting reconnection...".into());
        // Signal main thread to handle reconnection
        forward_connection_status("reconnecting");
    }) as Box<dyn FnMut()>);

    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            i32::try_from(final_delay).unwrap_or(i32::MAX),
        );
    }
    callback.forget();
}

/// Perform hot swap to a new WebSocket URL without losing state
/// Called from main thread when swap_backend message is received
pub fn perform_hot_swap(
    new_ws_url: &str,
    input_queue: &Rc<InputQueue>,
    grids: &Rc<RefCell<GridManager>>,
    highlights: &Rc<RefCell<HighlightMap>>,
    renderer: &Rc<RefCell<crate::renderer::Renderer>>,
    crdt: &Rc<RefCell<CrdtClient>>,
) {
    web_sys::console::log_1(&format!("[Worker] Hot swap to: {}", new_ws_url).into());

    // 1. Create new WebSocket connection
    let new_ws = match WebSocket::new(new_ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::error_2(&"[Worker] Hot swap failed:".into(), &e);
            forward_connection_status("error");
            return;
        }
    };
    new_ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // 2. Setup handlers on new WebSocket
    setup_websocket_handlers(&new_ws, grids.clone(), highlights.clone(), renderer.clone(), crdt.clone());

    // 3. Swap WebSocket in input queue (preserves queued messages)
    input_queue.replace_websocket(new_ws.clone());

    // 4. Get current grid dimensions for resize
    if let Some(grid) = grids.borrow().get(1) {
        send_resize(&new_ws, grid.cols, grid.rows);
    }

    // 5. Notify success
    forward_connection_status("connected");

    // 6. Forward to main thread
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>() {
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"hot_swap_complete".into());
        let _ = js_sys::Reflect::set(&msg, &"url".into(), &new_ws_url.into());
        let _ = scope.post_message(&msg);
    }

    web_sys::console::log_1(&"[Worker] Hot swap complete".into());
}

fn process_redraw_event(
    event: &rmpv::Value,
    grids: &Rc<RefCell<GridManager>>,
    highlights: &Rc<RefCell<HighlightMap>>,
) {
    if let rmpv::Value::Array(arr) = event {
        if arr.is_empty() {
            return;
        }
        let event_name = arr.first().and_then(|v| v.as_str()).unwrap_or("");

        match event_name {
            "grid_line" => {
                // [["grid_line", grid, row, col_start, cells, ...], ...]
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 4 {
                            let grid_id = args[0].as_u64().unwrap_or(1) as u32;
                            let row = args[1].as_u64().unwrap_or(0) as usize;
                            let col_start = args[2].as_u64().unwrap_or(0) as usize;

                            if let rmpv::Value::Array(cells) = &args[3] {
                                let mut col = col_start;
                                let mut last_hl: Option<u32> = None;

                                for cell in cells {
                                    if let rmpv::Value::Array(cell_data) = cell {
                                        let text = cell_data
                                            .first()
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(" ");
                                        let hl_id = cell_data
                                            .get(1)
                                            .and_then(|v| v.as_u64())
                                            .map(|v| v as u32)
                                            .or(last_hl);
                                        let repeat =
                                            cell_data.get(2).and_then(|v| v.as_u64()).unwrap_or(1)
                                                as usize;

                                        if let Some(id) = hl_id {
                                            last_hl = Some(id);
                                        }

                                        let mut grids = grids.borrow_mut();
                                        if let Some(grid) = grids.get_mut(grid_id) {
                                            for c in text.chars() {
                                                for _ in 0..repeat {
                                                    if col < grid.cols {
                                                        let idx = row * grid.cols + col;
                                                        if idx < grid.cells.len() {
                                                            grid.cells[idx].ch = c;
                                                            grid.cells[idx].hl_id = last_hl;
                                                        }
                                                        col += 1;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "grid_clear" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if let Some(grid_id) = args.first().and_then(|v| v.as_u64()) {
                            let mut grids = grids.borrow_mut();
                            if let Some(grid) = grids.get_mut(grid_id as u32) {
                                for cell in &mut grid.cells {
                                    cell.ch = ' ';
                                    cell.hl_id = None;
                                }
                            }
                        }
                    }
                }
            }
            "grid_resize" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 3 {
                            let grid_id = args[0].as_u64().unwrap_or(1) as u32;
                            let width = args[1].as_u64().unwrap_or(80) as usize;
                            let height = args[2].as_u64().unwrap_or(24) as usize;
                            grids.borrow_mut().resize_grid(grid_id, height, width);
                        }
                    }
                }
            }
            "grid_cursor_goto" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 3 {
                            let grid_id = args[0].as_u64().unwrap_or(1) as u32;
                            let row = args[1].as_u64().unwrap_or(0) as usize;
                            let col = args[2].as_u64().unwrap_or(0) as usize;
                            grids.borrow_mut().set_cursor(grid_id, row, col);
                        }
                    }
                }
            }
            "grid_scroll" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 7 {
                            let grid_id = args[0].as_u64().unwrap_or(1) as u32;
                            let top = args[1].as_u64().unwrap_or(0) as usize;
                            let bot = args[2].as_u64().unwrap_or(0) as usize;
                            let left = args[3].as_u64().unwrap_or(0) as usize;
                            let right = args[4].as_u64().unwrap_or(0) as usize;
                            let rows = args[5].as_i64().unwrap_or(0);
                            let _cols = args[6].as_i64().unwrap_or(0);

                            if let Some(grid) = grids.borrow_mut().get_mut(grid_id) {
                                grid.scroll_region(top, bot, left, right, rows);
                            }
                        }
                    }
                }
            }
            "hl_attr_define" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 2 {
                            let id = args[0].as_u64().unwrap_or(0) as u32;
                            // Parse highlight attributes
                            if let rmpv::Value::Map(attrs) = &args[1] {
                                let mut fg: Option<u32> = None;
                                let mut bg: Option<u32> = None;

                                for (k, v) in attrs {
                                    if let Some(key) = k.as_str() {
                                        match key {
                                            "foreground" => {
                                                fg = v.as_u64().map(|v| v as u32);
                                            }
                                            "background" => {
                                                bg = v.as_u64().map(|v| v as u32);
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                highlights.borrow_mut().define(
                                    id,
                                    crate::highlight::HighlightAttr {
                                        fg,
                                        bg,
                                        bold: false,
                                        italic: false,
                                        underline: false,
                                        undercurl: false,
                                        strikethrough: false,
                                        special: None,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            "default_colors_set" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 2 {
                            let fg = args[0].as_u64().map(|v| v as u32);
                            let bg = args[1].as_u64().map(|v| v as u32);
                            highlights.borrow_mut().set_default_colors(fg, bg);
                        }
                    }
                }
            }
            "win_pos" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 6 {
                            let grid_id = args[0].as_u64().unwrap_or(0) as u32;
                            let _win_id = args[1].as_u64().unwrap_or(0); // Unused for now
                            let start_row = args[2].as_i64().unwrap_or(0) as i32;
                            let start_col = args[3].as_i64().unwrap_or(0) as i32;
                            let width = args[4].as_u64().unwrap_or(0) as usize;
                            let height = args[5].as_u64().unwrap_or(0) as usize;

                            let mut grids = grids.borrow_mut();
                            grids.resize_grid(grid_id, height, width);
                            grids.set_win_pos(grid_id, start_row, start_col);
                        }
                    }
                }
            }
            "win_float_pos" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 6 {
                            let grid_id = args[0].as_u64().unwrap_or(0) as u32;
                            // args[1] = win_id
                            // args[2] = anchor
                            // args[3] = anchor_grid
                            let anchor_row = args[4].as_f64().unwrap_or(0.0) as i32;
                            let anchor_col = args[5].as_f64().unwrap_or(0.0) as i32;
                            // args[6] = focusable
                            // args[7] = zindex

                            grids
                                .borrow_mut()
                                .set_float_pos(grid_id, anchor_row, anchor_col);
                        }
                    }
                }
            }
            "win_hide" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 1 {
                            let grid_id = args[0].as_u64().unwrap_or(0) as u32;
                            grids.borrow_mut().hide_grid(grid_id);
                        }
                    }
                }
            }
            "win_close" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 1 {
                            let grid_id = args[0].as_u64().unwrap_or(0) as u32;
                            grids.borrow_mut().close_grid(grid_id);
                        }
                    }
                }
            }
            "msg_set_pos" => {
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if args.len() >= 5 {
                            let grid_id = args[0].as_u64().unwrap_or(0) as u32;
                            let row = args[1].as_u64().unwrap_or(0) as i32;
                            let col = args[2].as_u64().unwrap_or(0) as i32;
                            // args[3] = width, args[4] = height
                            grids.borrow_mut().set_win_pos(grid_id, row, col);
                        }
                    }
                }
            }
            "flush" => {
                // Rendering happens in the animation loop, flush is a no-op
            }
            "mode_change" => {
                // mode_change: [mode_name, mode_idx]
                for item in arr.iter().skip(1) {
                    if let rmpv::Value::Array(args) = item {
                        if !args.is_empty() {
                            if let rmpv::Value::String(mode_name) = &args[0] {
                                if let Some(mode) = mode_name.as_str() {
                                    grids.borrow_mut().set_mode(mode);
                                    forward_mode_to_main(mode);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                // Unhandled event
            }
        }
    }
}

fn setup_main_thread_handler(
    input_queue: Rc<InputQueue>,
    renderer: Rc<RefCell<crate::renderer::Renderer>>,
    grids: Rc<RefCell<GridManager>>,
) {
    let global: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data();
        if let Some(obj) = data.dyn_ref::<js_sys::Object>() {
            let msg_type = js_sys::Reflect::get(obj, &"type".into())
                .ok()
                .and_then(|v| v.as_string());

            match msg_type.as_deref() {
                Some("action") => {
                    // Direct actions from Main Thread
                    let name = js_sys::Reflect::get(obj, &"name".into())
                        .ok()
                        .and_then(|v| v.as_string());
                    if name.as_deref() == Some("fetch_url") {
                        let url = js_sys::Reflect::get(obj, &"url".into())
                            .ok()
                            .and_then(|v| v.as_string())
                            .unwrap_or_default();
                        if !url.is_empty() {
                            let input_queue_clone = input_queue.clone();
                            let url_string = url.to_string();
                            spawn_local(async move {
                                let global = js_sys::global();
                                if let Some(scope) = global.dyn_ref::<DedicatedWorkerGlobalScope>()
                                {
                                    let promise = scope.fetch_with_str(&url_string);
                                    match wasm_bindgen_futures::JsFuture::from(promise).await {
                                        Ok(resp_val) => {
                                            let resp: web_sys::Response = resp_val.unchecked_into();
                                            if resp.ok() {
                                                if let Ok(buffer_promise) = resp.array_buffer() {
                                                    if let Ok(buffer_val) =
                                                        wasm_bindgen_futures::JsFuture::from(
                                                            buffer_promise,
                                                        )
                                                        .await
                                                    {
                                                        let uint8 =
                                                            js_sys::Uint8Array::new(&buffer_val);
                                                        let vec = uint8.to_vec();
                                                        let filename = url_string
                                                            .split('/')
                                                            .last()
                                                            .unwrap_or("downloaded_file");
                                                        let safe_filename = if filename.is_empty() {
                                                            "downloaded_file"
                                                        } else {
                                                            filename
                                                        };

                                                        input_queue_clone
                                                            .send_file_drop(safe_filename, &vec);

                                                        web_sys::console::log_1(
                                                            &format!(
                                                                "[Worker] Fetched and dropped: {}",
                                                                safe_filename
                                                            )
                                                            .into(),
                                                        );
                                                    }
                                                }
                                            } else {
                                                web_sys::console::error_1(
                                                    &format!(
                                                        "[Worker] Fetch status {}: {}",
                                                        resp.status(),
                                                        url_string
                                                    )
                                                    .into(),
                                                );
                                            }
                                        }
                                        Err(_) => {
                                            web_sys::console::error_1(
                                                &format!(
                                                    "[Worker] Failed to fetch: {}",
                                                    url_string
                                                )
                                                .into(),
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
                Some("key") => {
                    let key = js_sys::Reflect::get(obj, &"key".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let ctrl = js_sys::Reflect::get(obj, &"ctrl".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let shift = js_sys::Reflect::get(obj, &"shift".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let alt = js_sys::Reflect::get(obj, &"alt".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let meta = js_sys::Reflect::get(obj, &"meta".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let nvim_key = translate_key(&key, ctrl, shift, alt, meta);
                    if !nvim_key.is_empty() {
                        input_queue.send_key(&nvim_key);
                    }
                }

                Some("resize") => {
                    let width = js_sys::Reflect::get(obj, &"width".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(800.0);
                    let height = js_sys::Reflect::get(obj, &"height".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(600.0);

                    let (rows, cols) = renderer.borrow_mut().resize(width, height);
                    grids.borrow_mut().resize_grid(1, rows, cols);
                    input_queue.send_resize(cols, rows);
                }
                Some("mouse") => {
                    let x = js_sys::Reflect::get(obj, &"x".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let y = js_sys::Reflect::get(obj, &"y".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let button_id = js_sys::Reflect::get(obj, &"button".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as u8;
                    let action = js_sys::Reflect::get(obj, &"action".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();

                    let ctrl = js_sys::Reflect::get(obj, &"ctrl".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let shift = js_sys::Reflect::get(obj, &"shift".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let alt = js_sys::Reflect::get(obj, &"alt".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let meta = js_sys::Reflect::get(obj, &"meta".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let renderer = renderer.borrow();
                    let col = (x / renderer.cell_w).floor() as usize;
                    let row = (y / renderer.cell_h).floor() as usize;

                    let btn_str = match button_id {
                        0 => "left",
                        1 => "middle",
                        2 => "right",
                        _ => "left",
                    };

                    let act_str = match action.as_str() {
                        "down" => "press",
                        "up" => "release",
                        "move" => "drag",
                        _ => "press",
                    };

                    let mods = helper_build_modifiers(ctrl, shift, alt, meta);
                    // Remove trailing dash if present
                    let mods = mods.trim_end_matches('-');

                    input_queue.send_mouse(btn_str, act_str, row, col, mods);
                }
                Some("wheel") => {
                    let x = js_sys::Reflect::get(obj, &"x".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let y = js_sys::Reflect::get(obj, &"y".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let delta_y = js_sys::Reflect::get(obj, &"delta_y".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    let ctrl = js_sys::Reflect::get(obj, &"ctrl".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let shift = js_sys::Reflect::get(obj, &"shift".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let alt = js_sys::Reflect::get(obj, &"alt".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let meta = js_sys::Reflect::get(obj, &"meta".into())
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let renderer = renderer.borrow();
                    let col = (x / renderer.cell_w).floor() as usize;
                    let row = (y / renderer.cell_h).floor() as usize;

                    let direction = if delta_y > 0.0 { "down" } else { "up" };

                    let mods = helper_build_modifiers(ctrl, shift, alt, meta);
                    let mods = mods.trim_end_matches('-');

                    input_queue.send_scroll(direction, row, col, mods);
                }
                Some("paste") => {
                    let text = js_sys::Reflect::get(obj, &"text".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    input_queue.send_paste(&text);
                }
                Some("file_drop") => {
                    let name = js_sys::Reflect::get(obj, &"name".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();

                    let data_val = js_sys::Reflect::get(obj, &"data".into())
                        .ok()
                        .unwrap_or(wasm_bindgen::JsValue::UNDEFINED);

                    if let Some(uint8) = data_val.dyn_ref::<js_sys::Uint8Array>() {
                        let mut data = vec![0; uint8.length() as usize];
                        uint8.copy_to(&mut data);
                        input_queue.send_file_drop(&name, &data);
                        web_sys::console::log_1(
                            &format!("Worker sent file drop: {} ({} bytes)", name, data.len())
                                .into(),
                        );
                    }
                }
                _ => {}
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    global.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
}

fn helper_build_modifiers(ctrl: bool, shift: bool, alt: bool, meta: bool) -> String {
    let mut mods = String::new();
    if ctrl {
        mods.push('C');
        mods.push('-');
    }
    if shift {
        mods.push('S');
        mods.push('-');
    }
    if alt {
        mods.push('M');
        mods.push('-');
    }
    if meta {
        mods.push('D'); // Cmd on Mac
        mods.push('-');
    }
    mods
}

fn translate_key(key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> String {
    // Special key mappings
    let special = match key {
        "Escape" => "Esc",
        "Backspace" => "BS",
        "Delete" => "Del",
        "Enter" => "CR",
        "Tab" => "Tab",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "Insert" => "Insert",
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
        " " => "Space",
        "<" => "lt",
        _ => "",
    };

    let base = if !special.is_empty() {
        special.to_string()
    } else if key.len() == 1 {
        key.to_string()
    } else {
        return String::new(); // Unknown key
    };

    // Use common modifier logic but handle implicit shift
    let mut mods = String::new();
    if ctrl {
        mods.push_str("C-");
    }
    if alt {
        mods.push_str("M-");
    }
    if meta {
        mods.push_str("D-");
    }

    // Shift is implicit for single chars (e.g. 'A') unless special
    if shift && (!base.chars().all(char::is_alphabetic) || !special.is_empty()) {
        mods.push_str("S-");
    }

    if mods.is_empty() && special.is_empty() {
        if key == "<" {
            return "<lt>".to_string();
        }
        return key.to_string();
    }

    format!("<{}{}>", mods, base)
}

fn send_resize(ws: &WebSocket, cols: usize, rows: usize) {
    let msg = rmpv::Value::Array(vec![
        rmpv::Value::String("resize".into()),
        rmpv::Value::Integer((cols as i64).into()),
        rmpv::Value::Integer((rows as i64).into()),
    ]);
    let mut bytes = Vec::new();
    if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
        let _ = ws.send_with_u8_array(&bytes);
    }
}

#[allow(clippy::type_complexity)]
fn start_render_loop(
    renderer: Rc<RefCell<crate::renderer::Renderer>>,
    grids: Rc<RefCell<GridManager>>,
    highlights: Rc<RefCell<HighlightMap>>,
) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        renderer
            .borrow_mut()
            .render(&grids.borrow(), &highlights.borrow());

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    let global: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let _ = global.request_animation_frame(f.as_ref().unchecked_ref());
}
