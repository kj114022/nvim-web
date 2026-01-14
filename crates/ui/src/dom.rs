//! DOM helpers for UI
#![allow(dead_code)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Document};

/// Get document helper
fn get_document() -> Option<Document> {
    window().and_then(|w| w.document())
}

/// Set connection status indicator (connected/connecting/disconnected)
pub fn set_status(status: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-status") {
            el.set_class_name(&format!("status-{status}"));
        }
    }
}

/// Show a toast notification (auto-hides after 3 seconds)
pub fn show_toast(message: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-toast") {
            el.set_text_content(Some(message));
            let _ = el.set_attribute("class", "show");

            // Auto-hide after 3 seconds
            let callback = Closure::once(Box::new(move || {
                let _ = el.set_attribute("class", "");
            }) as Box<dyn FnOnce()>);

            if let Some(win) = window() {
                let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    3000,
                );
            }
            callback.forget();
        }
    }
}

/// Focus the hidden input textarea (for IME/mobile)
pub fn focus_input() {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-input") {
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html_el.focus();
            }
        }
    }
}

/// Update hidden input position to follow cursor (for IME)
pub fn update_input_position(x: f64, y: f64) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("nvim-input") {
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = html_el.style().set_property("left", &format!("{}px", x));
                let _ = html_el.style().set_property("top", &format!("{}px", y));
            }
        }
    }
}
/// Load a font from bytes using FontFace API
pub fn load_font_face(family: &str, data: &[u8]) -> Result<(), JsValue> {
    let uint8_array = js_sys::Uint8Array::from(data);
    let buffer = uint8_array.buffer();

    // Create FontFace
    let font_face = web_sys::FontFace::new_with_array_buffer(family, &buffer)?;

    let _font_face_clone = font_face.clone();
    let family_clone = family.to_string();

    let closure = Closure::wrap(Box::new(move |loaded_face: JsValue| {
        if let Some(doc) = get_document() {
            // Add to document.fonts
            let fonts = doc.fonts(); // Returns FontFaceSet
            let _ = fonts.add(&loaded_face.unchecked_into());

            web_sys::console::log_1(&format!("[DOM] Font loaded: {}", family_clone).into());
            show_toast(&format!("Font '{}' installed!", family_clone));
        }
    }) as Box<dyn FnMut(JsValue)>);

    // Load the font
    let _ = font_face.load()?.then(&closure);
    closure.forget();

    Ok(())
}

/// Show/Update an image overlay
pub fn update_image(id: &str, url: &str, x: f64, y: f64, width: f64, height: f64) {
    if let Some(doc) = get_document() {
        if let Some(container) = doc.get_element_by_id("nvim-images") {
            let img_id = format!("img-{}", id);

            // Check if exists
            let img = if let Some(existing) = doc.get_element_by_id(&img_id) {
                existing.dyn_into::<web_sys::HtmlImageElement>().ok()
            } else {
                // Create new
                if let Ok(el) = doc.create_element("img") {
                    let _ = el.set_attribute("id", &img_id);
                    let _ = el.set_attribute("class", "nvim-overlay-image");
                    let _ = el.set_attribute("style", "position:absolute;display:block;");
                    let _ = container.append_child(&el);
                    el.dyn_into::<web_sys::HtmlImageElement>().ok()
                } else {
                    None
                }
            };

            if let Some(img) = img {
                let _ = img.set_src(url);
                let style = img.style();
                let _ = style.set_property("left", &format!("{}px", x));
                let _ = style.set_property("top", &format!("{}px", y));
                let _ = style.set_property("width", &format!("{}px", width));
                let _ = style.set_property("height", &format!("{}px", height));
            }
        }
    }
}

/// Remove an image overlay
pub fn remove_image(id: &str) {
    if let Some(doc) = get_document() {
        let img_id = format!("img-{}", id);
        if let Some(el) = doc.get_element_by_id(&img_id) {
            el.remove();
        }
    }
}

/// Clear all images
pub fn clear_images() {
    if let Some(doc) = get_document() {
        if let Some(container) = doc.get_element_by_id("nvim-images") {
            container.set_inner_html("");
        }
    }
}

/// Trigger the hidden file picker
pub fn open_file_picker() {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("file-picker") {
            if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                // Reset value to allow selecting same file again
                input.set_value("");
                input.click();
            }
        }
    }
}

/// Show or hide the Start Screen
pub fn show_start_screen(show: bool) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("start-screen") {
            let class_list = el.class_list();
            if show {
                let _ = class_list.remove_1("hidden");
            } else {
                let _ = class_list.add_1("hidden");
            }
        }
    }
}

/// Populate the Recent Files list in the Start Screen
pub fn populate_recent_files(files: Vec<String>) {
    if let Some(doc) = get_document() {
        if let Some(list) = doc.get_element_by_id("recent-files-list") {
            list.set_inner_html(""); // Clear existing

            if files.is_empty() {
                let li = doc.create_element("li").unwrap();
                li.set_text_content(Some("No recent files found."));
                let _ = li.set_attribute("class", "recent-empty");
                let _ = list.append_child(&li);
            } else {
                for (idx, file) in files.iter().enumerate() {
                    if let Ok(li) = doc.create_element("li") {
                        let _ = li.set_attribute("class", "recent-item");
                        let _ = li.set_attribute("data-filename", file);
                        // Add with numbered key
                        li.set_inner_html(&format!(
                            "<span class=\"item-key\">{}</span>{}",
                            idx + 1,
                            file
                        ));
                        let _ = list.append_child(&li);
                    }
                }
            }
        }
    }
}

