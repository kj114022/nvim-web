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
