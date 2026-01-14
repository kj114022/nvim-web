//! Input queue available for FIFO input handling
//! Includes backpressure, retry logic, and event listener setup
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, HtmlElement, KeyboardEvent, WebSocket};

use crate::dom::focus_input;
use crate::grid::GridManager;
use crate::render::RenderState;
use crate::renderer::Renderer;
// Import the shared InputQueue that is worker-safe
use crate::input_queue::InputQueue;

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

    // 4. Mouse (mousedown on wrapper) - with multigrid support
    setup_mouse_listener(
        &input_queue,
        editor_root,
        canvas,
        renderer.clone(),
        grids.clone(),
    )?;

    // 5. Scroll (wheel on wrapper)
    setup_scroll_listener(&input_queue, editor_root)?;

    // 6. Focus/Blur
    setup_focus_blur(editor_root, grids, render_state)?;

    // 7. Paste
    setup_paste_listener(&input_queue, editor_root)?;

    // 8. Touch (with multigrid support)
    setup_touch_listener(
        &input_queue,
        editor_root,
        canvas,
        renderer.clone(),
        grids.clone(),
    )?;

    Ok(input_queue)
}

fn setup_keyboard_listener(
    input_queue: &Rc<InputQueue>,
    editor_root: &HtmlElement,
) -> Result<(), JsValue> {
    let input_queue_key = input_queue.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        if e.is_composing() || e.key_code() == 229 {
            return;
        }

        let key = e.key();
        if key == "Dead" {
            return;
        }

        let ctrl = e.ctrl_key() || e.meta_key();
        let shift = e.shift_key();
        let alt = e.alt_key();
        let alt_graph = e.get_modifier_state("AltGraph");

        // If AltGraph is active, treat Alt as NOT active for modifier wrapping
        // This allows typing chars like @ (AltGr+0 on some layouts) without sending <A-@>
        let effective_alt = alt && !alt_graph;

        let nvim_key: String = if ctrl || shift || effective_alt {
            let mut mods = String::new();
            if ctrl {
                mods.push('C');
                mods.push('-');
            }
            if shift {
                mods.push('S');
                mods.push('-');
            }
            if effective_alt {
                mods.push('A');
                mods.push('-');
            }

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
                "Insert" => "Insert",
                "Pause" => "Pause",
                "PrintScreen" => "PrintScrn",
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

                // If it's a single character produced with modifiers (e.g. Ctrl-c)
                k if k.len() == 1 => k,
                _ => return, // Ignore unknown special keys
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
                "Insert" => "<Insert>".to_string(),
                "Pause" => "<Pause>".to_string(),
                "PrintScreen" => "<PrintScrn>".to_string(),
                "F1" => "<F1>".to_string(),
                "F2" => "<F2>".to_string(),
                "F3" => "<F3>".to_string(),
                "F4" => "<F4>".to_string(),
                "F5" => "<F5>".to_string(),
                "F6" => "<F6>".to_string(),
                "F7" => "<F7>".to_string(),
                "F8" => "<F8>".to_string(),
                "F9" => "<F9>".to_string(),
                "F10" => "<F10>".to_string(),
                "F11" => "<F11>".to_string(),
                "F12" => "<F12>".to_string(),

                // Normal typing
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
            // Composition Start - show composition indicator
            let compositionstart = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                // Show composition preview overlay
                if let Some(doc) = window().and_then(|w| w.document()) {
                    if let Some(preview) = doc.get_element_by_id("ime-preview") {
                        let _ = preview.class_list().add_1("visible");
                        preview.set_text_content(Some(""));
                    }
                }
            }) as Box<dyn FnMut(_)>);
            let _ = input_el.add_event_listener_with_callback(
                "compositionstart",
                compositionstart.as_ref().unchecked_ref(),
            );
            compositionstart.forget();

            // Composition Update - show composing text
            let compositionupdate = Closure::wrap(Box::new(move |e: web_sys::Event| {
                if let Ok(data) = js_sys::Reflect::get(&e, &"data".into()) {
                    if let Some(text) = data.as_string() {
                        if let Some(doc) = window().and_then(|w| w.document()) {
                            if let Some(preview) = doc.get_element_by_id("ime-preview") {
                                preview.set_text_content(Some(&text));
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            let _ = input_el.add_event_listener_with_callback(
                "compositionupdate",
                compositionupdate.as_ref().unchecked_ref(),
            );
            compositionupdate.forget();

            // Composition End - send final text and hide preview
            let input_queue_compose = input_queue.clone();
            let compositionend = Closure::wrap(Box::new(move |e: web_sys::Event| {
                // Hide composition preview
                if let Some(doc) = window().and_then(|w| w.document()) {
                    if let Some(preview) = doc.get_element_by_id("ime-preview") {
                        let _ = preview.class_list().remove_1("visible");
                        preview.set_text_content(Some(""));
                    }
                }

                if let Ok(data) = js_sys::Reflect::get(&e, &"data".into()) {
                    if let Some(text) = data.as_string() {
                        if !text.is_empty() {
                            input_queue_compose.send_key(&text);
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            let _ = input_el.add_event_listener_with_callback(
                "compositionend",
                compositionend.as_ref().unchecked_ref(),
            );
            compositionend.forget();

            // Input (for non-composing input)
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
                            }
                        }
                    }
                }
                if let Some(target) = e.target() {
                    let _ = js_sys::Reflect::set(&target, &"value".into(), &"".into());
                }
            }) as Box<dyn FnMut(_)>);
            let _ = input_el
                .add_event_listener_with_callback("input", oninput.as_ref().unchecked_ref());
            oninput.forget();
        }
    }
}

fn setup_mouse_listener(
    input_queue: &Rc<InputQueue>,
    editor_root: &HtmlElement,
    canvas: &HtmlCanvasElement,
    renderer: Rc<Renderer>,
    grids: Rc<RefCell<GridManager>>,
) -> Result<(), JsValue> {
    let editor_root_click = editor_root.clone();

    // State to track dragging
    let is_dragging = Rc::new(Cell::new(false));
    let is_dragging_move = is_dragging.clone();
    let is_dragging_up = is_dragging.clone();
    let is_dragging_leave = is_dragging.clone(); // Explicit clone for leave

    // Mousedown
    let input_queue_down = input_queue.clone();
    let canvas_down = canvas.clone();
    let renderer_down = renderer.clone();
    let grids_down = grids.clone();

    let onmousedown = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        let _ = editor_root_click.focus();
        focus_input();
        is_dragging.set(true);

        send_mouse_event(
            &e,
            &input_queue_down,
            &canvas_down,
            &renderer_down,
            &grids_down,
            "press",
        );
    }) as Box<dyn FnMut(_)>);

    editor_root
        .add_event_listener_with_callback("mousedown", onmousedown.as_ref().unchecked_ref())?;
    onmousedown.forget();

    // Mousemove (Drag)
    let input_queue_move = input_queue.clone();
    let canvas_move = canvas.clone();
    let renderer_move = renderer.clone();
    let grids_move = grids.clone();

    let onmousemove = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        if is_dragging_move.get() {
            // Prevent default selection behavior while dragging
            e.prevent_default();
            send_mouse_event(
                &e,
                &input_queue_move,
                &canvas_move,
                &renderer_move,
                &grids_move,
                "drag",
            );
        }
    }) as Box<dyn FnMut(_)>);

    editor_root
        .add_event_listener_with_callback("mousemove", onmousemove.as_ref().unchecked_ref())?;
    onmousemove.forget();

    // Mouseup (Release)
    let input_queue_up = input_queue.clone();
    let canvas_up = canvas.clone();
    let renderer_up = renderer.clone();
    let grids_up = grids.clone();

    let onmouseup = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        if is_dragging_up.get() {
            is_dragging_up.set(false);
            send_mouse_event(
                &e,
                &input_queue_up,
                &canvas_up,
                &renderer_up,
                &grids_up,
                "release",
            );
        }
    }) as Box<dyn FnMut(_)>);

    editor_root.add_event_listener_with_callback("mouseup", onmouseup.as_ref().unchecked_ref())?;
    onmouseup.forget();

    // Also handle mouseleave to release drag if cursor leaves window
    let input_queue_leave = input_queue.clone();
    let canvas_leave = canvas.clone();
    let renderer_leave = renderer.clone();
    let grids_leave = grids.clone();

    let onmouseleave = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        if is_dragging_leave.get() {
            is_dragging_leave.set(false);
            // Optional: Send release or just stop? Neovim prefers explicit release.
            send_mouse_event(
                &e,
                &input_queue_leave,
                &canvas_leave,
                &renderer_leave,
                &grids_leave,
                "release",
            );
        }
    }) as Box<dyn FnMut(_)>);

    editor_root
        .add_event_listener_with_callback("mouseleave", onmouseleave.as_ref().unchecked_ref())?;
    onmouseleave.forget();

    Ok(())
}

