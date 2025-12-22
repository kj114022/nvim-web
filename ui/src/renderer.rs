use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};
use wasm_bindgen::{JsCast, JsValue};
use std::rc::Rc;
use std::cell::RefCell;
use crate::grid::Grid;
use crate::highlight::HighlightMap;

// Default colors (Neovim dark theme)
const DEFAULT_BG: &str = "#1a1a1a";
const DEFAULT_FG: &str = "#cccccc";
// Selection color: Windows-style blue with alpha
const SELECTION_COLOR: &str = "rgba(0, 120, 215, 0.35)";
// Focus overlay: subtle dim
const FOCUS_LOST_OVERLAY: &str = "rgba(0, 0, 0, 0.08)";
// Cursor color
const CURSOR_COLOR: &str = "#ff6600";

/// Convert RGB u32 to CSS string
fn rgb_to_css(rgb: u32) -> String {
    format!(
        "rgb({},{},{})",
        (rgb >> 16) & 0xff,
        (rgb >> 8) & 0xff,
        rgb & 0xff
    )
}

#[derive(Clone)]
pub struct Renderer {
    canvas: Rc<HtmlCanvasElement>,
    ctx: Rc<CanvasRenderingContext2d>,
    cell_w: f64,
    cell_h: f64,
    ascent: f64,
    dpr: f64,
    // Color caches to avoid per-cell allocations
    cached_fg: Rc<RefCell<Option<(u32, String)>>>,
    cached_bg: Rc<RefCell<Option<(u32, String)>>>,
}

impl Renderer {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        let ctx = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()
            .unwrap();

        // Get device pixel ratio for HiDPI
        let dpr = window().unwrap().device_pixel_ratio();

        // Set font and measure metrics ONCE
        ctx.set_font("14px monospace");
        let metrics = ctx.measure_text("M").unwrap();
        
        // Derive cell dimensions from actual font metrics
        let cell_w = metrics.width();
        let ascent = metrics.actual_bounding_box_ascent();
        let descent = metrics.actual_bounding_box_descent();
        let cell_h = ascent + descent;

