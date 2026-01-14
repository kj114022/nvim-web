mod dom;
mod fs;
mod grid;
mod highlight;
mod input;
mod input_queue;
mod opfs;
mod render;
mod renderer;
mod worker;
pub mod crdt;

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    window, DedicatedWorkerGlobalScope, HtmlCanvasElement, KeyboardEvent, MouseEvent, WheelEvent,
    Worker,
};

use crate::dom::focus_input;

/// Detect if we are running in a Worker context
fn is_worker() -> bool {
    js_sys::global()
        .dyn_ref::<DedicatedWorkerGlobalScope>()
        .is_some()
}

/// Check if GPU acceleration is disabled by user
/// Similar to VS Code's "GPU Acceleration" toggle in settings
/// Can be set via:
/// - URL param: ?gpu=0 or ?canvas=1
/// - localStorage: nvim-web-gpu-disabled = "true"
fn is_gpu_disabled() -> bool {
    let window = match window() {
        Some(w) => w,
        None => return false,
    };

    // Check URL parameters (simple string matching)
    if let Ok(search) = window.location().search() {
        // ?gpu=0 or ?gpu=false disables GPU
        if search.contains("gpu=0") || search.contains("gpu=false") {
            return true;
        }
        // ?canvas=1 forces Canvas2D
        if search.contains("canvas=1") || search.contains("canvas=true") {
            return true;
        }
    }

    // Check localStorage preference
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(val)) = storage.get_item("nvim-web-gpu-disabled") {
            if val == "true" || val == "1" {
                return true;
            }
        }
    }

    false
}

#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    if is_worker() {
        // Worker Entry Point
        worker::worker_entry();
    } else {
        // Main Thread Entry Point
        main_thread_entry()?;
    }

    Ok(())
}

