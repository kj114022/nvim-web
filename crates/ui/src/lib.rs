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
mod dom;
mod opfs;
mod session;
mod handler;
mod network;

use grid::GridManager;
use highlight::HighlightMap;
use renderer::Renderer;
use input::InputQueue;
use render::RenderState;
use events::apply_redraw;

// DOM helpers
use dom::{set_status, show_toast, set_dirty, focus_input, update_drawer_session, update_drawer_cwd_info};
// OPFS
use opfs::js_handle_fs_request;

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

    // Initialize session (params, open token, etc)
    let session_config = session::init_session()?;
    let ws_url = session_config.ws_url;
    let open_token = session_config.open_token;
    
    // Store project path if we have an open token
    let project_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let project_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    if let Some(ref token) = open_token {
        project_path.borrow_mut().replace(token.clone());
    }

    // Connect to WebSocket with session support
    // Connect to WebSocket with session support
    let ws = network::setup_websocket(
        &ws_url,
        initial_cols as u32,
        initial_rows as u32,
        grids.clone(),
        render_state.clone(),
        highlights.clone(),
    )?;

    // Expose WS to window for debugging
    let _ = js_sys::Reflect::set(
        &window().unwrap(),
        &"__nvim_ws".into(),
        &ws.clone().into(),
    );
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
