//! Input queue available for FIFO input handling
//! Includes backpressure, retry logic, and event listener setup

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, WebSocket, HtmlCanvasElement, HtmlElement, KeyboardEvent};

use crate::grid::GridManager;
use crate::renderer::Renderer;
use crate::render::RenderState;
use crate::dom::{set_dirty, focus_input};

const MAX_RETRIES: u8 = 5;

/// Connection state for resilience tracking
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum ConnectionState {
    Connected,
    Degraded,     // Experiencing failures but still trying
    Disconnected, // WebSocket closed
}

/// Input queue for FIFO keyboard/mouse input handling
pub struct InputQueue {
    queue: RefCell<VecDeque<(Vec<u8>, u8)>>, // (bytes, retry_count)
    ws: RefCell<Option<WebSocket>>,
    send_failures: Cell<u32>,
    state: Cell<ConnectionState>,
}

impl InputQueue {
    pub fn new(ws: WebSocket) -> Rc<Self> {
        Rc::new(Self {
            queue: RefCell::new(VecDeque::new()),
            ws: RefCell::new(Some(ws)),
            send_failures: Cell::new(0),
            state: Cell::new(ConnectionState::Connected),
        })
    }

    /// Get current connection state
    #[allow(dead_code)]
    pub const fn connection_state(&self) -> ConnectionState {
        self.state.get()
    }

    /// Update WebSocket on reconnection
    #[allow(dead_code)]
    pub fn set_websocket(&self, ws: WebSocket) {
        *self.ws.borrow_mut() = Some(ws);
        self.state.set(ConnectionState::Connected);
        self.send_failures.set(0);
        web_sys::console::log_1(&"InputQueue: WebSocket reconnected".into());
        self.flush();
    }

    /// Mark as disconnected (called from onclose handler)
    #[allow(dead_code)]
    pub fn mark_disconnected(&self) {
        self.state.set(ConnectionState::Disconnected);
        web_sys::console::warn_1(&"InputQueue: Disconnected, messages queued".into());
    }

    /// Enqueue an input event (already encoded as msgpack bytes)
    pub fn enqueue(&self, bytes: Vec<u8>) {
        self.queue.borrow_mut().push_back((bytes, 0));
        self.flush();
    }

    /// Calculate backoff delay for logging
    const fn backoff_delay_ms(retry_count: u8) -> u32 {
        100 * (1 << (if retry_count < 4 { retry_count } else { 4 }))
    }

    /// Flush all queued input to WebSocket
    pub fn flush(&self) {
        let ws_opt = self.ws.borrow();
        let ws = match ws_opt.as_ref() {
            Some(ws) if ws.ready_state() == WebSocket::OPEN => ws,
            _ => return,
        };

        let mut queue = self.queue.borrow_mut();
        let mut requeue: VecDeque<(Vec<u8>, u8)> = VecDeque::new();
        
        while let Some((bytes, retry_count)) = queue.pop_front() {
            match ws.send_with_u8_array(&bytes) {
                Ok(()) => {
                    if self.send_failures.get() > 0 {
                        self.send_failures.set(0);
                        self.state.set(ConnectionState::Connected);
                        web_sys::console::log_1(&"InputQueue: Send recovered".into());
                    }
                }
                Err(e) => {
                    self.send_failures.set(self.send_failures.get() + 1);
                    if self.send_failures.get() >= 3 {
                        self.state.set(ConnectionState::Degraded);
                    }
                    if retry_count < MAX_RETRIES {
                        requeue.push_back((bytes, retry_count + 1));
                        web_sys::console::warn_1(&format!(
                            "InputQueue: Send failed (retry {}/{}, backoff ~{}ms): {:?}", 
                            retry_count + 1, MAX_RETRIES, 
                            Self::backoff_delay_ms(retry_count), e
                        ).into());
                    } else {
                        web_sys::console::error_1(&format!(
                            "InputQueue: Dropping after {MAX_RETRIES} retries: {e:?}", 
                        ).into());
                    }
                }
            }
        }
        
        for item in requeue {
            queue.push_back(item);
        }
    }