fn main_thread_entry() -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    // 1. Get Canvas
    let canvas = document
        .get_element_by_id("grid-canvas")
        .expect("canvas not found")
        .dyn_into::<HtmlCanvasElement>()?;

    // 2. Transfer to OffscreenCanvas
    let offscreen = canvas.transfer_control_to_offscreen()?;

    // 3. Create Worker (pointing to shim)
    let opts = web_sys::WorkerOptions::new();
    opts.set_type(web_sys::WorkerType::Module);
    let worker = Worker::new_with_options("./worker.js", &opts)?;

    // 4. Collect Init Parameters (need to capture these for the closure)
    let dpr = window.device_pixel_ratio();
    let rect = canvas.get_bounding_client_rect();
    let width = rect.width();
    let height = rect.height();
    let ws_url = get_ws_url()?;

    web_sys::console::log_1(&format!("[Main] WS URL: {}", ws_url).into());

    // 5. Setup message handler to wait for 'ready' signal
    let worker_clone = worker.clone();
    let _canvas_clone = canvas.clone();
    let _window_clone = window.clone();

    // Store offscreen in RefCell so closure can take ownership
    let offscreen_cell = Rc::new(RefCell::new(Some(offscreen)));
    let offscreen_for_closure = offscreen_cell.clone();

    let onmessage =
        wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            let data = e.data();
            if let Some(obj) = data.dyn_ref::<js_sys::Object>() {
                let msg_type = js_sys::Reflect::get(obj, &"type".into())
                    .ok()
                    .and_then(|v| v.as_string());

                match msg_type.as_deref() {
                    Some("ready") => {
                        web_sys::console::log_1(&"[Main] Worker ready, sending init".into());

                        // Take the offscreen canvas out of the cell
                        if let Some(offscreen) = offscreen_for_closure.borrow_mut().take() {
                            // Check GPU acceleration preference (like VS Code)
                            let gpu_disabled = is_gpu_disabled();

                            let init_msg = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&init_msg, &"type".into(), &"init".into());
                            let _ = js_sys::Reflect::set(&init_msg, &"canvas".into(), &offscreen);
                            let _ = js_sys::Reflect::set(
                                &init_msg,
                                &"ws_url".into(),
                                &ws_url.clone().into(),
                            );
                            let _ = js_sys::Reflect::set(&init_msg, &"width".into(), &width.into());
                            let _ =
                                js_sys::Reflect::set(&init_msg, &"height".into(), &height.into());
                            let _ = js_sys::Reflect::set(&init_msg, &"dpr".into(), &dpr.into());
                            let _ = js_sys::Reflect::set(
                                &init_msg,
                                &"gpu_disabled".into(),
                                &gpu_disabled.into(),
                            );

                            let transfer = js_sys::Array::new();
                            transfer.push(&offscreen);
                            let _ = worker_clone.post_message_with_transfer(&init_msg, &transfer);

                            web_sys::console::log_1(
                                &format!(
                                    "[Main] Init message sent (GPU disabled: {})",
                                    gpu_disabled
                                )
                                .into(),
                            );
                        }
                    }
                    Some("session_id") => {
                        // Store session ID for reconnection
                        if let Ok(session_id) = js_sys::Reflect::get(obj, &"session_id".into()) {
                            if let Some(id) = session_id.as_string() {
                                store_session_id(&id);
                            }
                        }
                    }
                    Some("mode_change") => {
                        // Update mode indicator in DOM
                        if let Ok(mode_val) = js_sys::Reflect::get(obj, &"mode".into()) {
                            if let Some(mode) = mode_val.as_string() {
                                crate::dom::update_mode_badge(&mode);
                                // Also update hints panel if visible
                                crate::dom::update_hints_panel(&mode);
                            }
                        }
                    }
                    Some("connection_status") => {
                        // Update connection status indicator
                        if let Ok(status_val) = js_sys::Reflect::get(obj, &"status".into()) {
                            if let Some(status) = status_val.as_string() {
                                crate::dom::update_connection_status(&status);
                            }
                        }
                    }
                    Some("cursor_goto") => {
                        // Update cursor position display
                        if let (Ok(line_val), Ok(col_val)) = (
                            js_sys::Reflect::get(obj, &"line".into()),
                            js_sys::Reflect::get(obj, &"col".into()),
                        ) {
                            let line = line_val.as_f64().unwrap_or(1.0) as i32;
                            let col = col_val.as_f64().unwrap_or(1.0) as i32;
                            crate::dom::update_cursor_pos(line, col);
                        }
                    }
                    Some("guifont_update") => {
                        let font_val = js_sys::Reflect::get(obj, &"font".into()).unwrap();
                        if let Some(font) = font_val.as_string() {
                            web_sys::console::log_1(
                                &format!("[Main] Received guifont update: {}", font).into(),
                            );
                        }
                    }
                    Some("image_update") => {
                        // Handle image show/hide/clear
                        if let Ok(action_val) = js_sys::Reflect::get(obj, &"action".into()) {
                            if let Some(action) = action_val.as_string() {
                                match action.as_str() {
                                    "show" => {
                                        let id = js_sys::Reflect::get(obj, &"id".into())
                                            .unwrap()
                                            .as_string()
                                            .unwrap_or_default();
                                        let url = js_sys::Reflect::get(obj, &"url".into())
                                            .unwrap()
                                            .as_string()
                                            .unwrap_or_default();
                                        let x = js_sys::Reflect::get(obj, &"x".into())
                                            .unwrap()
                                            .as_f64()
                                            .unwrap_or(0.0);
                                        let y = js_sys::Reflect::get(obj, &"y".into())
                                            .unwrap()
                                            .as_f64()
                                            .unwrap_or(0.0);
                                        let w = js_sys::Reflect::get(obj, &"width".into())
                                            .unwrap()
                                            .as_f64()
                                            .unwrap_or(0.0);
                                        let h = js_sys::Reflect::get(obj, &"height".into())
                                            .unwrap()
                                            .as_f64()
                                            .unwrap_or(0.0);
                                        crate::dom::update_image(&id, &url, x, y, w, h);
                                    }
                                    "hide" => {
                                        let id = js_sys::Reflect::get(obj, &"id".into())
                                            .unwrap()
                                            .as_string()
                                            .unwrap_or_default();
                                        crate::dom::remove_image(&id);
                                    }
                                    "clear" => {
                                        crate::dom::clear_images();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    Some("action") => {
                        // Handle generic actions
                        if let Ok(val) = js_sys::Reflect::get(obj, &"name".into()) {
                            if let Some(name) = val.as_string() {
                                if name == "open_file_picker" {
                                    crate::dom::open_file_picker();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // 6. Setup Event Forwarding
    let worker_rc = Rc::new(worker);
    setup_keyboard_forwarding(&window, &worker_rc)?;
    setup_mouse_forwarding(&canvas, &worker_rc)?;
    setup_wheel_forwarding(&canvas, &worker_rc)?;
    setup_resize_forwarding(&window, &canvas, &worker_rc)?;
    setup_paste_forwarding(&window, &worker_rc)?;
    setup_dragdrop_forwarding(&canvas, &worker_rc)?;
    setup_file_picker(&worker_rc)?;
    setup_start_screen(&worker_rc)?;

    web_sys::console::log_1(&"[Main] Worker spawned, waiting for ready signal".into());
    Ok(())
}

fn get_ws_url() -> Result<String, JsValue> {
    let window = web_sys::window().ok_or("no window")?;

    // Use Reflect to avoid web_sys::Location bindings which pollute Worker imports
    let location = js_sys::Reflect::get(&window, &"location".into())?;

    let hostname = js_sys::Reflect::get(&location, &"hostname".into())?
        .as_string()
        .unwrap_or_else(|| "localhost".to_string());

    let protocol = js_sys::Reflect::get(&location, &"protocol".into())?
        .as_string()
        .unwrap_or_else(|| "http:".to_string());

    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };

    // Get WS port from dynamic config (set by host in config.js)
    let config = js_sys::Reflect::get(&window, &"NVIM_CONFIG".into())?;
    let ws_port = if !config.is_undefined() {
        js_sys::Reflect::get(&config, &"wsPort".into())?
            .as_f64()
            .unwrap_or(9001.0) as u16
    } else {
        web_sys::console::warn_1(&"NVIM_CONFIG not found, defaulting wsPort to 9001".into());
        9001
    };

    // Check sessionStorage for existing session ID (for reconnection)
    let session_id = get_stored_session_id();

    let base_url = format!("{}//{}:{}", ws_protocol, hostname, ws_port);

    if let Some(id) = session_id {
        web_sys::console::log_1(&format!("[Main] Reconnecting to session: {}", id).into());
        Ok(format!("{}?session={}", base_url, id))
    } else {
        Ok(base_url)
    }
}

/// Get stored session ID from sessionStorage
fn get_stored_session_id() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.session_storage().ok()??;
    storage.get_item("nvim_web_session_id").ok()?
}

/// Store session ID in sessionStorage for reconnection
pub fn store_session_id(id: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.set_item("nvim_web_session_id", id);
            web_sys::console::log_1(&format!("[Main] Session ID stored: {}", id).into());
        }
    }
}

// ============================================================================
// Event Forwarding: Main Thread -> Worker
// ============================================================================

fn setup_keyboard_forwarding(window: &web_sys::Window, worker: &Rc<Worker>) -> Result<(), JsValue> {
    let worker = worker.clone();
    let callback = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        // Prevent default for certain keys to avoid browser behavior
        let key = e.key();
        if matches!(
            key.as_str(),
            "Tab"
                | "Escape"
                | "F1"
                | "F2"
                | "F3"
                | "F4"
                | "F5"
                | "F6"
                | "F7"
                | "F8"
                | "F9"
                | "F10"
                | "F11"
                | "F12"
        ) {
            e.prevent_default();
        }

        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"key".into());
        let _ = js_sys::Reflect::set(&msg, &"key".into(), &key.into());
        let _ = js_sys::Reflect::set(&msg, &"code".into(), &e.code().into());
        let _ = js_sys::Reflect::set(&msg, &"ctrl".into(), &e.ctrl_key().into());
        let _ = js_sys::Reflect::set(&msg, &"shift".into(), &e.shift_key().into());
        let _ = js_sys::Reflect::set(&msg, &"alt".into(), &e.alt_key().into());
        let _ = js_sys::Reflect::set(&msg, &"meta".into(), &e.meta_key().into());
        let _ = worker.post_message(&msg);
    }) as Box<dyn FnMut(KeyboardEvent)>);

    window.add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}

fn setup_mouse_forwarding(canvas: &HtmlCanvasElement, worker: &Rc<Worker>) -> Result<(), JsValue> {
    let worker = worker.clone();
    let canvas_clone = canvas.clone();

    // Mouse Move
    let worker_move = worker.clone();
    let canvas_move = canvas_clone.clone();
    let on_move = Closure::wrap(Box::new(move |e: MouseEvent| {
        let rect = canvas_move.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();

        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"mouse".into());
        let _ = js_sys::Reflect::set(&msg, &"action".into(), &"move".into());
        let _ = js_sys::Reflect::set(&msg, &"x".into(), &x.into());
        let _ = js_sys::Reflect::set(&msg, &"y".into(), &y.into());
        let _ = js_sys::Reflect::set(&msg, &"buttons".into(), &e.buttons().into());
        let _ = js_sys::Reflect::set(&msg, &"ctrl".into(), &e.ctrl_key().into());
        let _ = js_sys::Reflect::set(&msg, &"shift".into(), &e.shift_key().into());
        let _ = js_sys::Reflect::set(&msg, &"alt".into(), &e.alt_key().into());
        let _ = js_sys::Reflect::set(&msg, &"meta".into(), &e.meta_key().into());
        let _ = worker_move.post_message(&msg);
    }) as Box<dyn FnMut(MouseEvent)>);
    canvas.add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref())?;
    on_move.forget();

    // Mouse Down
    let worker_down = worker.clone();
    let canvas_down = canvas_clone.clone();
    let on_down = Closure::wrap(Box::new(move |e: MouseEvent| {
        e.prevent_default();
        let rect = canvas_down.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();

        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"mouse".into());
        let _ = js_sys::Reflect::set(&msg, &"action".into(), &"down".into());
        let _ = js_sys::Reflect::set(&msg, &"x".into(), &x.into());
        let _ = js_sys::Reflect::set(&msg, &"y".into(), &y.into());
        let _ = js_sys::Reflect::set(&msg, &"button".into(), &e.button().into());
        let _ = js_sys::Reflect::set(&msg, &"ctrl".into(), &e.ctrl_key().into());
        let _ = js_sys::Reflect::set(&msg, &"shift".into(), &e.shift_key().into());
        let _ = js_sys::Reflect::set(&msg, &"alt".into(), &e.alt_key().into());
        let _ = js_sys::Reflect::set(&msg, &"meta".into(), &e.meta_key().into());
        let _ = worker_down.post_message(&msg);
    }) as Box<dyn FnMut(MouseEvent)>);
    canvas.add_event_listener_with_callback("mousedown", on_down.as_ref().unchecked_ref())?;
    on_down.forget();

    // Mouse Up
    let worker_up = worker.clone();
    let canvas_up = canvas_clone.clone();
    let on_up = Closure::wrap(Box::new(move |e: MouseEvent| {
        let rect = canvas_up.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();

        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"mouse".into());
        let _ = js_sys::Reflect::set(&msg, &"action".into(), &"up".into());
        let _ = js_sys::Reflect::set(&msg, &"x".into(), &x.into());
        let _ = js_sys::Reflect::set(&msg, &"y".into(), &y.into());
        let _ = js_sys::Reflect::set(&msg, &"button".into(), &e.button().into());
        let _ = js_sys::Reflect::set(&msg, &"ctrl".into(), &e.ctrl_key().into());
        let _ = js_sys::Reflect::set(&msg, &"shift".into(), &e.shift_key().into());
        let _ = js_sys::Reflect::set(&msg, &"alt".into(), &e.alt_key().into());
        let _ = js_sys::Reflect::set(&msg, &"meta".into(), &e.meta_key().into());
        let _ = worker_up.post_message(&msg);
    }) as Box<dyn FnMut(MouseEvent)>);
    canvas.add_event_listener_with_callback("mouseup", on_up.as_ref().unchecked_ref())?;
    on_up.forget();

    Ok(())
}

