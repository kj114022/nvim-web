use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Request, Response};
use wasm_bindgen_futures::JsFuture;

pub async fn render() -> Result<(), JsValue> {
    let win = window().ok_or("No window")?;
    let doc = win.document().ok_or("No document")?;
    
    // Fetch sessions
    let req = Request::new_with_str("/api/sessions")?;
    let resp_val = JsFuture::from(win.fetch_with_request(&req)).await?;
    let resp: Response = resp_val.dyn_into()?;
    
    if !resp.ok() {
        web_sys::console::error_1(&"Failed to fetch sessions".into());
        return Ok(());
    }
    
    let json_val = JsFuture::from(resp.json()?).await?;
    
    // Target #session-list (inside #panel-sessions)
    let sessions_list_div = doc.get_element_by_id("session-list").ok_or("No session list")?;
    sessions_list_div.set_inner_html(""); 
    
    // Get "sessions" array
    let sessions = js_sys::Reflect::get(&json_val, &"sessions".into())?;
    
    if let Some(arr) = sessions.dyn_ref::<js_sys::Array>() {
        if arr.length() == 0 {
             let empty = doc.create_element("div")?;
             empty.set_class_name("session-item");
             empty.set_attribute("style", "justify-content:center; color:#858585; cursor:default;")?;
             empty.set_inner_html("<span class='session-id'>No active sessions</span>");
             sessions_list_div.append_child(&empty)?;
        }
        
        for i in 0..arr.length() {
            let sess = arr.get(i);
            let id = js_sys::Reflect::get(&sess, &"id".into())?.as_string().unwrap_or_default();
            
            let item = doc.create_element("div")?;
            item.set_class_name("session-item");
            
            // Inner HTML
            item.set_inner_html(&format!(r#"
                <div class="session-id">{}</div>
                <div class="session-info">Active</div>
            "#, id));
            
            // Click handler -> Join
            let id_clone = id.clone();
            let cb = Closure::wrap(Box::new(move || {
                let _ = web_sys::window().unwrap().location().set_href(&format!("?session={}", id_clone));
            }) as Box<dyn FnMut()>);
            item.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
            cb.forget();
            
            sessions_list_div.append_child(&item)?;
        }
    }
    
    Ok(())
}
