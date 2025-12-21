use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, KeyboardEvent, ErrorEvent};
use rmpv::Value;

use crate::grid::Grid;
use crate::renderer::Renderer;

pub struct Protocol {
    ws: WebSocket,
    grid: Grid,
    renderer: Renderer,
}

impl Protocol {
    pub fn new(grid: Grid, renderer: Renderer) -> Result<Self, JsValue> {
        let ws = WebSocket::new("ws://127.0.0.1:9001")?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        
        Ok(Self { ws, grid, renderer })
    }
    
    pub fn setup_handlers(mut self) -> Result<(), JsValue> {
        // Handle WebSocket messages
        let grid_clone = self.grid.clone();
        let renderer_clone = self.renderer.clone();
        
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                let array = js_sys::Uint8Array::new(&abuf);
                let bytes = array.to_vec();
                
                // Decode msgpack
                let mut cursor = std::io::Cursor::new(bytes);
                if let Ok(msg) = rmpv::decode::read_value(&mut cursor) {
                    // Apply redraw event to grid
                    // grid_clone is not mut here - need to refactor
                    web_sys::console::log_1(&format!("Received redraw: {:?}", msg).into());
                }
            }
        }) as Box<dyn FnMut(_)>);
        
        self.ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
        
        // Handle errors
        let onerror = Closure::wrap(Box::new(move |e: ErrorEvent| {
            web_sys::console::error_1(&format!("WebSocket error: {:?}", e).into());
        }) as Box<dyn FnMut(_)>);
        
        self.ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
        
        // Handle open
        let onopen = Closure::wrap(Box::new(move |_| {
            web_sys::console::log_1(&"WebSocket connected!".into());
        }) as Box<dyn FnMut(_)>);
        
        self.ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();
        
        Ok(())
    }
    
    pub fn send_input(&self, keys: &str) -> Result<(), JsValue> {
        // Encode ["input", keys] as msgpack
        let msg = Value::Array(vec![
            Value::String("input".into()),
            Value::String(keys.into()),
        ]);
        
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &msg).map_err(|e| {
            JsValue::from_str(&format!("Encode error: {}", e))
        })?;
        
        self.ws.send_with_u8_array(&bytes)?;
        Ok(())
    }
}

pub fn setup_keyboard(protocol_ws: WebSocket) -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let key = e.key();
        
        // Simple key mapping - expand later
        let nvim_key = match key.as_str() {
            "Enter" => "<CR>",
            "Escape" => "<Esc>",
            "Backspace" => "<BS>",
            "Tab" => "<Tab>",
            "ArrowUp" => "<Up>",
            "ArrowDown" => "<Down>",
            "ArrowLeft" => "<Left>",
            "ArrowRight" => "<Right>",
            _ if key.len() == 1 => &key,
            _ => return,
        };
        
        // Send input via WebSocket
        let msg = Value::Array(vec![
            Value::String("input".into()),
            Value::String(nvim_key.into()),
        ]);
        
        if let Ok(mut bytes) = Vec::new().into() {
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = protocol_ws.send_with_u8_array(&bytes);
            }
        }
        
        e.prevent_default();
    }) as Box<dyn FnMut(_)>);
    
    document.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();
    
    Ok(())
}