fn setup_wheel_forwarding(canvas: &HtmlCanvasElement, worker: &Rc<Worker>) -> Result<(), JsValue> {
    let worker = worker.clone();
    let canvas_clone = canvas.clone();

    let on_wheel = Closure::wrap(Box::new(move |e: WheelEvent| {
        e.prevent_default();
        let rect = canvas_clone.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();

        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"wheel".into());
        let _ = js_sys::Reflect::set(&msg, &"x".into(), &x.into());
        let _ = js_sys::Reflect::set(&msg, &"y".into(), &y.into());
        let _ = js_sys::Reflect::set(&msg, &"delta_x".into(), &e.delta_x().into());
        let _ = js_sys::Reflect::set(&msg, &"delta_y".into(), &e.delta_y().into());
        let _ = js_sys::Reflect::set(&msg, &"delta_mode".into(), &e.delta_mode().into());
        let _ = js_sys::Reflect::set(&msg, &"ctrl".into(), &e.ctrl_key().into());
        let _ = js_sys::Reflect::set(&msg, &"shift".into(), &e.shift_key().into());
        let _ = js_sys::Reflect::set(&msg, &"alt".into(), &e.alt_key().into());
        let _ = js_sys::Reflect::set(&msg, &"meta".into(), &e.meta_key().into());
        let _ = worker.post_message(&msg);
    }) as Box<dyn FnMut(WheelEvent)>);

    canvas.add_event_listener_with_callback_and_bool(
        "wheel",
        on_wheel.as_ref().unchecked_ref(),
        false,
    )?;
    on_wheel.forget();
    Ok(())
}

