#!/usr/bin/env bash
#
# nvim-web: Phase 4 Part 3 - UI Refactoring
# Decomposes network and handler logic from lib.rs
#

set -euo pipefail
IFS=$'\n\t'

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly UI_SRC="${PROJECT_ROOT}/crates/ui/src"

DRY_RUN=false

log_info() { echo -e "\033[0;34m[INFO]\033[0m $1"; }
log_step() { echo -e "\n\033[0;32m==>\033[0m \033[0;34m$1\033[0m"; }

step_1_create_handler_module() {
    log_step "Step 1: Creating crates/ui/src/handler.rs"
    
    cat > "${UI_SRC}/handler.rs" << 'RUST'
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, WebSocket};
use std::rc::Rc;
use std::cell::RefCell;
use rmpv::Value;

use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use crate::renderer::Renderer;
use crate::render::RenderState;
use crate::dom::{set_status, show_toast, update_drawer_session, update_drawer_cwd_info};
use crate::opfs::js_handle_fs_request;
use crate::events::apply_redraw;

/// Handle incoming MessagePack message
pub async fn handle_message(
    msg: Value,
    grids: Rc<RefCell<GridManager>>,
    render_state: Rc<RefCell<RenderState>>,
    highlights: Rc<RefCell<HighlightMap>>,
    ws: WebSocket, // Cloned socket for responses
) {
    if let Value::Array(ref arr) = msg {
        if arr.len() >= 2 {
            // Check for Protocol Message Type (Integer)
            // Type 2: FS Request or Notification
            if let Value::Integer(ref msg_type) = arr[0] {
                if msg_type.as_i64() == Some(2) {
                     // Check if this is an FS request (arr[1] is integer ID)
                    if let Value::Integer(ref fs_id) = arr[1] {
                        let request_id = fs_id.as_u64().unwrap_or(0) as u32;
                        handle_fs_request(arr, request_id, ws).await;
                        return;
                    }
                }
                
                // Type 1: RPC Response
                if msg_type.as_i64() == Some(1) {
                     handle_rpc_response(arr, ws).await;
                     return;
                }
            }
            
            // Check for String messages (Session, CWD Info, Redraw?)
            // Note: Redraw is [2, "redraw", [...]] - handled by logic below via default ApplyRedraw?
            // Actually, existing code handles redraw in the "default" case if it's not caught above.
            
            if let Value::String(ref method) = arr[0] {
                // Session: ["session", id]
                if method.as_str() == Some("session") {
                     handle_session_message(arr, ws).await;
                     return;
                }
                
                // Cwd Info Push: ["cwd_info", {...}]
                if method.as_str() == Some("cwd_info") {
                    handle_cwd_info_push(arr).await;
                    return;
                }
            }
        }
    }
    
    // Default: Treat as Redraw Notification [2, "redraw", events]
    // The existing logic passed `msg` directly to `apply_redraw`.
    // We assume if it wasn't intercepted above, it's a redraw or ignored.
    apply_redraw(msg, &mut grids.borrow_mut(), &mut highlights.borrow_mut(), &mut render_state.borrow_mut());
}

async fn handle_fs_request(arr: &Vec<Value>, request_id: u32, ws: WebSocket) {
    // Parse Payload: [2, id, [op, ns, path, data?]]
    if let Value::Array(ref payload) = arr[2] {
        if payload.len() >= 3 {
            let op = payload[0].as_str().unwrap_or("").to_string();
            let ns = payload[1].as_str().unwrap_or("default").to_string();
            let path = payload[2].as_str().unwrap_or("").to_string();
            
            let data: Option<Vec<u8>> = if payload.len() >= 4 {
                if let Value::Binary(ref bytes) = payload[3] {
                    Some(bytes.clone())
                } else { None }
            } else { None };
            
            web_sys::console::log_1(&format!("FS: Request id={} op={} ns={} path={}", request_id, op, ns, path).into());

            // Prepare JS data
            let js_data = data.map(|bytes| {
                let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                arr.copy_from(&bytes);
                arr
            });
            
            // Call JS OPFS
            let result = js_handle_fs_request(&op, &ns, &path, js_data, request_id).await;
            
            // Build response
            let response = match result {
                Ok(js_result) => {
                    let ok = js_sys::Reflect::get(&js_result, &"ok".into()).ok().and_then(|v| v.as_bool()).unwrap_or(false);
                     if ok {
                         let result_val = js_sys::Reflect::get(&js_result, &"result".into()).ok();
                         let msgpack_result = convert_js_to_msgpack(result_val);
                         
                         Value::Array(vec![
                             Value::Integer(3.into()),
                             Value::Integer((request_id as i64).into()),
                             Value::Boolean(true),
                             msgpack_result,
                         ])
                     } else {
                         let error = js_sys::Reflect::get(&js_result, &"error".into()).ok().and_then(|v| v.as_string()).unwrap_or("Unknown error".to_string());
                          Value::Array(vec![
                             Value::Integer(3.into()),
                             Value::Integer((request_id as i64).into()),
                             Value::Boolean(false),
                             Value::String(error.into()),
                         ])
                     }
                }
                Err(e) => {
                     let error = e.as_string().unwrap_or("JS exception".to_string());
                     Value::Array(vec![
                        Value::Integer(3.into()),
                        Value::Integer((request_id as i64).into()),
                        Value::Boolean(false),
                        Value::String(error.into()),
                    ])
                }
            };
            
            // Send response
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &response).is_ok() {
                 let _ = ws.send_with_u8_array(&bytes);
            }
        }
    }
}

