use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Document};

/// Get document helper
fn get_document() -> Option<Document> {
    window().and_then(|w| w.document())
}

/// Set connection status indicator (connected/connecting/disconnected)
pub fn set_status(status: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-status") {
            let _ = el.set_class_name(&format!("status-{}", status));
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
            let el_clone = el.clone();
            let callback = Closure::once(Box::new(move || {
                let _ = el_clone.set_attribute("class", "");
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

/// Set dirty state indicator (unsaved changes)
pub fn set_dirty(dirty: bool) {
    if let Some(doc) = get_document() {
        // Update dirty indicator visibility
        if let Some(el) = doc.get_element_by_id("nvim-dirty") {
            let _ = el.set_attribute("class", if dirty { "show" } else { "" });
        }
        // Update page title
        let base_title = "Neovim Web";
        let new_title = if dirty { format!("* {}", base_title) } else { base_title.to_string() };
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