fn setup_resize_forwarding(
    window: &web_sys::Window,
    canvas: &HtmlCanvasElement,
    worker: &Rc<Worker>,
) -> Result<(), JsValue> {
    let worker = worker.clone();
    let canvas = canvas.clone();

    let on_resize = Closure::wrap(Box::new(move || {
        let rect = canvas.get_bounding_client_rect();
        let msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"resize".into());
        let _ = js_sys::Reflect::set(&msg, &"width".into(), &rect.width().into());
        let _ = js_sys::Reflect::set(&msg, &"height".into(), &rect.height().into());
        let _ = worker.post_message(&msg);
    }) as Box<dyn FnMut()>);

    window.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref())?;
    on_resize.forget();
    Ok(())
}

fn setup_paste_forwarding(window: &web_sys::Window, worker: &Rc<Worker>) -> Result<(), JsValue> {
    let worker = worker.clone();

    let on_paste = Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
        if let Some(data) = e.clipboard_data() {
            if let Ok(text) = data.get_data("text/plain") {
                let msg = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&msg, &"type".into(), &"paste".into());
                let _ = js_sys::Reflect::set(&msg, &"text".into(), &text.into());
                let _ = worker.post_message(&msg);
            }
        }
    }) as Box<dyn FnMut(web_sys::ClipboardEvent)>);

    window.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref())?;
    on_paste.forget();
    Ok(())
}