fn convert_js_to_msgpack(val: Option<JsValue>) -> Value {
    if let Some(val) = val {
        if val.is_null() || val.is_undefined() {
            Value::Nil
        } else if let Some(arr) = val.dyn_ref::<js_sys::Uint8Array>() {
            Value::Binary(arr.to_vec())
        } else if let Some(arr) = val.dyn_ref::<js_sys::Array>() {
            let items: Vec<Value> = (0..arr.length())
                .filter_map(|i| arr.get(i).as_string().map(|s| Value::String(s.into())))
                .collect();
            Value::Array(items)
        } else {
            Value::Nil
        }
    } else {
        Value::Nil
    }
}

async fn handle_rpc_response(arr: &Vec<Value>, ws: WebSocket) {
    if arr.len() < 4 { return; }
    let id = arr[1].as_i64().unwrap_or(0);
    let error = &arr[2];
    let result = &arr[3];
    
    // ID 1: Settings
    if id == 1 {
        if error.is_nil() {
            if let Value::Map(ref settings) = result {
                for (k, v) in settings {
                    if let (Some(key), Some(val)) = (k.as_str(), v.as_str()) {
                         web_sys::console::log_1(&format!("SETTING: {}={}", key, val).into());
                    }
                }
            }
        }
    }
    
    // ID 2: CWD Info
    if id == 2 {
        if error.is_nil() {
             process_cwd_info(result);
        }
    }
}

async fn handle_session_message(arr: &Vec<Value>, ws: WebSocket) {
    if let Value::String(ref session_id) = arr[1] {
        if let Some(id) = session_id.as_str() {
             // Reconnection logic
             let is_reconnection = if let Ok(Some(storage)) = window().unwrap().local_storage() {
                let existing = storage.get_item("nvim_session_id").ok().flatten();
                let is_recon = existing.as_ref().map(|e| e == id).unwrap_or(false);
                let _ = storage.set_item("nvim_session_id", id);
                is_recon
            } else { false };
            
            update_drawer_session(id, is_reconnection);
            
            // Request CWD info (ID 2)
            let cwd_req = Value::Array(vec![
                Value::Integer(0.into()),
                Value::Integer(2.into()),
                Value::String("get_cwd_info".into()),
                Value::Array(vec![]),
            ]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &cwd_req).is_ok() {
                let _ = ws.send_with_u8_array(&bytes);
            }
        }
    }
}

async fn handle_cwd_info_push(arr: &Vec<Value>) {
    if let Value::Map(ref info) = arr[1] {
        process_cwd_info(&Value::Map(info.clone()));
    }
}

fn process_cwd_info(val: &Value) {
    if let Value::Map(ref info_map) = val {
        let mut cwd = String::new();
        let mut file = String::new();
        let mut backend = "local".to_string();
        let mut git_branch: Option<String> = None;
        
        for (key, value) in info_map {
             match key.as_str() {
                Some("cwd") => cwd = value.as_str().unwrap_or("~").to_string(),
                Some("file") => file = value.as_str().unwrap_or("").to_string(),
                Some("backend") => backend = value.as_str().unwrap_or("local").to_string(),
                Some("git_branch") => git_branch = value.as_str().map(|s| s.to_string()),
                _ => {}
            }
        }
        update_drawer_cwd_info(&cwd, &file, &backend, git_branch.as_deref());
    }
}
RUST
}

step_2_create_network_module() {
    log_step "Step 2: Creating crates/ui/src/network.rs"
    
    cat > "${UI_SRC}/network.rs" << 'RUST'
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
    render_state: Rc<RefCell<RenderState>>,
    highlights: Rc<RefCell<HighlightMap>>,
) -> Result<WebSocket, JsValue> {
    let ws = WebSocket::new(ws_url)?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
    
    // ON OPEN
    let ws_clone = ws.clone();
    let onopen = Closure::handle(Box::new(move || {
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
    let onmessage = Closure::handle(Box::new(move |e: MessageEvent| {
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
    let onerror = Closure::handle(Box::new(move |e: ErrorEvent| {
        web_sys::console::error_1(&"WS ERROR".into());
        set_status("disconnected");
        show_toast("Connection error");
    }) as Box<dyn FnMut(_)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();
    
    // ON CLOSE
    let onclose = Closure::handle(Box::new(move |e: CloseEvent| {
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
RUST
}

step_3_register_modules() {
    log_step "Step 3: Registering modules in lib.rs"
    if ! grep -q "mod handler;" "${UI_SRC}/lib.rs"; then
         sed -i '' '/mod session;/a\
mod handler;\
mod network;
' "${UI_SRC}/lib.rs"
    fi
}

main() {
    step_1_create_handler_module
    step_2_create_network_module
    step_3_register_modules
}

main "$@"
