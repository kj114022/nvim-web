use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
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

use grid::GridManager;
use highlight::HighlightMap;
use renderer::Renderer;
use render::RenderState;

// DOM helpers (only used for initialization if needed, or remove if unused)
// None of the DOM helpers are used directly in start() anymore, so we don't import them.




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
    let _project_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let _project_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    if let Some(ref token) = open_token {
        _project_path.borrow_mut().replace(token.clone());
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

    // Phase 9.1.2: Input queue and listeners (keyboard, mouse, touch, ime, paste)
    let _input_queue = input::setup_input_listeners(
        &ws,
        &canvas,
        &editor_root,
        grids.clone(),
        renderer.clone(),
        render_state.clone(),
    )?;

    // Focus the wrapper on startup
    editor_root.focus()?;
    web_sys::console::log_1(&"EDITOR FOCUS INITIALIZED".into());

    Ok(())
}