fn setup_dragdrop_forwarding(
    canvas: &HtmlCanvasElement,
    worker: &Rc<Worker>,
) -> Result<(), JsValue> {
    let canvas_el: &web_sys::EventTarget = canvas.as_ref();

    // Prevent default dragover to allow drop
    let on_dragover = Closure::wrap(Box::new(move |e: web_sys::DragEvent| {
        e.prevent_default();
    }) as Box<dyn FnMut(web_sys::DragEvent)>);
    canvas_el.add_event_listener_with_callback("dragover", on_dragover.as_ref().unchecked_ref())?;
    on_dragover.forget();

    // Handle drop
    let worker = worker.clone();
    let on_drop = Closure::wrap(Box::new(move |e: web_sys::DragEvent| {
        e.prevent_default();
        if let Some(data_transfer) = e.data_transfer() {
            if let Some(files) = data_transfer.files() {
                for i in 0..files.length() {
                    if let Some(file) = files.get(i) {
                        let worker_clone = worker.clone();
                        let file_name = file.name();

                        // Read file content asynchronously
                        let reader = web_sys::FileReader::new().unwrap();
                        let reader_clone = reader.clone();

                        let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
                            if let Ok(result) = reader_clone.result() {
                                if let Some(array_buffer) = result.dyn_ref::<js_sys::ArrayBuffer>()
                                {
                                    let uint8 = js_sys::Uint8Array::new(array_buffer);

                                    // Check extension
                                    let lower_name = file_name.to_lowercase();
                                    if lower_name.ends_with(".ttf")
                                        || lower_name.ends_with(".otf")
                                        || lower_name.ends_with(".woff2")
                                    {
                                        // Handling Font File
                                        // Synthesize family name: "FiraCode-Regular.ttf" -> "FiraCode-Regular"
                                        let family = std::path::Path::new(&file_name)
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or(&file_name);

                                        web_sys::console::log_1(
                                            &format!("[Main] Loading font: {}", family).into(),
                                        );

                                        if let Err(e) =
                                            crate::dom::load_font_face(family, &uint8.to_vec())
                                        {
                                            web_sys::console::error_2(
                                                &"Failed to load font".into(),
                                                &e,
                                            );
                                        }

                                        // Also send to worker as file just in case user wants to save it?
                                        // Unnecessary for now.
                                        let msg = format!(
                                            "Font '{}' installed. Use :set guifont={} to activate.",
                                            family, family
                                        );
                                        crate::dom::show_toast(&msg);
                                    } else {
                                        // Send to worker (VFS write)
                                        let msg = js_sys::Object::new();
                                        let _ = js_sys::Reflect::set(
                                            &msg,
                                            &"type".into(),
                                            &"file_drop".into(),
                                        );
                                        let _ = js_sys::Reflect::set(
                                            &msg,
                                            &"name".into(),
                                            &file_name.clone().into(),
                                        );
                                        let _ = js_sys::Reflect::set(&msg, &"data".into(), &uint8);
                                        let _ = worker_clone.post_message(&msg);

                                        web_sys::console::log_1(
                                            &format!("[Main] Dropped file: {}", file_name).into(),
                                        );
                                    }
                                }
                            }
                        })
                            as Box<dyn FnMut(web_sys::Event)>);

                        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                        onload.forget();
                        let _ = reader.read_as_array_buffer(&file);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::DragEvent)>);
    canvas_el.add_event_listener_with_callback("drop", on_drop.as_ref().unchecked_ref())?;
    on_drop.forget();

    Ok(())
}

fn setup_file_picker(worker: &Rc<Worker>) -> Result<(), JsValue> {
    if let Some(doc) = window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("file-picker") {
            let el_clone = el.clone();
            let worker = worker.clone();

            let on_change = Closure::wrap(Box::new(move || {
                if let Ok(input) = el_clone
                    .dyn_ref::<web_sys::HtmlInputElement>()
                    .ok_or("Not input")
                {
                    if let Some(files) = input.files() {
                        if let Some(file) = files.get(0) {
                            let worker_clone = worker.clone();
                            let file_name = file.name();

                            // Read file content
                            let reader = web_sys::FileReader::new().unwrap();
                            let reader_clone = reader.clone();

                            let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
                                if let Ok(result) = reader_clone.result() {
                                    if let Some(array_buffer) =
                                        result.dyn_ref::<js_sys::ArrayBuffer>()
                                    {
                                        let uint8 = js_sys::Uint8Array::new(array_buffer);

                                        // Reuse logic: Font or File?
                                        let lower_name = file_name.to_lowercase();
                                        if lower_name.ends_with(".ttf")
                                            || lower_name.ends_with(".otf")
                                            || lower_name.ends_with(".woff2")
                                        {
                                            let family = std::path::Path::new(&file_name)
                                                .file_stem()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or(&file_name);

                                            if let Err(e) =
                                                crate::dom::load_font_face(family, &uint8.to_vec())
                                            {
                                                web_sys::console::error_2(
                                                    &"Failed to load font".into(),
                                                    &e,
                                                );
                                            }
                                            let msg = format!("Font '{}' installed.", family);
                                            crate::dom::show_toast(&msg);
                                        } else {
                                            // Send to VFS
                                            let msg = js_sys::Object::new();
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"type".into(),
                                                &"file_drop".into(),
                                            );
                                            let _ = js_sys::Reflect::set(
                                                &msg,
                                                &"name".into(),
                                                &file_name.clone().into(),
                                            );
                                            let _ =
                                                js_sys::Reflect::set(&msg, &"data".into(), &uint8);
                                            let _ = worker_clone.post_message(&msg);

                                            // Also save to OPFS for persistence
                                            let name_clone = file_name.clone();
                                            let data_vec = uint8.to_vec();
                                            spawn_local(async move {
                                                let _ = crate::fs::opfs::save_file(
                                                    &name_clone,
                                                    &data_vec,
                                                )
                                                .await;
                                            });
                                        }
                                    }
                                }
                            })
                                as Box<dyn FnMut(web_sys::Event)>);

                            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                            onload.forget();
                            let _ = reader.read_as_array_buffer(&file);
                        }
                    }
                }
            }) as Box<dyn FnMut()>);

            el.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())?;
            on_change.forget();
        }
    }
    Ok(())
}