    /// Send a key input (convenience method)
    pub fn send_key(&self, nvim_key: &str) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("input".into()),
            rmpv::Value::String(nvim_key.into()),
        ]);
        
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Get queue length
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.queue.borrow().len()
    }
}

/// Setup all input event listeners (keyboard, mouse, touch, paste, focus)
pub fn setup_input_listeners(
    ws: &WebSocket,
    canvas: &HtmlCanvasElement,
    editor_root: &HtmlElement,
    grids: &Rc<RefCell<GridManager>>,
    renderer: &Rc<Renderer>,
    render_state: &Rc<RenderState>,
) -> Result<Rc<InputQueue>, JsValue> {
    // Create InputQueue
    let input_queue = InputQueue::new(ws.clone());
    
    // 1. Expose to JS
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

    // 2. Keyboard (keydown on wrapper)
    setup_keyboard_listener(&input_queue, editor_root)?;
    
    // 3. IME / Input (on hidden textarea)
    setup_ime_listener(&input_queue);

    // 4. Mouse (mousedown on wrapper)
    setup_mouse_listener(&input_queue, editor_root, canvas, renderer.clone())?;
    
    // 5. Scroll (wheel on wrapper)
    setup_scroll_listener(&input_queue, editor_root)?;

    // 6. Focus/Blur
    setup_focus_blur(editor_root, grids, render_state)?;
    
    // 7. Paste
    setup_paste_listener(&input_queue, editor_root)?;
    
    // 8. Touch
    setup_touch_listener(&input_queue, editor_root, canvas, renderer.clone())?;

    Ok(input_queue)
}

fn setup_keyboard_listener(input_queue: &Rc<InputQueue>, editor_root: &HtmlElement) -> Result<(), JsValue> {
    let input_queue_key = input_queue.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let key = e.key();
        let ctrl = e.ctrl_key() || e.meta_key(); 
        let shift = e.shift_key();
        let alt = e.alt_key();
        
        let nvim_key: String = if ctrl || shift || alt {
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
            format!("<{mods}{base}>")
        } else {
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
        
        input_queue_key.send_key(&nvim_key);
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();
    Ok(())
}

fn setup_ime_listener(input_queue: &Rc<InputQueue>) {
    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(input_el) = document.get_element_by_id("nvim-input") {
             // Composition End
            let input_queue_compose = input_queue.clone();
            let compositionend = Closure::wrap(Box::new(move |e: web_sys::Event| {
                if let Ok(data) = js_sys::Reflect::get(&e, &"data".into()) {
                    if let Some(text) = data.as_string() {
                        if !text.is_empty() {
                            input_queue_compose.send_key(&text);
                            set_dirty(true);
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            let _ = input_el.add_event_listener_with_callback("compositionend", compositionend.as_ref().unchecked_ref());
            compositionend.forget();

            // Input
            let input_queue_input = input_queue.clone();
            let oninput = Closure::wrap(Box::new(move |e: web_sys::Event| {
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
                if let Some(target) = e.target() {
                    let _ = js_sys::Reflect::set(&target, &"value".into(), &"".into());
                }
            }) as Box<dyn FnMut(_)>);
             let _ = input_el.add_event_listener_with_callback("input", oninput.as_ref().unchecked_ref());
             oninput.forget();
        }
    }
}

fn setup_mouse_listener(
    input_queue: &Rc<InputQueue>, 
    editor_root: &HtmlElement, 
    canvas: &HtmlCanvasElement, 
    renderer: Rc<Renderer>
) -> Result<(), JsValue> {
    let input_queue_mouse = input_queue.clone();
    let canvas_mouse = canvas.clone();
    let editor_root_click = editor_root.clone();
    
    let onmousedown = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        let _ = editor_root_click.focus();
        focus_input();
        
        let canvas_element: &web_sys::Element = canvas_mouse.as_ref();
        let rect = canvas_element.get_bounding_client_rect();
        let x = e.client_x() as f64 - rect.left();
        let y = e.client_y() as f64 - rect.top();
        
        let (cell_w, cell_h) = renderer.cell_size();
        let col = (x / cell_w).floor() as i32;
        let row = (y / cell_h).floor() as i32;
        
        let mouse_input = format!("<LeftMouse><{col},{row}>");
        input_queue_mouse.send_key(&mouse_input);
    }) as Box<dyn FnMut(_)>);
    
    editor_root.add_event_listener_with_callback("mousedown", onmousedown.as_ref().unchecked_ref())?;
    onmousedown.forget();
    Ok(())
}

fn setup_scroll_listener(input_queue: &Rc<InputQueue>, editor_root: &HtmlElement) -> Result<(), JsValue> {
    let input_queue_scroll = input_queue.clone();
    let onwheel = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
        e.prevent_default();
        let delta_y = e.delta_y();
        let key = if delta_y > 0.0 { "<ScrollWheelDown>" } else if delta_y < 0.0 { "<ScrollWheelUp>" } else { return; };
        input_queue_scroll.send_key(key);
    }) as Box<dyn FnMut(_)>);
    editor_root.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();
    Ok(())
}

