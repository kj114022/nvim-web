use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, WebSocket, MessageEvent, KeyboardEvent, ResizeObserver, ResizeObserverEntry};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

mod grid;
mod highlight;
mod renderer;

use grid::Grid;
use highlight::{HighlightMap, HighlightAttr};
use renderer::Renderer;

/// Input queue for FIFO input handling - decoupled from rendering
struct InputQueue {
    queue: RefCell<VecDeque<Vec<u8>>>,
    ws: WebSocket,
}

impl InputQueue {
    fn new(ws: WebSocket) -> Rc<Self> {
        Rc::new(Self {
            queue: RefCell::new(VecDeque::new()),
            ws,
        })
    }

    /// Enqueue an input event (already encoded as msgpack bytes)
    fn enqueue(&self, bytes: Vec<u8>) {
        self.queue.borrow_mut().push_back(bytes);
        self.flush();
    }

    /// Flush all queued input to WebSocket immediately
    /// Never waits for render, preserves FIFO order
    fn flush(&self) {
        let mut queue = self.queue.borrow_mut();
        while let Some(bytes) = queue.pop_front() {
            let _ = self.ws.send_with_u8_array(&bytes);
        }
    }

    /// Send a key input (convenience method)
    fn send_key(&self, nvim_key: &str) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("input".into()),
            rmpv::Value::String(nvim_key.into()),
        ]);
        
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }
}

/// Render state for RAF-based batching
struct RenderState {
    grid: Rc<RefCell<Grid>>,
    highlights: Rc<RefCell<HighlightMap>>,
    renderer: Rc<Renderer>,
    needs_render: Rc<RefCell<bool>>,
    raf_scheduled: Rc<RefCell<bool>>,
}

impl RenderState {
    fn new(grid: Rc<RefCell<Grid>>, highlights: Rc<RefCell<HighlightMap>>, renderer: Rc<Renderer>) -> Rc<Self> {
        Rc::new(Self {
            grid,
            highlights,
            renderer,
            needs_render: Rc::new(RefCell::new(false)),
            raf_scheduled: Rc::new(RefCell::new(false)),
        })
    }

    /// Mark that a render is needed and schedule RAF if not already scheduled
    fn request_render(self: &Rc<Self>) {
        *self.needs_render.borrow_mut() = true;
        
        if !*self.raf_scheduled.borrow() {
            *self.raf_scheduled.borrow_mut() = true;
            
            let state = self.clone();
            let callback = Closure::once(Box::new(move || {
                state.do_render();
            }) as Box<dyn FnOnce()>);
            
            let _ = window().unwrap().request_animation_frame(
                callback.as_ref().unchecked_ref()
            );
            callback.forget();
        }
    }

    /// Execute the actual render (called from RAF)
    fn do_render(&self) {
        *self.raf_scheduled.borrow_mut() = false;
        
        if *self.needs_render.borrow() {
            *self.needs_render.borrow_mut() = false;
            self.renderer.draw(&self.grid.borrow(), &self.highlights.borrow());
        }
    }