/// Update the mode badge in the status bar
pub fn update_mode_badge(mode: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("mode-badge") {
            // Map mode to display name and CSS class
            let (display_name, class_name) = match mode.to_lowercase().as_str() {
                "normal" | "n" => ("Normal", "mode-normal"),
                "insert" | "i" => ("Insert", "mode-insert"),
                "visual" | "v" => ("Visual", "mode-visual"),
                "v-line" | "V" => ("V-Line", "mode-visual"),
                "v-block" | "\x16" => ("V-Block", "mode-visual"),
                "select" | "s" => ("Select", "mode-visual"),
                "replace" | "R" | "r" => ("Replace", "mode-replace"),
                "command" | "c" => ("Command", "mode-command"),
                "terminal" | "t" => ("Terminal", "mode-insert"),
                _ => (mode, "mode-normal"),
            };

            el.set_text_content(Some(display_name));
            el.set_class_name(&format!("mode-badge {class_name}"));
        }
    }
}

/// Update cursor position display
pub fn update_cursor_pos(line: i32, col: i32) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("cursor-pos") {
            el.set_text_content(Some(&format!("{}:{}", line, col)));
        }
    }
}

/// Update connection status dot
pub fn update_connection_status(status: &str) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("connection-dot") {
            el.set_class_name(&format!("connection-dot {status}"));
            let title = match status {
                "connected" => "Connected",
                "connecting" => "Connecting...",
                "disconnected" => "Disconnected",
                _ => status,
            };
            let _ = el.set_attribute("title", title);
        }
    }
}

/// Update git branch display
pub fn update_git_branch(branch: Option<&str>) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("git-branch") {
            if let Some(branch) = branch {
                el.set_text_content(Some(branch));
                let _ = el
                    .dyn_ref::<web_sys::HtmlElement>()
                    .map(|e| e.style().set_property("display", "flex"));
            } else {
                let _ = el
                    .dyn_ref::<web_sys::HtmlElement>()
                    .map(|e| e.style().set_property("display", "none"));
            }
        }
    }
}

/// Update file path display in status bar
pub fn update_file_path(path: Option<&str>) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("file-path") {
            if let Some(path) = path {
                // Truncate long paths
                let display = if path.len() > 40 {
                    format!("...{}", &path[path.len() - 37..])
                } else {
                    path.to_string()
                };
                el.set_text_content(Some(&display));
            } else {
                el.set_text_content(Some(""));
            }
        }
    }
}

/// Update file type display
pub fn update_file_type(filetype: Option<&str>) {
    if let Some(doc) = get_document() {
        if let Some(el) = doc.get_element_by_id("file-type") {
            if let Some(ft) = filetype {
                el.set_text_content(Some(ft));
            } else {
                el.set_text_content(Some(""));
            }
        }
    }
}

/// Update the hints panel with context-aware keybindings based on current mode
pub fn update_hints_panel(mode: &str) {
    if let Some(doc) = get_document() {
        if let Some(panel) = doc.get_element_by_id("hints-panel") {
            // Get hints for this mode
            let (header, hints) = get_hints_for_mode(mode);

            // Build HTML content
            let mut html = format!(r#"<div class="hints-header">{}</div>"#, header);
            for (key, desc) in hints {
                html.push_str(&format!(
                    r#"<div class="hint-row"><span class="hint-key">{}</span><span class="hint-desc">{}</span></div>"#,
                    key, desc
                ));
            }

            panel.set_inner_html(&html);
        }
    }
}

/// Get keybinding hints for a specific mode
fn get_hints_for_mode(mode: &str) -> (&'static str, Vec<(&'static str, &'static str)>) {
    match mode.to_lowercase().as_str() {
        "insert" | "i" => (
            "Insert Mode",
            vec![
                ("Esc", "exit to normal"),
                ("Ctrl+c", "exit to normal"),
                ("Ctrl+w", "delete word"),
                ("Ctrl+u", "delete to start"),
                ("Ctrl+o", "run one cmd"),
                ("Ctrl+r", "insert register"),
            ],
        ),
        "visual" | "v" | "V" | "\x16" => (
            "Visual Mode",
            vec![
                ("Esc", "exit to normal"),
                ("d", "delete selection"),
                ("y", "yank selection"),
                ("c", "change selection"),
                (">", "indent"),
                ("<", "unindent"),
                ("o", "other end"),
            ],
        ),
        "command" | "c" | "cmdline" => (
            "Command Mode",
            vec![
                ("Enter", "execute"),
                ("Esc", "cancel"),
                ("Ctrl+r", "insert register"),
                ("Tab", "complete"),
                ("↑/↓", "history"),
            ],
        ),
        "replace" | "r" | "R" => (
            "Replace Mode",
            vec![("Esc", "exit to normal"), ("Ctrl+c", "exit to normal")],
        ),
        _ => (
            "Normal Mode",
            vec![
                ("i", "insert before"),
                ("a", "insert after"),
                ("o", "new line below"),
                ("dd", "delete line"),
                ("yy", "yank line"),
                ("p", "paste"),
                ("/", "search"),
                (":w", "save"),
                (":q", "quit"),
            ],
        ),
    }
}
