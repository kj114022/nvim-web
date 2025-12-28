#!/usr/bin/env bash
#
# nvim-web: Phase 4 Part 1 - UI Refactoring
# Decomposes dom and opfs logic from lib.rs
#

set -euo pipefail
IFS=$'\n\t'

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly UI_SRC="${PROJECT_ROOT}/crates/ui/src"

DRY_RUN=false

# -----------------------------------------------------------------------------
# Logging
# -----------------------------------------------------------------------------

log_info() { echo -e "\033[0;34m[INFO]\033[0m $1"; }
log_success() { echo -e "\033[0;32m[OK]\033[0m $1"; }
log_warn() { echo -e "\033[1;33m[WARN]\033[0m $1"; }
log_step() { echo -e "\n\033[0;32m==>\033[0m \033[0;34m$1\033[0m"; }

# -----------------------------------------------------------------------------
# Steps
# -----------------------------------------------------------------------------

# Helper to check if file contains string
contains() { grep -q "$1" "$2" 2>/dev/null; }

step_1_create_dom_module() {
    log_step "Step 1: Creating crates/ui/src/dom.rs"
    
     if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create dom.rs"
        return
    fi
    
    cat > "${UI_SRC}/dom.rs" << 'RUST'
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
RUST
    log_success "Created dom.rs"
}

step_2_create_opfs_module() {
    log_step "Step 2: Creating crates/ui/src/opfs.rs"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create opfs.rs"
        return
    fi

    cat > "${UI_SRC}/opfs.rs" << 'RUST'
use wasm_bindgen::prelude::*;

// JavaScript OPFS bridge - calls handleFsRequest from opfs.ts
#[wasm_bindgen(module = "/fs/opfs.js")]
extern "C" {
    #[wasm_bindgen(js_name = handleFsRequest, catch)]
    pub async fn js_handle_fs_request(
        op: &str,
        ns: &str,
        path: &str,
        data: Option<js_sys::Uint8Array>,
        id: u32,
    ) -> Result<JsValue, JsValue>;
}
RUST
    log_success "Created opfs.rs"
}

step_3_register_modules() {
    log_step "Step 3: Registering modules in lib.rs"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would modify lib.rs"
        return
    fi
    
    # We do NOT want to overwrite lib.rs entirely as it has complex logic.
    # We will assume the user (me, the agent) will manually remove the implementations
    # and add use statements. But we CAN inject the mod declarations.
    
    # Prepend mod declarations if not present
    if ! grep -q "mod dom;" "${UI_SRC}/lib.rs"; then
         # Insert after "mod events;"
         sed -i '' '/mod events;/a\
mod dom;\
mod opfs;
' "${UI_SRC}/lib.rs"
         log_success "Injected mod declarations into lib.rs"
    fi
}

step_4_verify_build_check() {
    log_step "Step 4: Verification (check only)"
    
    # This WILL fail because of duplicate definitions in lib.rs until we remove them manually.
    if [[ "${DRY_RUN}" == true ]]; then
        return
    fi
    
    log_warn "Expect build errors until manual removal of duplicate code in lib.rs"
}

main() {
     if [[ "${1:-}" == "--dry-run" ]]; then
        DRY_RUN=true
        log_warn "DRY RUN MODE"
    fi
    
    step_1_create_dom_module
    step_2_create_opfs_module
    step_3_register_modules
    step_4_verify_build_check
    
    log_success "Phase 4 Part 1 - UI Refactoring Prep Complete"
    log_info "Next: Manually delete moved functions from lib.rs and update call sites."
}

main "$@"
