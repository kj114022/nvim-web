#![allow(clippy::cast_possible_wrap)]
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Document, ResizeObserver, ResizeObserverEntry, HtmlCanvasElement, WebSocket};
use std::rc::Rc;
use std::cell::RefCell;
use crate::grid::GridManager;
use crate::renderer::Renderer;
use crate::render::RenderState;

/// Get document helper
fn get_document() -> Option<Document> {
    window().and_then(|w| w.document())
}

/// Set connection status indicator (connected/connecting/disconnected)
pub fn set_status(status: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-status") {
            el.set_class_name(&format!("status-{status}"));
        }
    }
}

/// Show a toast notification (auto-hides after 3 seconds)
pub fn show_toast(message: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-toast") {
            el.set_text_content(Some(message));
            let _ = el.set_attribute("class", "show");
            
            // Auto-hide after 3 seconds
            let callback = Closure::once(Box::new(move || {
                let _ = el.set_attribute("class", "");
            }) as Box<dyn FnOnce()>);
            
            if let Some(win) = window() {
                let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    3000,
                );
            }
            callback.forget();
        }
    }
}

/// Set dirty state indicator (unsaved changes) - uses status bar
pub fn set_dirty(dirty: bool) {
    if let Some(doc) = get_document() {
        // Update drawer-modified visibility in status bar (shows "*")
        if let Some(el) = doc.get_element_by_id("drawer-modified") {
            let display = if dirty { "inline" } else { "none" };
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html_el.style().set_property("display", display);
            }
        }
        // Update page title
        let base_title = "Neovim Web";
        let new_title = if dirty { format!("* {base_title}") } else { base_title.to_string() };
        doc.set_title(&new_title);
    }
}

/// Focus the hidden input textarea (for IME/mobile)
pub fn focus_input() {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-input") {
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html_el.focus();
            }
        }
    }
}

/// Update drawer status bar with session ID
pub fn update_drawer_session(session_id: &str, is_reconnection: bool) {
    if let Some(win) = window() {
        // Call window.__drawer.setSession(id, isReconnect)
        if let Ok(drawer) = js_sys::Reflect::get(&win, &"__drawer".into()) {
            if !drawer.is_undefined() {
                if let Ok(set_session) = js_sys::Reflect::get(&drawer, &"setSession".into()) {
                    if let Some(func) = set_session.dyn_ref::<js_sys::Function>() {
                        let _ = func.call2(&drawer, &session_id.into(), &is_reconnection.into());
                    }
                }
            }
        }
    }
}

/// Update drawer with CWD info (backend, cwd, git branch)
pub fn update_drawer_cwd_info(cwd: &str, file: &str, backend: &str, git_branch: Option<&str>) {
    if let Some(win) = window() {
        if let Ok(drawer) = js_sys::Reflect::get(&win, &"__drawer".into()) {
            if drawer.is_undefined() {
                return;
            }
            
            // Set CWD
            if let Ok(set_cwd) = js_sys::Reflect::get(&drawer, &"setCwd".into()) {
                if let Some(func) = set_cwd.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &cwd.into());
                }
            }
            
            // Set file
            if let Ok(set_file) = js_sys::Reflect::get(&drawer, &"setFile".into()) {
                if let Some(func) = set_file.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &file.into());
                }
            }
            
            // Set backend
            if let Ok(set_backend) = js_sys::Reflect::get(&drawer, &"setBackend".into()) {
                if let Some(func) = set_backend.dyn_ref::<js_sys::Function>() {
                    let _ = func.call1(&drawer, &backend.into());
                }
            }
            
            // Set git branch
            if let Ok(set_git) = js_sys::Reflect::get(&drawer, &"setGitBranch".into()) {
                if let Some(func) = set_git.dyn_ref::<js_sys::Function>() {
                    let branch_js: JsValue = match git_branch {
                        Some(b) => b.into(),
                        None => JsValue::NULL,
                    };
                    let _ = func.call1(&drawer, &branch_js);
                }
            }
        }
    }
}

/// Show macro recording indicator in status bar
pub fn show_macro_recording(register: &str) {
    if let Some(doc) = get_document() {
        // Show the macro indicator element
        if let Some(el) = doc.get_element_by_id("macro-indicator") {
            let _ = el.set_attribute("class", "drawer-item macro");
        }
        // Update the register display (the span with class macro-reg)
        if let Some(el) = doc.query_selector(".macro-reg").ok().flatten() {
            el.set_text_content(Some(&format!("@{register}")));
        }
    }
}

/// Hide macro recording indicator
pub fn hide_macro_recording() {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("macro-indicator") {
            let _ = el.set_attribute("class", "drawer-item macro hidden");
        }
    }
}

// NOTE: set_diagnostics_visible removed - handled by JS toggleDiagnostics() in index.html

/// Update the diagnostics overlay with current metrics
pub fn update_diagnostics(data: &crate::render::DiagnosticsData) {
    if let Some(doc) = get_document() {
        // Update FPS value
        if let Some(el) = doc.get_element_by_id("diag-fps") {
            el.set_text_content(Some(&format!("{:.1}", data.fps)));
        }
        // Update frame time
        if let Some(el) = doc.get_element_by_id("diag-frametime") {
            el.set_text_content(Some(&format!("{:.2}ms", data.frame_time_ms)));
        }
        // Update render count
        if let Some(el) = doc.get_element_by_id("diag-renders") {
            el.set_text_content(Some(&format!("{}", data.render_count)));
        }
        // Update dropped frames
        if let Some(el) = doc.get_element_by_id("diag-dropped") {
            el.set_text_content(Some(&format!("{}", data.dropped_frames)));
        }
    }
}


/// Setup `ResizeObserver` for the canvas
pub fn setup_resize_listener(
    canvas: &HtmlCanvasElement,
    grids: Rc<RefCell<GridManager>>,
    renderer: Rc<Renderer>,
    render_state: Rc<RenderState>,
    ws: &WebSocket,
) -> Result<(), JsValue> {
    let ws_resize = ws.clone();
    
    let resize_callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
        for i in 0..entries.length() {
            if let Ok(entry) = entries.get(i).dyn_into::<ResizeObserverEntry>() {
                let rect = entry.content_rect();
                let css_width = rect.width();
                let css_height = rect.height();

                // D1 + D2: Resize canvas with HiDPI handling
                let (new_rows, new_cols) = renderer.resize(css_width, css_height);

                // Update grid dimensions
                grids.borrow_mut().resize_grid(1, new_rows, new_cols);

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
                render_state.render_now();
            }
        }
    }) as Box<dyn FnMut(_)>);

    let observer = ResizeObserver::new(resize_callback.as_ref().unchecked_ref())?;
    observer.observe(canvas);
    resize_callback.forget();
    
    Ok(())
}
