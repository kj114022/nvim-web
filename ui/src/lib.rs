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
    renderer: Rc<Renderer>,
    needs_render: Rc<RefCell<bool>>,
    raf_scheduled: Rc<RefCell<bool>>,
}

impl RenderState {
    fn new(grid: Rc<RefCell<Grid>>, renderer: Rc<Renderer>) -> Rc<Self> {
        Rc::new(Self {
            grid,
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
            self.renderer.draw(&self.grid.borrow());
        }
    }

    /// Force immediate render (for resize, focus changes)
    fn render_now(&self) {
        *self.needs_render.borrow_mut() = false;
        self.renderer.draw(&self.grid.borrow());
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

    // Apply initial HiDPI scaling
    renderer.resize(css_width, css_height);

    // Create render state for batching
    let render_state = RenderState::new(grid.clone(), renderer.clone());

    // Initial render
    render_state.render_now();

    // Connect to WebSocket
    let ws = WebSocket::new("ws://127.0.0.1:9001")?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // Phase 9.2.1: Highlight storage
    let highlights = Rc::new(RefCell::new(HighlightMap::new()));

    // Handle incoming redraw events with batching
    let grid_msg = grid.clone();
    let render_state_msg = render_state.clone();
    let highlights_msg = highlights.clone();
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            
            // Decode msgpack redraw event
            let mut cursor = std::io::Cursor::new(bytes);
            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                // Apply redraw to grid model (fast, no canvas work)
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

    // Phase 9.1.2: Input queue for decoupled, FIFO input handling
    let input_queue = InputQueue::new(ws.clone());

    // Handle keyboard input - enqueue only, never render
    let input_queue_key = input_queue.clone();
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
        
        // Enqueue input (flushed immediately, FIFO order)
        // Never blocks on render, never waits for redraw
        input_queue_key.send_key(nvim_key);
        
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    
    document.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();

    // B1: Focus/blur detection for focus signaling
    // Make canvas focusable
    canvas.set_attribute("tabindex", "0")?;
    canvas.focus()?;

    let grid_focus = grid.clone();
    let render_state_focus = render_state.clone();
    let onfocus = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        grid_focus.borrow_mut().is_focused = true;
        render_state_focus.render_now();  // Immediate for focus feedback
    }) as Box<dyn FnMut(_)>);
    
    canvas.add_event_listener_with_callback("focus", onfocus.as_ref().unchecked_ref())?;
    onfocus.forget();

    let grid_blur = grid.clone();
    let render_state_blur = render_state.clone();
    let onblur = Closure::wrap(Box::new(move |_: web_sys::FocusEvent| {
        grid_blur.borrow_mut().is_focused = false;
        render_state_blur.render_now();  // Immediate for focus feedback
    }) as Box<dyn FnMut(_)>);
    
    canvas.add_event_listener_with_callback("blur", onblur.as_ref().unchecked_ref())?;
    onblur.forget();

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
                                    // hl_attr_define: ["hl_attr_define", id, rgb_attr, ...]
                                    // rgb_attr is a map with fg, bg, bold, italic, underline
                                    if ev.len() >= 3 {
                                        if let rmpv::Value::Integer(id) = &ev[1] {
                                            let hl_id = id.as_u64().unwrap_or(0) as u32;
                                            let attr = parse_hl_attr(&ev[2]);
                                            highlights.define(hl_id, attr);
                                        }
                                    }
                                }
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
                                                    
                                                    // Extract hl_id (second element, optional)
                                                    let hl_id = if cell_data.len() >= 2 {
                                                        if let rmpv::Value::Integer(h) = &cell_data[1] {
                                                            Some(h.as_u64().unwrap_or(0) as u32)
                                                        } else {
                                                            None
                                                        }
                                                    } else {
                                                        None
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

