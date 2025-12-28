use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket, CloseEvent, ErrorEvent, window};
use std::rc::Rc;
use std::cell::RefCell;

use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use crate::render::RenderState;
use crate::dom::{set_status, show_toast};
use crate::handler::handle_message;

pub fn setup_websocket(
    ws_url: &str,
    initial_cols: u32,
    initial_rows: u32,
    grids: Rc<RefCell<GridManager>>,
    render_state: Rc<RenderState>,
    highlights: Rc<RefCell<HighlightMap>>,
) -> Result<WebSocket, JsValue> {
    let ws = WebSocket::new(ws_url)?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
    
    // ON OPEN
    let ws_clone = ws.clone();
    let onopen = Closure::wrap(Box::new(move || {
        web_sys::console::log_1(&"WS: Connected".into());
        set_status("connected");
        show_toast("Connected to host");
        
        // Initial Resize
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("resize".into()),
            rmpv::Value::Integer((initial_cols as i64).into()),
            rmpv::Value::Integer((initial_rows as i64).into()),
        ]);
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            let _ = ws_clone.send_with_u8_array(&bytes);
        }
        
        // Settings Request (ID 1)
        let settings_req = rmpv::Value::Array(vec![
            rmpv::Value::Integer(0.into()),
            rmpv::Value::Integer(1.into()),
            rmpv::Value::String("settings_all".into()),
            rmpv::Value::Array(vec![]),
        ]);
         let mut settings_bytes = Vec::new();
        if rmpv::encode::write_value(&mut settings_bytes, &settings_req).is_ok() {
            let _ = ws_clone.send_with_u8_array(&settings_bytes);
        }
    }) as Box<dyn FnMut()>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();
    
    // ON MESSAGE
    let ws_clone2 = ws.clone();
    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let bytes = array.to_vec();
            let mut cursor = std::io::Cursor::new(bytes);
            
            if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                // Async processing
                let grids = grids.clone();
                let render_state = render_state.clone();
                let highlights = highlights.clone();
                let ws_socket = ws_clone2.clone();
                
                wasm_bindgen_futures::spawn_local(async move {
                    handle_message(msg, grids, render_state, highlights, ws_socket).await;
                });
            }
        }
    }) as Box<dyn FnMut(_)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
    
    // ON ERROR
    let onerror = Closure::wrap(Box::new(move |e: ErrorEvent| {
        web_sys::console::error_1(&"WS ERROR".into());
        set_status("disconnected");
        show_toast("Connection error");
    }) as Box<dyn FnMut(_)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();
    
    // ON CLOSE
    let onclose = Closure::wrap(Box::new(move |e: CloseEvent| {
        web_sys::console::warn_1(&format!("WS CLOSE: {} {}", e.code(), e.reason()).into());
        if e.code() != 1000 && e.code() != 1001 {
             set_status("disconnected");
             show_toast("Disconnected");
             // Clear session on abnormal disconnect
             if let Some(win) = window() {
                 if let Ok(Some(s)) = win.local_storage() {
                     let _ = s.remove_item("nvim_session_id");
                 }
             }
        }
    }) as Box<dyn FnMut(_)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
    
    Ok(ws)
}
