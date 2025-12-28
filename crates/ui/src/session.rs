use wasm_bindgen::prelude::*;
use web_sys::{window, Window};
use crate::dom::show_toast;

/// Output of session initialization
pub struct SessionConfig {
    pub ws_url: String,
    pub open_token: Option<String>,
}

/// Initialize session from URL params and `LocalStorage`
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
            .map(ToString::to_string)
    } else {
        None
    };
    
    // Parse ?open= from URL (magic link)
    let open_token: Option<String> = if search.contains("open=") {
        let search_clean = search.trim_start_matches('?');
        search_clean.split('&')
            .find(|p| p.starts_with("open="))
            .and_then(|p| p.strip_prefix("open="))
            .map(ToString::to_string)
    } else {
        None
    };
    
    // If we have an open token, notify user
    if let Some(ref token) = open_token {
        web_sys::console::log_1(&format!("MAGIC LINK: Claiming token {token}").into());
        show_toast("Opening project...");
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
            web_sys::console::log_1(&format!("SESSION: Joining session {id} (URL param)").into());
            (format!("ws://127.0.0.1:9001?session={id}"), true)
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
            
            existing_session.as_ref().map_or_else(
                || {
                    web_sys::console::log_1(&"SESSION: Creating new session".into());
                    ("ws://127.0.0.1:9001?session=new".to_string(), false)
                },
                |id| {
                    web_sys::console::log_1(&format!("SESSION: Reconnecting to session {id}").into());
                    (format!("ws://127.0.0.1:9001?session={id}"), false)
                },
            )
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
#[allow(dead_code)]
pub fn force_new_session(win: &Window) {
    if let Some(storage) = win.local_storage().ok().flatten() {
        let _ = storage.remove_item("nvim_session_id");
    }
    // Logic to reload page would go here if needed, or caller handles it
}