fn setup_focus_blur(
    editor_root: &HtmlElement,
    grids: &Rc<RefCell<GridManager>>,
    render_state: &Rc<RenderState>
) -> Result<(), JsValue> {
    let grids_focus = grids.clone();
    let render_state_focus = render_state.clone();
    let onfocus = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
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
        if let Some(grid) = grids_blur.borrow_mut().main_grid_mut() {
            grid.is_focused = false;
        }
        render_state_blur.render_now();
    }) as Box<dyn FnMut(_)>);
    editor_root.add_event_listener_with_callback("blur", onblur.as_ref().unchecked_ref())?;
    onblur.forget();
    Ok(())
}

fn setup_paste_listener(input_queue: &Rc<InputQueue>, editor_root: &HtmlElement) -> Result<(), JsValue> {
    let input_queue_paste = input_queue.clone();
    let onpaste = Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
        e.prevent_default();
        if let Some(data) = e.clipboard_data() {
            if let Ok(text) = data.get_data("text/plain") {
                if !text.is_empty() {
                    for c in text.chars() {
                         let key = match c {
                            '\n' => "<CR>".to_string(),
                            '\r' => continue,
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
    Ok(())
}

fn setup_touch_listener(
    input_queue: &Rc<InputQueue>,
    editor_root: &HtmlElement,
    canvas: &HtmlCanvasElement, 
    renderer: Rc<Renderer>
) -> Result<(), JsValue> {
    let input_queue_touch = input_queue.clone();
    let canvas_touch = canvas.clone();
    let editor_root_touch = editor_root.clone();
    
    let ontouchstart = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
        e.prevent_default();
        let _ = editor_root_touch.focus();
        focus_input();
        
        if let Some(touch) = e.touches().get(0) {
            let canvas_element: &web_sys::Element = canvas_touch.as_ref();
            let rect = canvas_element.get_bounding_client_rect();
            let x = touch.client_x() as f64 - rect.left();
            let y = touch.client_y() as f64 - rect.top();
            
            let (cell_w, cell_h) = renderer.cell_size();
            let col = (x / cell_w).floor() as i32;
            let row = (y / cell_h).floor() as i32;
            
            let mouse_input = format!("<LeftMouse><{col},{row}>");
            input_queue_touch.send_key(&mouse_input);
        }
    }) as Box<dyn FnMut(_)>);
    editor_root.add_event_listener_with_callback("touchstart", ontouchstart.as_ref().unchecked_ref())?;
    ontouchstart.forget();
    Ok(())
}
