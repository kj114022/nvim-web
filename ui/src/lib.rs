use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, WebSocket, MessageEvent, KeyboardEvent, ResizeObserver, ResizeObserverEntry};
use std::rc::Rc;
use std::cell::RefCell;

mod grid;
mod renderer;

use grid::Grid;
use renderer::Renderer;

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

    // Apply initial HiDPI scaling
    renderer.resize(css_width, css_height);

    // Initial render
    renderer.draw(&grid.borrow());

    // Connect to WebSocket
    let ws = WebSocket::new("ws://127.0.0.1:9001")?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // Handle incoming redraw events
    let grid_clone = grid.clone();
    let renderer_clone = renderer.clone();
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            
            // Decode msgpack redraw event
            let mut cursor = std::io::Cursor::new(bytes);
            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                // Log raw msgpack structure
                web_sys::console::log_1(&format!("RAW REDRAW: {:?}", msg).into());
                
                // Apply redraw to grid
                apply_redraw(&mut grid_clone.borrow_mut(), &msg);
                // Redraw canvas
                renderer_clone.draw(&grid_clone.borrow());
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // D1.1: ResizeObserver for window resize handling
    let grid_resize = grid.clone();
    let renderer_resize = renderer.clone();
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

                // D1.3: Full redraw
                renderer_resize.draw(&grid_resize.borrow());
            }
        }
    }) as Box<dyn FnMut(_)>);

    let observer = ResizeObserver::new(resize_callback.as_ref().unchecked_ref())?;
    observer.observe(&canvas);
    resize_callback.forget();

    // Handle keyboard input
    let ws_clone = ws.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let key = e.key();
        
        // Map to Neovim key notation
        let nvim_key = match key.as_str() {
            "Enter" => "<CR>",
            "Escape" => "<Esc>",
            "Backspace" => "<BS>",
            "Tab" => "<Tab>",
            "ArrowUp" => "<Up>",
            "ArrowDown" => "<Down>",
            "ArrowLeft" => "<Left>",
            "ArrowRight" => "<Right>",
            _=> if key.len() == 1 { key.as_str() } else { return },
        };
        
        // Send input message as msgpack
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("input".into()),
            rmpv::Value::String(nvim_key.into()),
        ]);
        
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            let _ = ws_clone.send_with_u8_array(&bytes);
        }
        
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    
    document.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();

    // B1: Focus/blur detection for focus signaling
    // Make canvas focusable
    canvas.set_attribute("tabindex", "0")?;
    canvas.focus()?;

    let grid_focus = grid.clone();
    let renderer_focus = renderer.clone();
    let onfocus = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        grid_focus.borrow_mut().is_focused = true;
        renderer_focus.draw(&grid_focus.borrow());
    }) as Box<dyn FnMut(_)>);
    
    canvas.add_event_listener_with_callback("focus", onfocus.as_ref().unchecked_ref())?;
    onfocus.forget();

    let grid_blur = grid.clone();
    let renderer_blur = renderer.clone();
    let onblur = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        grid_blur.borrow_mut().is_focused = false;
        renderer_blur.draw(&grid_blur.borrow());
    }) as Box<dyn FnMut(_)>);
    
    canvas.add_event_listener_with_callback("blur", onblur.as_ref().unchecked_ref())?;
    onblur.forget();

    Ok(())
}

// Apply redraw events to grid - minimal implementation for Phase 4
fn apply_redraw(grid: &mut Grid, msg: &rmpv::Value) {
    if let rmpv::Value::Array(arr) = msg {
        // Message format: [2, "redraw", [[event, ...args]...]]
        if arr.len() >= 3 {
            if let rmpv::Value::Array(events) = &arr[2] {
                for event in events {
                    if let rmpv::Value::Array(ev) = event {
                        if ev.is_empty() { continue; }
                        
                        if let rmpv::Value::String(name) = &ev[0] {
                            match name.as_str() {
                                Some("grid_line") => {
                                    // grid_line: ["grid_line", grid, row, col, cells]
                                    // Each cell: [text, hl_id?, repeat?]
                                    // ev[0]=name, ev[1]=grid, ev[2]=row, ev[3]=col_start, ev[4]=cells
                                    if ev.len() >= 5 {
                                        if let (rmpv::Value::Integer(row), rmpv::Value::Integer(col_start), rmpv::Value::Array(cells)) 
                                            = (&ev[2], &ev[3], &ev[4]) {
                                            let row = row.as_u64().unwrap_or(0) as usize;
                                            let mut col = col_start.as_u64().unwrap_or(0) as usize;
                                            
                                            for cell in cells {
                                                if let rmpv::Value::Array(cell_data) = cell {
                                                    if cell_data.is_empty() { continue; }
                                                    
                                                    // Extract text (first element)
                                                    let text = if let rmpv::Value::String(s) = &cell_data[0] {
                                                        s.as_str().unwrap_or("")
                                                    } else {
                                                        ""
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
                                                    
                                                    // Repeat character for repeat count
                                                    for _ in 0..repeat {
                                                        if col < grid.cols {
                                                            grid.set(row, col, ch);
                                                            col += 1;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_cursor_goto") => {
                                    // grid_cursor_goto: ["grid_cursor_goto", grid, row, col]
                                    // ev[0]=name, ev[1]=grid, ev[2]=row, ev[3]=col
                                    if ev.len() >= 4 {
                                        if let (rmpv::Value::Integer(row), rmpv::Value::Integer(col)) = (&ev[2], &ev[3]) {
                                            grid.cursor_row = row.as_u64().unwrap_or(0) as usize;
                                            grid.cursor_col = col.as_u64().unwrap_or(0) as usize;
                                        }
                                    }
                                }
                                Some("grid_clear") => {
                                    // Clear grid
                                    for cell in &mut grid.cells {
                                        cell.ch = ' ';
                                    }
                                }
                                Some("grid_resize") => {
                                    // grid_resize: ["grid_resize", grid_id, width, height]
                                    if ev.len() >= 4 {
                                        if let (rmpv::Value::Integer(width), rmpv::Value::Integer(height)) = (&ev[2], &ev[3]) {
                                            let new_cols = width.as_u64().unwrap_or(80) as usize;
                                            let new_rows = height.as_u64().unwrap_or(24) as usize;
                                            grid.resize(new_rows, new_cols);
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
