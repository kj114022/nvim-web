use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, WebSocket, MessageEvent, KeyboardEvent};
use std::rc::Rc;
use std::cell::RefCell;

mod grid;
mod renderer;
mod input;

use grid::Grid;
use renderer::Renderer;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();
    let canvas = document
        .get_element_by_id("nvim")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    let grid = Rc::new(RefCell::new(Grid::new(24, 80)));
    let renderer = Renderer::new(canvas);

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
                                _ => {} // Ignore unsupported events for now
                            }
                        }
                    }
                }
            }
        }
    }
}