        Self {
            canvas: Rc::new(canvas),
            ctx: Rc::new(ctx),
            cell_w,
            cell_h,
            ascent,
            dpr,
            cached_fg: Rc::new(RefCell::new(None)),
            cached_bg: Rc::new(RefCell::new(None)),
        }
    }

    /// Get cell dimensions for row/col calculation
    pub fn cell_size(&self) -> (f64, f64) {
        (self.cell_w, self.cell_h)
    }

    /// Handle resize with HiDPI-correct canvas scaling
    /// Returns (rows, cols) for the new size
    pub fn resize(&self, css_width: f64, css_height: f64) -> (usize, usize) {
        // D2: HiDPI correctness
        let backing_width = (css_width * self.dpr) as u32;
        let backing_height = (css_height * self.dpr) as u32;

        // Resize backing canvas
        self.canvas.set_width(backing_width);
        self.canvas.set_height(backing_height);

        // Reset transform before scaling (prevents compound scaling)
        let _ = self.ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

        // Scale drawing space for HiDPI
        let _ = self.ctx.scale(self.dpr, self.dpr);

        // Re-apply font after transform reset
        self.ctx.set_font("14px monospace");

        // D1.2: Compute grid size (always floor)
        let cols = (css_width / self.cell_w).floor() as usize;
        let rows = (css_height / self.cell_h).floor() as usize;

        (rows.max(1), cols.max(1))
    }

    #[allow(deprecated)]  // web-sys set_fill_style deprecation is overzealous
    pub fn draw(&self, grid: &Grid, highlights: &HighlightMap) {
        let _grid_width = (grid.cols as f64) * self.cell_w;
        let _grid_height = (grid.rows as f64) * self.cell_h;
        
        // Get actual canvas dimensions (CSS pixels, accounting for transform)
        let canvas_width = self.canvas.width() as f64 / self.dpr;
        let canvas_height = self.canvas.height() as f64 / self.dpr;

        // Step 1: Clear entire canvas with default background
        // This prevents artifacts outside the grid area
        self.ctx.set_fill_style(&DEFAULT_BG.into());
        self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);

        // Step 2: Per-cell background and text rendering
        // We batch by going through all cells, drawing bg then text per-cell
        // This is less optimal than pure batching but necessary for per-cell colors
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                let x = (col as f64) * self.cell_w;
                let y = (row as f64) * self.cell_h;
                
                // Get highlight attributes if present
                let hl = cell.hl_id.and_then(|id| highlights.get(id));
                
                // Draw background if different from default
                if let Some(hl) = hl {
                    if let Some(bg) = hl.bg {
                        let bg_css = {
                            let mut cache = self.cached_bg.borrow_mut();
                            if let Some((cached, ref css)) = *cache {
                                if cached == bg {
                                    css.clone()
                                } else {
                                    let css = rgb_to_css(bg);
                                    *cache = Some((bg, css.clone()));
                                    css
                                }
                            } else {
                                let css = rgb_to_css(bg);
                                *cache = Some((bg, css.clone()));
                                css
                            }
                        };
                        self.ctx.set_fill_style(&bg_css.into());
                        self.ctx.fill_rect(x, y, self.cell_w, self.cell_h);
                    }
                }
                
                // Draw selection overlay if selected
                if cell.selected {
                    self.ctx.set_fill_style(&SELECTION_COLOR.into());
                    self.ctx.fill_rect(x, y, self.cell_w, self.cell_h);
                }
                
                // Draw text if not space
                if cell.ch != ' ' {
                    // Get foreground color
                    let fg_css = if let Some(hl) = hl {
                        if let Some(fg) = hl.fg {
                            let mut cache = self.cached_fg.borrow_mut();
                            if let Some((cached, ref css)) = *cache {
                                if cached == fg {
                                    css.clone()
                                } else {
                                    let css = rgb_to_css(fg);
                                    *cache = Some((fg, css.clone()));
                                    css
                                }
                            } else {
                                let css = rgb_to_css(fg);
                                *cache = Some((fg, css.clone()));
                                css
                            }
                        } else {
                            DEFAULT_FG.to_string()
                        }
                    } else {
                        DEFAULT_FG.to_string()
                    };
                    
                    // Apply text styles (bold/italic)
                    let bold = hl.map_or(false, |h| h.bold);
                    let italic = hl.map_or(false, |h| h.italic);
                    let underline = hl.map_or(false, |h| h.underline);
                    
                    let font = match (bold, italic) {
                        (true, true) => "bold italic 14px monospace",
                        (true, false) => "bold 14px monospace",
                        (false, true) => "italic 14px monospace",
                        (false, false) => "14px monospace",
                    };
                    self.ctx.set_font(font);
                    
                    self.ctx.set_fill_style(&JsValue::from_str(&fg_css));
                    
                    // Draw character
                    let mut buf = [0u8; 4];
                    let s = cell.ch.encode_utf8(&mut buf);
                    let _ = self.ctx.fill_text(s, x, y + self.ascent);
                    
                    // Draw underline if needed
                    if underline {
                        self.ctx.set_stroke_style(&JsValue::from_str(&fg_css));
                        self.ctx.begin_path();
                        self.ctx.move_to(x, y + self.ascent + 2.0);
                        self.ctx.line_to(x + self.cell_w, y + self.ascent + 2.0);
                        self.ctx.stroke();
                    }
                    
                    // Reset font to default
                    if bold || italic {
                        self.ctx.set_font("14px monospace");
                    }
                }
            }
        }

        // Step 3: Cursor (on top of everything except focus overlay)
        self.ctx.set_fill_style(&CURSOR_COLOR.into());
        self.ctx.fill_rect(
            (grid.cursor_col as f64) * self.cell_w,
            (grid.cursor_row as f64) * self.cell_h,
            self.cell_w,
            self.cell_h,
        );

        // Step 4: Focus overlay (after all drawing, if unfocused)
        if !grid.is_focused {
            self.ctx.set_fill_style(&FOCUS_LOST_OVERLAY.into());
            self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);
        }
    }
}