/// Helper to encode and send mouse RPC
fn send_mouse_event(
    e: &web_sys::MouseEvent,
    input_queue: &InputQueue,
    canvas: &HtmlCanvasElement,
    renderer: &Renderer,
    grids: &RefCell<GridManager>,
    action: &str,
) {
    let canvas_element: &web_sys::Element = canvas.as_ref();
    let rect = canvas_element.get_bounding_client_rect();
    let x = e.client_x() as f64 - rect.left();
    let y = e.client_y() as f64 - rect.top();

    let (cell_w, cell_h) = renderer.cell_size();
    let screen_col = (x / cell_w).floor() as i32;
    let screen_row = (y / cell_h).floor() as i32;

    // Find which grid contains the click and get local coordinates
    let (grid_id, local_row, local_col) =
        grids.borrow().find_grid_at_position(screen_row, screen_col);

    // Construct modifiers
    let mut modifier = String::new();
    if e.ctrl_key() {
        modifier.push('C');
    }
    if e.alt_key() {
        modifier.push('A');
    }
    if e.shift_key() {
        modifier.push('S');
    }

    // Message: ["input_mouse", button, action, modifier, grid, row, col]
    let msg = rmpv::Value::Array(vec![
        rmpv::Value::String("input_mouse".into()),
        rmpv::Value::String("left".into()), // Assuming left button for main interactions
        rmpv::Value::String(action.into()),
        rmpv::Value::String(modifier.into()),
        rmpv::Value::Integer(grid_id.into()),
        rmpv::Value::Integer(local_row.into()),
        rmpv::Value::Integer(local_col.into()),
    ]);

    let mut bytes = Vec::new();
    if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
        input_queue.enqueue(bytes);
    }
}