    /// Force immediate render (for resize, focus changes)
    fn render_now(&self) {
        *self.needs_render.borrow_mut() = false;
        self.renderer.draw(&self.grid.borrow(), &self.highlights.borrow());
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
    
    let grid = Rc::new(RefCell::new(Grid::new(initial_rows.max(24), initial_cols.max(80))));
    let renderer = Rc::new(renderer);
    
    // Phase 9.2.1: Highlight storage (needed for RenderState)
    let highlights = Rc::new(RefCell::new(HighlightMap::new()));

    // Apply initial HiDPI scaling
    renderer.resize(css_width, css_height);

    // Create render state for batching
    let render_state = RenderState::new(grid.clone(), highlights.clone(), renderer.clone());

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
    
    // Determine session ID: URL param takes priority over localStorage
    let (ws_url, should_clear_url) = match url_session {
        Some(ref id) if id == "new" => {
            // Force new session - clear localStorage
            if let Some(ref s) = storage {
                let _ = s.remove_item("nvim_session_id");
            }
            web_sys::console::log_1(&"SESSION: Forcing new session (URL param)".into());
            ("ws://127.0.0.1:9001?session=new".to_string(), true)
        }
        Some(ref id) => {
            // Join specific session from URL
            web_sys::console::log_1(&format!("SESSION: Joining session {} (URL param)", id).into());
            (format!("ws://127.0.0.1:9001?session={}", id), true)
        }
        None => {
            // No URL param, check localStorage
            let existing_session = storage.as_ref()
                .and_then(|s| s.get_item("nvim_session_id").ok())
                .flatten();
            
            match existing_session {
                Some(ref id) => {
                    web_sys::console::log_1(&format!("SESSION: Reconnecting to session {}", id).into());
                    (format!("ws://127.0.0.1:9001?session={}", id), false)
                }
                None => {
                    web_sys::console::log_1(&"SESSION: Creating new session".into());
                    ("ws://127.0.0.1:9001?session=new".to_string(), false)
                }
            }
        }
    };
    
    // Clean URL after reading session param (removes ?session= from address bar)
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
    }) as Box<dyn FnMut(_)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // WS lifecycle: onerror
    let onerror = Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
        web_sys::console::error_1(&"WS ERROR".into());
        web_sys::console::error_1(&e);
    }) as Box<dyn FnMut(_)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // WS lifecycle: onclose - clear session on abnormal close
    let onclose = Closure::wrap(Box::new(move |e: web_sys::CloseEvent| {
        web_sys::console::warn_1(&"WS CLOSE".into());
        web_sys::console::warn_1(&format!("code={}, reason={}", e.code(), e.reason()).into());
        
        // If abnormal close (not 1000), clear session ID to force new session on reconnect
        if e.code() != 1000 && e.code() != 1001 {
            if let Some(s) = window().unwrap().local_storage().ok().flatten() {
                let _ = s.remove_item("nvim_session_id");
                web_sys::console::warn_1(&"SESSION: Cleared due to abnormal disconnect".into());
            }
        }
    }) as Box<dyn FnMut(_)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();


    // Handle incoming redraw events with batching
    let grid_msg = grid.clone();
    let render_state_msg = render_state.clone();
    let highlights_msg = highlights.clone();
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        web_sys::console::log_1(&"WS MESSAGE".into());
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            
            // Decode msgpack message
            let mut cursor = std::io::Cursor::new(bytes);
            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                // Check if this is a session message: ["session", "<id>"]
                if let rmpv::Value::Array(ref arr) = msg {
                    if arr.len() >= 2 {
                        if let rmpv::Value::String(ref method) = arr[0] {
                            if method.as_str() == Some("session") {
                                if let rmpv::Value::String(ref session_id) = arr[1] {
                                    if let Some(id) = session_id.as_str() {
                                        web_sys::console::log_1(&format!("SESSION: Received session ID: {}", id).into());
                                        // Store session ID in localStorage
                                        if let Ok(Some(storage)) = window().unwrap().local_storage() {
                                            let _ = storage.set_item("nvim_session_id", id);
                                            web_sys::console::log_1(&"SESSION: Stored in localStorage".into());
                                        }
                                        return; // Session message handled, don't process as redraw
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Not a session message, process as redraw
                apply_redraw(&mut grid_msg.borrow_mut(), &mut highlights_msg.borrow_mut(), &msg);
                
                // Schedule render via RAF (batched, at most once per frame)
                render_state_msg.request_render();
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // D1.1: ResizeObserver for window resize handling
    let grid_resize = grid.clone();
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
                grid_resize.borrow_mut().resize(new_rows, new_cols);

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

    // Phase 9.3: Mouse support - click to position cursor
    let input_queue_mouse = input_queue.clone();
    let renderer_mouse = renderer.clone();
    let canvas_mouse = canvas.clone();
    let editor_root_click = editor_root.clone();
    let onmousedown = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        // Focus the editor
        let _ = editor_root_click.focus();
        
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
    let grid_focus = grid.clone();
    let render_state_focus = render_state.clone();
    let onfocus = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        web_sys::console::log_1(&"FOCUS EVENT".into());
        grid_focus.borrow_mut().is_focused = true;
        render_state_focus.render_now();
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("focus", onfocus.as_ref().unchecked_ref())?;
    onfocus.forget();

    let grid_blur = grid.clone();
    let render_state_blur = render_state.clone();
    let onblur = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        web_sys::console::log_1(&"BLUR EVENT".into());
        grid_blur.borrow_mut().is_focused = false;
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
        
        // Focus editor
        let _ = editor_root_touch.focus();
        
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

// Apply redraw events to grid
fn apply_redraw(grid: &mut Grid, highlights: &mut HighlightMap, msg: &rmpv::Value) {
    if let rmpv::Value::Array(arr) = msg {
        // Message format: [2, "redraw", [[event, ...args]...]]
        if arr.len() >= 3 {
            if let rmpv::Value::Array(events) = &arr[2] {
                for event in events {
                    if let rmpv::Value::Array(ev) = event {
                        if ev.is_empty() { continue; }
                        
                        if let rmpv::Value::String(name) = &ev[0] {
                            match name.as_str() {
                                Some("hl_attr_define") => {
                                    // Batched: ["hl_attr_define", [id, rgb_attr, ...], [id, rgb_attr, ...], ...]
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 2 {
                                                if let rmpv::Value::Integer(id) = &args[0] {
                                                    let hl_id = id.as_u64().unwrap_or(0) as u32;
                                                    let attr = parse_hl_attr(&args[1]);
                                                    highlights.define(hl_id, attr);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_line") => {
                                    // Neovim batched event format:
                                    // ["grid_line", [grid, row, col, cells], [grid, row, col, cells], ...]
                                    // Each ev[1..] is a separate call with [grid, row, col_start, cells]
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 4 {
                                                if let (
                                                    rmpv::Value::Integer(_grid_id),
                                                    rmpv::Value::Integer(row),
                                                    rmpv::Value::Integer(col_start),
                                                    rmpv::Value::Array(cells)
                                                ) = (&args[0], &args[1], &args[2], &args[3]) {
                                                    let row = row.as_u64().unwrap_or(0) as usize;
                                                    let mut col = col_start.as_u64().unwrap_or(0) as usize;
                                                    let mut last_hl_id: Option<u32> = None;
                                                    
                                                    for cell in cells {
                                                        if let rmpv::Value::Array(cell_data) = cell {
                                                            if cell_data.is_empty() { continue; }
                                                            
                                                            // Extract text (first element)
                                                            let text = if let rmpv::Value::String(s) = &cell_data[0] {
                                                                s.as_str().unwrap_or("")
                                                            } else {
                                                                ""
                                                            };
                                                            
                                                            // Extract hl_id (second element, optional)
                                                            // If not present, use last_hl_id (sticky highlight)
                                                            let hl_id = if cell_data.len() >= 2 {
                                                                if let rmpv::Value::Integer(h) = &cell_data[1] {
                                                                    let id = Some(h.as_u64().unwrap_or(0) as u32);
                                                                    last_hl_id = id;
                                                                    id
                                                                } else {
                                                                    last_hl_id
                                                                }
                                                            } else {
                                                                last_hl_id
                                                            };
                                                            
                                                            // Extract repeat count (third element, defaults to 1)
                                                            let repeat = if cell_data.len() >= 3 {
                                                                if let rmpv::Value::Integer(r) = &cell_data[2] {
                                                                    r.as_u64().unwrap_or(1) as usize
                                                                } else {
                                                                    1
                                                                }
                                                            } else {
                                                                1
                                                            };
                                                            
                                                            // Handle empty string as space (Neovim convention)
                                                            let ch = if text.is_empty() {
                                                                ' '
                                                            } else {
                                                                text.chars().next().unwrap_or(' ')
                                                            };
                                                            
                                                            // Repeat character for repeat count (hl_id applies to all)
                                                            for _ in 0..repeat {
                                                                if col < grid.cols {
                                                                    grid.set_with_hl(row, col, ch, hl_id);
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
                                Some("grid_cursor_goto") => {
                                    // Batched: ["grid_cursor_goto", [grid, row, col], ...]
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 3 {
                                                if let (
                                                    rmpv::Value::Integer(_grid),
                                                    rmpv::Value::Integer(row),
                                                    rmpv::Value::Integer(col)
                                                ) = (&args[0], &args[1], &args[2]) {
                                                    grid.cursor_row = row.as_u64().unwrap_or(0) as usize;
                                                    grid.cursor_col = col.as_u64().unwrap_or(0) as usize;
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_clear") => {
                                    // Batched: ["grid_clear", [grid], ...]
                                    // For now, just clear on any call
                                    if ev.len() > 1 {
                                        for cell in &mut grid.cells {
                                            cell.ch = ' ';
                                            cell.hl_id = None;
                                        }
                                    }
                                }
                                Some("grid_resize") => {
                                    // Batched: ["grid_resize", [grid, width, height], ...]
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 3 {
                                                if let (
                                                    rmpv::Value::Integer(_grid),
                                                    rmpv::Value::Integer(width),
                                                    rmpv::Value::Integer(height)
                                                ) = (&args[0], &args[1], &args[2]) {
                                                    let new_cols = width.as_u64().unwrap_or(80) as usize;
                                                    let new_rows = height.as_u64().unwrap_or(24) as usize;
                                                    grid.resize(new_rows, new_cols);
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {} // Ignore unsupported events for now
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Parse highlight attributes from msgpack map
fn parse_hl_attr(value: &rmpv::Value) -> HighlightAttr {
    let mut attr = HighlightAttr::default();
    
    if let rmpv::Value::Map(map) = value {
        for (key, val) in map {
            if let rmpv::Value::String(k) = key {
                match k.as_str() {
                    Some("foreground") => {
                        if let rmpv::Value::Integer(i) = val {
                            attr.fg = Some(i.as_u64().unwrap_or(0) as u32);
                        }
                    }
                    Some("background") => {
                        if let rmpv::Value::Integer(i) = val {
                            attr.bg = Some(i.as_u64().unwrap_or(0) as u32);
                        }
                    }
                    Some("bold") => {
                        attr.bold = matches!(val, rmpv::Value::Boolean(true));
                    }
                    Some("italic") => {
                        attr.italic = matches!(val, rmpv::Value::Boolean(true));
                    }
                    Some("underline") => {
                        attr.underline = matches!(val, rmpv::Value::Boolean(true));
                    }
                    _ => {} // Ignore other attributes for now
                }
            }
        }
    }
    
    attr
}

