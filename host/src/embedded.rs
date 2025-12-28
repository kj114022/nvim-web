//! Embedded UI assets for single-binary distribution
//!
//! Uses rust-embed to compile all UI assets (HTML, WASM, JS, icons) into the binary.
//! In debug mode, files are loaded from disk for hot-reloading.
//! In release mode, files are embedded in the binary.

use rust_embed::RustEmbed;

/// Embedded UI assets from the ui/ directory
#[derive(RustEmbed)]
#[folder = "../ui/"]
#[include = "index.html"]
#[include = "pkg/*.js"]
#[include = "pkg/*.wasm"]
#[include = "pkg/*.d.ts"]
#[include = "sw.js"]
#[include = "manifest.json"]
#[include = "icons/*"]
pub struct UiAssets;

/// Get a file from embedded assets with proper MIME type
pub fn get_asset(path: &str) -> Option<(Vec<u8>, &'static str)> {
    // Handle root path
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };

    UiAssets::get(path).map(|file| {
        let mime = mime_guess::from_path(path)
            .first_raw()
            .unwrap_or("application/octet-stream");
        (file.data.into_owned(), mime)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_html_exists() {
        assert!(UiAssets::get("index.html").is_some());
    }

    #[test]
    fn test_get_asset() {
        let (data, mime) = get_asset("index.html").expect("index.html should exist");
        assert!(!data.is_empty());
        assert_eq!(mime, "text/html");
    }
}
