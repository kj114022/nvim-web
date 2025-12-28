#!/usr/bin/env bash
#
# nvim-web: Phase 4 Part 2 - UI Refactoring
# Decomposes session logic from lib.rs
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

step_1_create_session_module() {
    log_step "Step 1: Creating crates/ui/src/session.rs"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create session.rs"
        return
    fi
    
    cat > "${UI_SRC}/session.rs" << 'RUST'
use wasm_bindgen::prelude::*;
use web_sys::{window, Window};
use crate::dom::show_toast;

/// Output of session initialization
pub struct SessionConfig {
    pub ws_url: String,
    pub open_token: Option<String>,
}

/// Initialize session from URL params and LocalStorage
///
/// Handles:
/// - Parsing ?session= and ?open=
/// - Reconnecting to stored session
/// - Cleaning URL parameters
/// - Magic link token extraction
pub fn init_session() -> Result<SessionConfig, JsValue> {
    let win = window().ok_or("No window found")?;
    let search = win.location().search().unwrap_or_default();
    let storage = win.local_storage().ok().flatten();
    
    // Parse ?session= from URL (handles both ?session=x and &session=x)
    let url_session: Option<String> = if search.contains("session=") {
        let search_clean = search.trim_start_matches('?');
        search_clean.split('&')
            .find(|p| p.starts_with("session="))
            .and_then(|p| p.strip_prefix("session="))
            .map(|s| s.to_string())
    } else {
        None
    };
    
    // Parse ?open= from URL (magic link)
    let open_token: Option<String> = if search.contains("open=") {
        let search_clean = search.trim_start_matches('?');
        search_clean.split('&')
            .find(|p| p.starts_with("open="))
            .and_then(|p| p.strip_prefix("open="))
            .map(|s| s.to_string())
    } else {
        None
    };
    
    // If we have an open token, notify user
    if let Some(ref token) = open_token {
        web_sys::console::log_1(&format!("MAGIC LINK: Claiming token {}", token).into());
        show_toast(&format!("Opening project..."));
    }
    
    // Determine session ID: URL param takes priority over localStorage
    let (ws_url, should_clear_url) = match url_session {
        Some(ref id) if id == "new" => {
            // Force new session - clear localStorage
            if let Some(ref s) = storage {
                let _ = s.remove_item("nvim_session_id");
            }
            web_sys::console::log_1(&"SESSION: Forcing new session (URL param)".into());
            ("ws://127.0.0.1:9001?session=new".to_string(), true)
        }
        Some(ref id) => {
            // Join specific session from URL
            web_sys::console::log_1(&format!("SESSION: Joining session {} (URL param)", id).into());
            (format!("ws://127.0.0.1:9001?session={}", id), true)
        }
        None if open_token.is_some() => {
            // Magic link - always create new session
            web_sys::console::log_1(&"SESSION: Creating new session for magic link".into());
            ("ws://127.0.0.1:9001?session=new".to_string(), true)
        }
        None => {
            // No URL param, check localStorage
            let existing_session = storage.as_ref()
                .and_then(|s| s.get_item("nvim_session_id").ok())
                .flatten();
            
            match existing_session {
                Some(ref id) => {
                    web_sys::console::log_1(&format!("SESSION: Reconnecting to session {}", id).into());
                    (format!("ws://127.0.0.1:9001?session={}", id), false)
                }
                None => {
                    web_sys::console::log_1(&"SESSION: Creating new session".into());
                    ("ws://127.0.0.1:9001?session=new".to_string(), false)
                }
            }
        }
    };
    
    // Clean URL params if needed
    if should_clear_url {
        if let Ok(history) = win.history() {
            let pathname = win.location().pathname().unwrap_or_default();
            let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&pathname));
        }
    }
    
    Ok(SessionConfig {
        ws_url,
        open_token,
    })
}

/// Force new session (clears storage and connection info)
pub fn force_new_session(win: &Window) {
    if let Some(storage) = win.local_storage().ok().flatten() {
        let _ = storage.remove_item("nvim_session_id");
    }
    // Logic to reload page would go here if needed, or caller handles it
}
RUST
    log_success "Created session.rs"
}

step_2_register_module() {
    log_step "Step 2: Registering session module in lib.rs"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would modify lib.rs"
        return
    fi
     
    # Inject Mod declaration
    if ! grep -q "mod session;" "${UI_SRC}/lib.rs"; then
         sed -i '' '/mod opfs;/a\
mod session;
' "${UI_SRC}/lib.rs"
         log_success "Injected mod declaration"
    fi
}

main() {
     if [[ "${1:-}" == "--dry-run" ]]; then
        DRY_RUN=true
        log_warn "DRY RUN MODE"
    fi
    
    step_1_create_session_module
    step_2_register_module
    
    log_success "Phase 4 Part 2 - UI Session Extraction Prep Complete"
    log_info "Next: Manually replace session logic in lib.rs with session::init_session()."
}

main "$@"