fn setup_scroll_listener(
    input_queue: &Rc<InputQueue>,
    editor_root: &HtmlElement,
) -> Result<(), JsValue> {
    let input_queue_scroll = input_queue.clone();
    let onwheel = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
        e.prevent_default();
        let delta_y = e.delta_y();
        let key = if delta_y > 0.0 {
            "<ScrollWheelDown>"
        } else if delta_y < 0.0 {
            "<ScrollWheelUp>"
        } else if e.delta_x() > 0.0 {
            "<ScrollWheelRight>"
        } else if e.delta_x() < 0.0 {
            "<ScrollWheelLeft>"
        } else {
            return;
        };
        input_queue_scroll.send_key(key);
    }) as Box<dyn FnMut(_)>);
    editor_root.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();
    Ok(())
}

fn setup_focus_blur(
    editor_root: &HtmlElement,
    grids: &Rc<RefCell<GridManager>>,
    render_state: &Rc<RenderState>,
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

fn setup_paste_listener(
    input_queue: &Rc<InputQueue>,
    editor_root: &HtmlElement,
) -> Result<(), JsValue> {
    let input_queue_paste = input_queue.clone();
    let onpaste = Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
        e.prevent_default();
        if let Some(data) = e.clipboard_data() {
            if let Ok(text) = data.get_data("text/plain") {
                if !text.is_empty() {
                    // Optimization: Batch input instead of sending char-by-char
                    // This significantly reduces WebSocket overhead for large pastes
                    let mut sanitized_input = String::with_capacity(text.len());
                    for c in text.chars() {
                        match c {
                            '\n' => sanitized_input.push_str("<CR>"),
                            '\r' => continue, // Skip CR, handle LF
                            '\t' => sanitized_input.push_str("<Tab>"),
                            '<' => sanitized_input.push_str("<lt>"),
                            '\\' => sanitized_input.push('\\'), // Escape backslash if needed
                            _ => sanitized_input.push(c),
                        }
                    }
                    if !sanitized_input.is_empty() {
                        input_queue_paste.send_key(&sanitized_input);
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
    renderer: Rc<Renderer>,
    grids: Rc<RefCell<GridManager>>,
) -> Result<(), JsValue> {
    // Shared state for touch gestures
    struct TouchState {
        start_x: f64,
        start_y: f64,
        last_x: f64,
        last_y: f64,
        start_time: f64,
        is_scrolling: bool,
        timer_id: Option<i32>,
        long_press_triggered: bool,
    }

    let state = Rc::new(RefCell::new(TouchState {
        start_x: 0.0,
        start_y: 0.0,
        last_x: 0.0,
        last_y: 0.0,
        start_time: 0.0,
        is_scrolling: false,
        timer_id: None,
        long_press_triggered: false,
    }));

    let input_queue_start = input_queue.clone();
    let canvas_start = canvas.clone();
    let renderer_start = renderer.clone();
    let grids_start = grids.clone();
    let state_start = state.clone();
    let editor_root_start = editor_root.clone();

    // prevent context menu to allow custom right click
    let oncontextmenu = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    editor_root
        .add_event_listener_with_callback("contextmenu", oncontextmenu.as_ref().unchecked_ref())?;
    oncontextmenu.forget();

    let ontouchstart = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
        // e.prevent_default(); // Only prevent if we handle it? Better to prevent always for game-like app
        // Actually, we want to allow pinch-zoom? No, fixed viewport.
        if e.touches().length() > 1 {
            return;
        } // Ignore multi-touch for now

        let _ = editor_root_start.focus();
        focus_input();

        if let Some(touch) = e.touches().get(0) {
            let canvas_element: &web_sys::Element = canvas_start.as_ref();
            let rect = canvas_element.get_bounding_client_rect();
            let x = touch.client_x() as f64 - rect.left();
            let y = touch.client_y() as f64 - rect.top();

            let mut s = state_start.borrow_mut();
            s.start_x = x;
            s.start_y = y;
            s.last_x = x;
            s.last_y = y;
            s.start_time = js_sys::Date::now();
            s.is_scrolling = false;
            s.long_press_triggered = false;

            // Start Long Press Timer (600ms)
            let input_queue_timer = input_queue_start.clone();
            let canvas_timer = canvas_start.clone();
            let renderer_timer = renderer_start.clone();
            let grids_timer = grids_start.clone();
            let state_timer = state_start.clone();
            let touch_copy = touch.clone(); // Keep touch info? No, need coords.
                                            // Closure for timer
            let callback = Closure::wrap(Box::new(move || {
                let mut s = state_timer.borrow_mut();
                if !s.is_scrolling {
                    s.long_press_triggered = true;
                    // Trigger Right Click
                    // Note: We use the start coordinates
                    // Construct a fake MouseEvent? Or just call helper?
                    // Helper needs MouseEvent, which we don't have here.
                    // We duplicate logic or refactor helper. duplication is easier now.

                    let (cell_w, cell_h) = renderer_timer.cell_size();
                    let screen_col = (s.start_x / cell_w).floor() as i32;
                    let screen_row = (s.start_y / cell_h).floor() as i32;

                    let (grid_id, local_row, local_col) = grids_timer
                        .borrow()
                        .find_grid_at_position(screen_row, screen_col);

                    send_mouse_rpc(
                        &input_queue_timer,
                        "right",
                        "press",
                        "",
                        grid_id.into(),
                        local_row,
                        local_col,
                    );
                    send_mouse_rpc(
                        &input_queue_timer,
                        "right",
                        "release",
                        "",
                        grid_id.into(),
                        local_row,
                        local_col,
                    ); // Click

                    // Vibration feedback if available
                    if !js_sys::Reflect::get(&window().unwrap(), &"navigator".into())
                        .and_then(|n| js_sys::Reflect::get(&n, &"vibrate".into()))
                        .is_err()
                    {
                        let _ = window().unwrap().navigator().vibrate_with_duration(50);
                    }
                }
            }) as Box<dyn FnMut()>);

            let id = window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    600,
                )
                .unwrap_or(0);
            callback.forget();
            s.timer_id = Some(id);
        }
    }) as Box<dyn FnMut(_)>);
    editor_root
        .add_event_listener_with_callback("touchstart", ontouchstart.as_ref().unchecked_ref())?;
    ontouchstart.forget();

    // Touch Move (Scroll)
    let state_move = state.clone();
    let renderer_move = renderer.clone();
    let input_queue_move = input_queue.clone();

    let ontouchmove = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
        e.prevent_default(); // Prevent native scrolling

        let mut s = state_move.borrow_mut();

        if let Some(touch) = e.touches().get(0) {
            let x = touch.client_x() as f64; // Relative to viewport for delta is fine
            let y = touch.client_y() as f64;

            let dx = x - s.last_x; // Note: Dragging UP (negative dy) means scrolling DOWN
                                   // Actually: Dragging finger UP means we want to see content below, so Scroll Down.
                                   // Wheel: deltaY > 0 is Down.
                                   // If we drag finger UP (dy < 0), we want Scroll Down (delta > 0).
                                   // So scroll delta = -dy. (approx)

            let dy = y - s.last_y;

            // Check threshold for scrolling/canceling long press
            if !s.is_scrolling {
                if dx.abs() > 5.0 || dy.abs() > 5.0 {
                    s.is_scrolling = true;
                    // Cancel timer
                    if let Some(id) = s.timer_id {
                        window().unwrap().clear_timeout_with_handle(id);
                        s.timer_id = None;
                    }
                }
            }

            if s.is_scrolling {
                let (cell_w, cell_h) = renderer_move.cell_size();

                // Accumulate? For now just simple thresholding per move
                // Sensitivity factor
                if dy.abs() > cell_h * 0.5 {
                    let key = if dy < 0.0 {
                        "<ScrollWheelDown>"
                    } else {
                        "<ScrollWheelUp>"
                    };
                    input_queue_move.send_key(key);
                    s.last_y = y; // Reset last reference
                }
                if dx.abs() > cell_w * 0.5 {
                    let key = if dx < 0.0 {
                        "<ScrollWheelRight>"
                    } else {
                        "<ScrollWheelLeft>"
                    };
                    input_queue_move.send_key(key);
                    s.last_x = x;
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    editor_root
        .add_event_listener_with_callback("touchmove", ontouchmove.as_ref().unchecked_ref())?;
    ontouchmove.forget();

    // Touch End (Click)
    let state_end = state.clone();
    let input_queue_end = input_queue.clone();
    let renderer_end = renderer.clone();
    let grids_end = grids.clone();

    let ontouchend = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
        // e.prevent_default(); // Prevent mouse emulation?

        let mut s = state_end.borrow_mut();
        // Cancel timer
        if let Some(id) = s.timer_id {
            window().unwrap().clear_timeout_with_handle(id);
            s.timer_id = None;
        }

        if !s.is_scrolling && !s.long_press_triggered {
            // It was a tap! -> Left Click
            // Need coordinates. changedTouches?
            if let Some(touch) = e.changed_touches().get(0) {
                // ... calc grid ...
                // Duplicate logic again, ideally shared
                // let rect = canvas.get_bounding_client_rect(); // Need canvas ref?
                // We don't have canvas in scope of this closure unless captured.
                // Assuming tap is at start pos (roughly).
                // Or use changed touches.
                // Let's take start pos for stability.

                let (cell_w, cell_h) = renderer_end.cell_size();
                let screen_col = (s.start_x / cell_w).floor() as i32;
                let screen_row = (s.start_y / cell_h).floor() as i32;

                let (grid_id, local_row, local_col) = grids_end
                    .borrow()
                    .find_grid_at_position(screen_row, screen_col);

                send_mouse_rpc(
                    &input_queue_end,
                    "left",
                    "press",
                    "",
                    grid_id.into(),
                    local_row,
                    local_col,
                );
                send_mouse_rpc(
                    &input_queue_end,
                    "left",
                    "release",
                    "",
                    grid_id.into(),
                    local_row,
                    local_col,
                );
            }
        }
    }) as Box<dyn FnMut(_)>);
    editor_root
        .add_event_listener_with_callback("touchend", ontouchend.as_ref().unchecked_ref())?;
    ontouchend.forget();

    Ok(())
}

fn send_mouse_rpc(
    input_queue: &InputQueue,
    button: &str,
    action: &str,
    modifier: &str,
    grid_id: u64,
    row: i32,
    col: i32,
) {
    let msg = rmpv::Value::Array(vec![
        rmpv::Value::String("input_mouse".into()),
        rmpv::Value::String(button.into()),
        rmpv::Value::String(action.into()),
        rmpv::Value::String(modifier.into()),
        rmpv::Value::Integer(grid_id.into()),
        rmpv::Value::Integer(row.into()),
        rmpv::Value::Integer(col.into()),
    ]);
    let mut bytes = Vec::new();
    if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
        input_queue.enqueue(bytes);
    }
}