fn setup_start_screen(worker: &Rc<Worker>) -> Result<(), JsValue> {
    let win = window().ok_or("No window")?;
    let doc = win.document().ok_or("No document")?;

    // New Session Button
    if let Some(btn) = doc.get_element_by_id("btn-new") {
        let on_click = Closure::wrap(Box::new(move || {
            crate::dom::show_start_screen(false);
            // Ensure focus returns to editor
            focus_input();
        }) as Box<dyn FnMut()>);
        btn.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // Resume Button
    if let Some(btn) = doc.get_element_by_id("btn-resume") {
        let on_click = Closure::wrap(Box::new(move || {
            crate::dom::show_start_screen(false);
            focus_input();
        }) as Box<dyn FnMut()>);
        btn.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // Browse Button
    if let Some(btn) = doc.get_element_by_id("btn-browse") {
        let on_click = Closure::wrap(Box::new(move || {
            crate::dom::open_file_picker();
        }) as Box<dyn FnMut()>);
        btn.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // Open URL Button
    if let Some(btn) = doc.get_element_by_id("btn-open-url") {
        let on_click = Closure::wrap(Box::new(move || {
            if let Ok(Some(url)) = window().unwrap().prompt_with_message("Enter URL to open:") {
                // Trigger logic (redirect? or fetch logic)
                // For now, reload with ?file=url
                if !url.is_empty() {
                    let _ = window()
                        .unwrap()
                        .location()
                        .set_href(&format!("/?file={}", url));
                }
            }
        }) as Box<dyn FnMut()>);
        btn.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // Keyboard Navigation for Start Screen
    {
        let doc_nav = doc.clone();
        let on_keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
            if let Some(screen) = doc_nav.get_element_by_id("start-screen") {
                let class_list = screen.class_list();
                if !class_list.contains("hidden") {
                    let key = e.key();
                    if key == "ArrowDown" || key == "ArrowRight" || key == "Tab" {
                        // managed by browser for Tab, but Arrows?
                        // Let's rely on browser Tab order for now, and just prevent "Enter" from doing nothing if focused?
                        // Actually, if we want Arrow Key nav, we need manual logic.
                        // Simple logic:
                        if key.starts_with("Arrow") {
                            // find active element?
                            // Just focus next button
                            // Skip for now, rely on Tab.
                        }
                    }
                    // If Escape, hide screen (New Session)
                    if key == "Escape" {
                        crate::dom::show_start_screen(false);
                        focus_input();
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        win.add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())?;
        on_keydown.forget();
    }

    // Recent Files List Delegation
    if let Some(list) = doc.get_element_by_id("recent-files-list") {
        let worker_clone = worker.clone();
        let on_click = Closure::wrap(Box::new(move |e: MouseEvent| {
            let target = e.target().unwrap();
            let el = target.dyn_into::<web_sys::HtmlElement>().unwrap();
            if let Some(li) = el.closest("li").unwrap_or(None) {
                if let Some(filename) = li.get_attribute("data-filename") {
                    // Load file from OPFS
                    let worker = worker_clone.clone();
                    let filename_clone = filename.clone();

                    spawn_local(async move {
                        if let Ok(data) = crate::fs::opfs::read_file(&filename_clone).await {
                            // Send to worker
                            let msg = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"file_drop".into());
                            let _ =
                                js_sys::Reflect::set(&msg, &"name".into(), &filename_clone.into());
                            let _ = js_sys::Reflect::set(
                                &msg,
                                &"content".into(),
                                &js_sys::Uint8Array::from(&data[..]),
                            );
                            let _ = worker.post_message(&msg);

                            // Hide screen
                            crate::dom::show_start_screen(false);
                        }
                    });
                }
            }
        }) as Box<dyn FnMut(MouseEvent)>);
        list.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // "Open URL" button
    if let Some(btn) = doc.get_element_by_id("btn-open-url") {
        let worker_clone = worker.clone();
        let on_click = Closure::wrap(Box::new(move || {
            if let Some(win) = web_sys::window() {
                if let Ok(Some(url)) = win.prompt_with_message("Enter URL to open:") {
                    if !url.is_empty() {
                        let msg = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"action".into());
                        let _ = js_sys::Reflect::set(&msg, &"name".into(), &"fetch_url".into());
                        let _ = js_sys::Reflect::set(&msg, &"url".into(), &url.into());
                        let _ = worker_clone.post_message(&msg);
                        crate::dom::show_start_screen(false);
                    }
                }
            }
        }) as Box<dyn FnMut()>);
        btn.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
        on_click.forget();
    }

    // Initial Load of Files via spawn_local
    let worker_clone = worker.clone();
    spawn_local(async move {
        // Show by default initially
        crate::dom::show_start_screen(true);

        // Check for ?file= query param
        if let Some(win) = web_sys::window() {
            if let Ok(location) = js_sys::Reflect::get(&win, &"location".into()) {
                if let Ok(search) = js_sys::Reflect::get(&location, &"search".into()) {
                    let search_str = search.as_string().unwrap_or_default();
                    if search_str.starts_with("?file=") {
                        let url = &search_str[6..]; // simplistic parsing
                        let decoded = js_sys::decode_uri_component(url)
                            .unwrap_or(url.into())
                            .as_string()
                            .unwrap_or(url.to_string());

                        web_sys::console::log_1(
                            &format!("[Main] Found file param: {}", decoded).into(),
                        );

                        // Send fetch_url action to worker
                        let msg = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&msg, &"type".into(), &"action".into());
                        let _ = js_sys::Reflect::set(&msg, &"name".into(), &"fetch_url".into());
                        let _ = js_sys::Reflect::set(&msg, &"url".into(), &decoded.into());

                        let _ = worker_clone.post_message(&msg);

                        // Hide start screen if loading persistent file?
                        crate::dom::show_start_screen(false);
                    }
                }
            }
        }

        if let Ok(files) = crate::fs::opfs::list_files().await {
            crate::dom::populate_recent_files(files);
        }
    });

    Ok(())
}
