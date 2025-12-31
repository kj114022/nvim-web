#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::future_not_send)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::ptr_arg)]
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{window, HtmlCanvasElement};
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
mod drawer;

use wasm_bindgen_futures::spawn_local;

use grid::GridManager;
use highlight::HighlightMap;
use renderer::Renderer;
use render::RenderState;

// DOM helpers (only used for initialization if needed, or remove if unused)
// None of the DOM helpers are used directly in start() anymore, so we don't import them.




#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    web_sys::console::log_1(&"[WASM] start() called".into());
    let document = window()
        .ok_or("No window found")?
        .document()
        .ok_or("No document found")?;
    let canvas = document
        .get_element_by_id("nvim")
        .ok_or("Canvas element #nvim not found")?
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
    
    // Highlight storage (needed for RenderState)
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
    web_sys::console::log_1(&"[WASM] Calling init_session()".into());
    let session_config = match session::init_session()? {
        Some(config) => {
            web_sys::console::log_1(&format!("[WASM] Session config: {}", config.ws_url).into());
            config
        }
        None => {
            // No session active - Show Dashboard in Drawer
            let drawer = document.get_element_by_id("nvim-drawer");
            // We use 'drawer-panels' as the container for all modular panels
            let panels = document.get_element_by_id("drawer-panels");
            
            if let (Some(drawer), Some(panels)) = (drawer, panels) {
                 // Expand drawer to full screen
                 let _ = drawer.class_list().remove_1("collapsed");
                 let _ = drawer.class_list().add_1("expanded-dashboard");
                 
                 // Show panels container
                 let _ = panels.class_list().remove_1("hidden");
                 
                 // Initialize Modular Drawer (renders sessions, binds tabs)
                 // Initialize Modular Drawer (renders sessions, binds tabs)
                 spawn_local(async {
                     if let Err(e) = drawer::init().await {
                         web_sys::console::error_2(&"[Drawer] Init Failed:".into(), &e);
                     } else {
                         web_sys::console::log_1(&"[Drawer] Init Success".into());
                     }
                 });
                 
                 // NOTE: Button binding removed - handled by failsafe JS script
                 // This prevents duplicate event listeners (race condition fix)
            }

            return Ok(());
        }
    };

    let ws_url = session_config.ws_url;
    let open_token = session_config.open_token;
    
    // Store project path if we have an open token
    let project_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let _project_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    if let Some(ref token) = open_token {
        project_path.borrow_mut().replace(token.clone());
    }

    // Connect to WebSocket with session support
    web_sys::console::log_1(&format!("[WASM] Connecting to WebSocket: {}", ws_url).into());
    let ws = network::setup_websocket(
        &ws_url,
        initial_cols as u32,
        initial_rows as u32,
        grids.clone(),
        render_state.clone(),
        highlights,
    )?;
    web_sys::console::log_1(&"[WASM] WebSocket setup complete".into());

    // Expose WS to window for debugging
    let _ = js_sys::Reflect::set(
        &window().unwrap(),
        &"__nvim_ws".into(),
        &ws.clone().into(),
    );
    // D1.1: ResizeObserver for window resize handling
    dom::setup_resize_listener(
        &canvas,
        grids.clone(),
        renderer.clone(),
        render_state.clone(),
        &ws,
    )?;

    // Get the focusable wrapper div
    let editor_root = document
        .get_element_by_id("editor-root")
        .unwrap()
        .dyn_into::<web_sys::HtmlElement>()?;

    // Input queue and listeners (keyboard, mouse, touch, ime, paste)
    let _input_queue = input::setup_input_listeners(
        &ws,
        &canvas,
        &editor_root,
        &grids,
        &renderer,
        &render_state,
    )?;

    // Focus the wrapper on startup
    editor_root.focus()?;
    web_sys::console::log_1(&"EDITOR FOCUS INITIALIZED".into());

    // Wire selection text extraction for drag-to-select copy
    let grids_for_selection = grids.clone();
    let selection_callback = Closure::wrap(Box::new(move |e: web_sys::CustomEvent| {
        if let Some(detail) = e.detail().dyn_ref::<js_sys::Object>() {
            let start_row = js_sys::Reflect::get(detail, &"startRow".into())
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
            let end_row = js_sys::Reflect::get(detail, &"endRow".into())
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
            let start_col = js_sys::Reflect::get(detail, &"startCol".into())
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
            let end_col = js_sys::Reflect::get(detail, &"endCol".into())
                .ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
            let result = js_sys::Reflect::get(detail, &"result".into()).ok();
            
            // Extract text from grid cells (using flat index)
            let grids = grids_for_selection.borrow();
            if let Some(grid) = grids.main_grid() {
                let mut text = String::new();
                for row in start_row..=end_row.min(grid.rows.saturating_sub(1)) {
                    let row_start = if row == start_row { start_col } else { 0 };
                    let row_end = if row == end_row { end_col } else { grid.cols.saturating_sub(1) };
                    
                    for col in row_start..=row_end.min(grid.cols.saturating_sub(1)) {
                        let idx = row * grid.cols + col;
                        if idx < grid.cells.len() {
                            text.push(grid.cells[idx].ch);
                        }
                    }
                    if row < end_row {
                        text.push('\n');
                    }
                }
                
                // Set result.text
                if let Some(result_obj) = result {
                    let _ = js_sys::Reflect::set(&result_obj, &"text".into(), &text.into());
                }
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    window()
        .unwrap()
        .add_event_listener_with_callback("get-selection-text", selection_callback.as_ref().unchecked_ref())?;
    selection_callback.forget();

    Ok(())
}
