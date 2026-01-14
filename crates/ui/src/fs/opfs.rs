use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    window, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetFileOptions,
    FileSystemHandle, FileSystemWritableFileStream,
};

/// Save a file to OPFS (root)
pub async fn save_file(filename: &str, data: &[u8]) -> Result<(), JsValue> {
    let win = window().ok_or("No window")?;
    let navigator = win.navigator();
    let storage = navigator.storage();

    // Get root directory handle
    let root_promise = storage.get_directory();
    let root_val = JsFuture::from(root_promise).await?;
    let root_dir = root_val.dyn_into::<FileSystemDirectoryHandle>()?;

    // Create file handle
    let opts = FileSystemGetFileOptions::new();
    opts.set_create(true);
    let file_promise = root_dir.get_file_handle_with_options(filename, &opts);
    let file_val = JsFuture::from(file_promise).await?;
    let file_handle = file_val.dyn_into::<FileSystemFileHandle>()?;

    // Create writable stream
    let writable_promise = file_handle.create_writable();
    let writable_val = JsFuture::from(writable_promise).await?;
    let writable = writable_val.dyn_into::<FileSystemWritableFileStream>()?;

    // Write data
    let array = js_sys::Uint8Array::from(data);
    let write_promise = writable.write_with_buffer_source(&array)?;
    JsFuture::from(write_promise).await?;

    // Close
    let close_promise = writable.close();
    JsFuture::from(close_promise).await?;

    web_sys::console::log_1(&format!("[OPFS] Saved: {}", filename).into());
    Ok(())
}

/// Read a file from OPFS (root)
pub async fn read_file(filename: &str) -> Result<Vec<u8>, JsValue> {
    let win = window().ok_or("No window")?;
    let navigator = win.navigator();
    let storage = navigator.storage();

    let root_promise = storage.get_directory();
    let root_val = JsFuture::from(root_promise).await?;
    let root_dir = root_val.dyn_into::<FileSystemDirectoryHandle>()?;

    let file_promise = root_dir.get_file_handle(filename);
    let file_val = JsFuture::from(file_promise).await?;
    let file_handle = file_val.dyn_into::<FileSystemFileHandle>()?;

    let file_obj_promise = file_handle.get_file();
    let file_obj = JsFuture::from(file_obj_promise).await?;
    let file = file_obj.dyn_into::<web_sys::File>()?;

    let array_buffer_promise = file.array_buffer();
    let array_buffer = JsFuture::from(array_buffer_promise).await?;
    let uint8 = js_sys::Uint8Array::new(&array_buffer);

    Ok(uint8.to_vec())
}

/// Read all files from OPFS and restore them (returns list of filenames)
/// This is a simplified version that just lists them for now.
/// List all files from OPFS (recursive)
pub async fn list_files() -> Result<Vec<String>, JsValue> {
    let win = window().ok_or("No window")?;
    let navigator = win.navigator();
    let storage = navigator.storage();

    let root_promise = storage.get_directory();
    let root_val = JsFuture::from(root_promise).await?;
    let root_dir = root_val.dyn_into::<FileSystemDirectoryHandle>()?;

    let mut files = Vec::new();
    read_directory(&root_dir, "", &mut files).await?;
    Ok(files)
}

async fn read_directory(
    dir: &FileSystemDirectoryHandle,
    path_prefix: &str,
    files: &mut Vec<String>,
) -> Result<(), JsValue> {
    // Use JS helper to get array of entries.
    let promise = get_entries_array(dir);
    let result = JsFuture::from(promise).await?;
    let entries: js_sys::Array = result.dyn_into()?;

    // js_sys::Array::to_vec returns Vec<JsValue>
    let vec = entries.to_vec();

    for val in vec {
        let handle = val.dyn_into::<FileSystemHandle>()?;

        // Check if file
        if let Ok(file_handle) = handle.clone().dyn_into::<FileSystemFileHandle>() {
            let name = file_handle.name();
            let full_path = if path_prefix.is_empty() {
                name
            } else {
                format!("{}/{}", path_prefix, name)
            };
            files.push(full_path);
        } else if let Ok(_dir_handle) = handle.dyn_into::<FileSystemDirectoryHandle>() {
            // Skipping recursive directory walking for now
        }
    }

    Ok(())
}

#[wasm_bindgen(inline_js = "
    export async function get_entries_array(dirHandle) {
        const entries = [];
        for await (const entry of dirHandle.values()) {
            entries.push(entry);
        }
        return entries;
    }
")]
extern "C" {
    fn get_entries_array(dir: &FileSystemDirectoryHandle) -> js_sys::Promise;
}
