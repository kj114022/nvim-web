use wasm_bindgen::prelude::*;
use web_sys::window;
use crate::dom::show_toast;

/// Output of session initialization
pub struct SessionConfig {
    pub ws_url: String,
    pub open_token: Option<String>,
}

/// Initialize session from URL params and `LocalStorage`
/// Returns None if no session is active (Dashboard mode)
pub fn init_session() -> Result<Option<SessionConfig>, JsValue> {
    let win = window().ok_or("No window found")?;
    let search = win.location().search().unwrap_or_default();
    let storage = win.local_storage().ok().flatten();
    
    // Get WS port from dynamic config
    let ws_port = if let Ok(config) = js_sys::Reflect::get(&win, &"NVIM_CONFIG".into()) {
         if let Ok(port) = js_sys::Reflect::get(&config, &"wsPort".into()) {
             port.as_f64().unwrap_or(9001.0) as u16
         } else { 9001 }
    } else {
        web_sys::console::warn_1(&"Config not found, defaulting to 9001".into());
        9001
    };
    
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
    
    let base_ws = format!("ws://127.0.0.1:{ws_port}");

    // Determine session ID: URL param takes priority over localStorage
    let config = match url_session {
        Some(ref id) if id == "new" => {
            // Force new session - clear localStorage
            if let Some(ref s) = storage {
                let _ = s.remove_item("nvim_session_id");
            }
            web_sys::console::log_1(&"SESSION: Forcing new session (URL param)".into());
             
             // Get context (current URL) for Firenvim behavior
             let context = win.location().href().unwrap_or_default();
             let encoded_context = js_sys::encode_uri_component(&context);
             Some((format!("{base_ws}?session=new&context={encoded_context}"), true))
        }
        Some(ref id) => {
            // Join specific session from URL
            web_sys::console::log_1(&format!("SESSION: Joining session {id} (URL param)").into());
             
             let context = win.location().href().unwrap_or_default();
             let encoded_context = js_sys::encode_uri_component(&context);
             Some((format!("{base_ws}?session={id}&context={encoded_context}"), true))
        }
        None if open_token.is_some() => {
            // Magic link - always create new session
            web_sys::console::log_1(&"SESSION: Creating new session for magic link".into());
             
             let context = win.location().href().unwrap_or_default();
             let encoded_context = js_sys::encode_uri_component(&context);
             Some((format!("{base_ws}?session=new&context={encoded_context}"), true))
        }
        None => {
            // No URL param, check localStorage
            let existing_session = storage.as_ref()
                .and_then(|s| s.get_item("nvim_session_id").ok())
                .flatten();
            
            existing_session.as_ref().map_or(
                None, // Show Dashboard instead of creating new
                |id| {
                    web_sys::console::log_1(&format!("SESSION: Reconnecting to session {id}").into());
                    
                    let context = win.location().href().unwrap_or_default();
                    let encoded_context = js_sys::encode_uri_component(&context);
                    Some((format!("{base_ws}?session={id}&context={encoded_context}"), false))
                },
            )
        }
    };
    
    if let Some((ws_url, should_clear_url)) = config {
        // Clean URL params if needed
        if should_clear_url {
            if let Ok(history) = win.history() {
                let pathname = win.location().pathname().unwrap_or_default();
                let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&pathname));
            }
        }
        
        Ok(Some(SessionConfig {
            ws_url,
            open_token,
        }))
    } else {
        Ok(None)
    }
}

// NOTE: force_new_session removed - URL-based navigation used instead (?session=new)
