use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::window;

pub mod sessions;
pub mod settings;
pub mod help;

pub async fn init() -> Result<(), JsValue> {
    let win = window().ok_or("No window")?;
    let doc = win.document().ok_or("No document")?;
    
    // Initialize sessions (fetch data)
    sessions::render().await?;
    // Init other modules if needed
    let _ = settings::render();
    let _ = help::render();
    
    // Tab switching logic
    bind_tab(&doc, "tab-sessions", "panel-sessions")?;
    bind_tab(&doc, "tab-connections", "panel-connections")?;
    bind_tab(&doc, "tab-settings", "panel-settings")?;
    bind_tab(&doc, "tab-help", "panel-help")?;
    
    Ok(())
}

fn bind_tab(doc: &web_sys::Document, tab_id: &str, panel_id: &str) -> Result<(), JsValue> {
    if let Some(el) = doc.get_element_by_id(tab_id) {
        let panel_id = panel_id.to_string();
        let cb = Closure::wrap(Box::new(move || {
           switch_to(&panel_id);
        }) as Box<dyn FnMut()>);
        el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    Ok(())
}

fn switch_to(target_id: &str) {
    let doc = window().unwrap().document().unwrap();
    let panels = ["panel-sessions", "panel-connections", "panel-settings", "panel-help"];
    let tabs = ["tab-sessions", "tab-connections", "tab-settings", "tab-help"];
    
    for panel in panels {
        if let Some(el) = doc.get_element_by_id(panel) {
            if panel == target_id {
                let _ = el.class_list().remove_1("hidden");
            } else {
                let _ = el.class_list().add_1("hidden");
            }
        }
    }
     // Update tab active state
    let active_tab = format!("tab-{}", target_id.trim_start_matches("panel-"));
    for tab in tabs {
        if let Some(el) = doc.get_element_by_id(tab) {
            if tab == active_tab {
                let _ = el.class_list().add_1("active");
            } else {
                let _ = el.class_list().remove_1("active");
            }
        }
    }
}
