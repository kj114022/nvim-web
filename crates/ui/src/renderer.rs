use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};
use wasm_bindgen::{JsCast, JsValue};
use std::rc::Rc;
use std::cell::RefCell;
use crate::grid::{Grid, GridManager};
use crate::highlight::HighlightMap;

// Default colors (Neovim dark theme)
const DEFAULT_BG: &str = "#1a1a1a";
const DEFAULT_FG: &str = "#cccccc";
// Selection color: Windows-style blue with alpha
#[allow(dead_code)]
const SELECTION_COLOR: &str = "rgba(0, 120, 215, 0.35)";
// Focus overlay: subtle dim
#[allow(dead_code)]
const FOCUS_LOST_OVERLAY: &str = "rgba(0, 0, 0, 0.08)";
// Cursor color
const CURSOR_COLOR: &str = "#ff6600";

// Cursor animation constants
const CURSOR_BLINK_ON_MS: f64 = 530.0;
const CURSOR_BLINK_OFF_MS: f64 = 530.0;
const CURSOR_ANIMATION_DURATION_MS: f64 = 80.0;

/// Convert RGB u32 to CSS string
fn rgb_to_css(rgb: u32) -> String {
    format!(
        "rgb({},{},{})",
        (rgb >> 16) & 0xff,
        (rgb >> 8) & 0xff,
        rgb & 0xff
    )
}

/// Cursor animation state for smooth movement and blinking
#[derive(Clone)]
struct CursorState {
    // Target position (where Neovim says cursor is)
    target_col: f64,
    target_row: f64,
    // Current animated position
    current_col: f64,
    current_row: f64,
    // Animation timing
    move_start_time: f64,
    move_start_col: f64,
    move_start_row: f64,
    // Blink state
    last_move_time: f64,
    blink_visible: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            target_col: 0.0,
            target_row: 0.0,
            current_col: 0.0,
            current_row: 0.0,
            move_start_time: 0.0,
            move_start_col: 0.0,
            move_start_row: 0.0,
            last_move_time: 0.0,
            blink_visible: true,
        }
    }
}

impl CursorState {
    /// Update cursor target position, trigger animation if moved
    fn set_target(&mut self, col: usize, row: usize, now: f64) {
        let new_col = col as f64;
        let new_row = row as f64;
        
        // Only trigger animation if position changed
        if (new_col - self.target_col).abs() > 0.01 || (new_row - self.target_row).abs() > 0.01 {
            self.move_start_time = now;
            self.move_start_col = self.current_col;
            self.move_start_row = self.current_row;
            self.target_col = new_col;
            self.target_row = new_row;
            self.last_move_time = now;
            self.blink_visible = true; // Reset blink on move
        }
    }
    
    /// Update animation state, returns (col, row, visible)
    fn update(&mut self, now: f64) -> (f64, f64, bool) {
        // Smooth position interpolation
        let elapsed = now - self.move_start_time;
        let t = (elapsed / CURSOR_ANIMATION_DURATION_MS).min(1.0);
        let ease_t = ease_out_quad(t);
        
        self.current_col = self.move_start_col + (self.target_col - self.move_start_col) * ease_t;
        self.current_row = self.move_start_row + (self.target_row - self.move_start_row) * ease_t;
        
        // Blink logic (only after cursor has been still for a moment)
        let time_since_move = now - self.last_move_time;
        if time_since_move > 300.0 {
            // Start blinking after 300ms of stillness
            let blink_cycle = CURSOR_BLINK_ON_MS + CURSOR_BLINK_OFF_MS;
            let cycle_pos = (time_since_move - 300.0) % blink_cycle;
            self.blink_visible = cycle_pos < CURSOR_BLINK_ON_MS;
        } else {
            self.blink_visible = true;
        }
        
        (self.current_col, self.current_row, self.blink_visible)
    }
}

/// Ease-out quadratic for smooth cursor animation
fn ease_out_quad(t: f64) -> f64 {
    1.0 - (1.0 - t) * (1.0 - t)
}

#[derive(Clone)]
#[allow(dead_code)]
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
    // Cursor animation state
    cursor_state: Rc<RefCell<CursorState>>,
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
            cursor_state: Rc::new(RefCell::new(CursorState::default())),
        }
    }

    /// Get cell dimensions for row/col calculation
    pub const fn cell_size(&self) -> (f64, f64) {
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
    #[allow(dead_code)]
    #[allow(clippy::cast_precision_loss)] // Canvas API requires f64, generic logic uses f64, but some sources are u32
    pub fn draw(&self, grid: &Grid, highlights: &HighlightMap) {
        let _ = (grid.cols as f64) * self.cell_w;
        let _ = (grid.rows as f64) * self.cell_h;
        
        // Get actual canvas dimensions (CSS pixels, accounting for transform)
        let canvas_width = self.canvas.width() as f64 / self.dpr;
        let canvas_height = self.canvas.height() as f64 / self.dpr;

        // Clear entire canvas with default background
        // This prevents artifacts outside the grid area
        self.ctx.set_fill_style(&DEFAULT_BG.into());
        self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);

        // Per-cell background and text rendering
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
                    let bold = hl.is_some_and(|h| h.bold);
                    let italic = hl.is_some_and(|h| h.italic);
                    let underline = hl.is_some_and(|h| h.underline);
                    
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

        // Animated cursor (on top of everything except focus overlay)
        // Use js_sys::Date::now() for timing (always available in WASM)
        let now = js_sys::Date::now();
        
        // Update cursor target and get animated state
        {
            let mut cursor = self.cursor_state.borrow_mut();
            cursor.set_target(grid.cursor_col, grid.cursor_row, now);
        }
        let (cursor_col, cursor_row, cursor_visible) = self.cursor_state.borrow_mut().update(now);
        
        // Draw cursor if visible (blink on)
        if cursor_visible {
            self.ctx.set_fill_style(&CURSOR_COLOR.into());
            self.ctx.fill_rect(
                cursor_col * self.cell_w,
                cursor_row * self.cell_h,
                self.cell_w,
                self.cell_h,
            );
        }

        // Focus overlay (after all drawing, if unfocused)
        if !grid.is_focused {
            self.ctx.set_fill_style(&FOCUS_LOST_OVERLAY.into());
            self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);
        }
    }

    /// Draw all grids in z-order (for multigrid support)
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    pub fn draw_all(&self, grids: &GridManager, highlights: &HighlightMap) {
        let canvas_width = self.canvas.width() as f64 / self.dpr;
        let canvas_height = self.canvas.height() as f64 / self.dpr;

        // Clear entire canvas
        self.ctx.set_fill_style(&DEFAULT_BG.into());
        self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);

        // Simple single-grid mode: just draw all visible grids
        // With multigrid disabled, only Grid 1 will exist
        for grid in grids.grids_in_order() {
            self.draw_grid_at_offset(grid, highlights);
        }

        // Draw cursor on active grid
        let active_id = grids.active_grid_id();
        if let Some(grid) = grids.get(active_id) {
            let cursor_x = (grid.cursor_col as f64) * self.cell_w;
            let cursor_y = (grid.cursor_row as f64) * self.cell_h;
            self.ctx.set_fill_style(&CURSOR_COLOR.into());
            self.ctx.fill_rect(cursor_x, cursor_y, self.cell_w, self.cell_h);
        }
    }

    /// Draw a single grid at its offset position
    /// Simplified for single-grid mode (multigrid disabled)
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    fn draw_grid_at_offset(&self, grid: &Grid, highlights: &HighlightMap) {
        // In single-grid mode, just draw at (0,0)
        // Grid offsets are only used when multigrid is enabled
        let offset_x = (grid.col_offset as f64) * self.cell_w;
        let offset_y = (grid.row_offset as f64) * self.cell_h;

        // Draw optional background/border for floating windows
        if grid.is_float {
            let grid_width = (grid.cols as f64) * self.cell_w;
            let grid_height = (grid.rows as f64) * self.cell_h;
            // Shadow/border for float
            self.ctx.set_fill_style(&"rgba(0,0,0,0.3)".into());
            self.ctx.fill_rect(offset_x + 2.0, offset_y + 2.0, grid_width, grid_height);
            // Background
            self.ctx.set_fill_style(&DEFAULT_BG.into());
            self.ctx.fill_rect(offset_x, offset_y, grid_width, grid_height);
        }

        // Draw all cells
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                let x = (col as f64).mul_add(self.cell_w, offset_x);
                let y = (row as f64).mul_add(self.cell_h, offset_y);
                
                let hl = cell.hl_id.and_then(|id| highlights.get(id));
                
                // Background
                if let Some(hl) = hl {
                    if let Some(bg) = hl.bg {
                        self.ctx.set_fill_style(&JsValue::from_str(&rgb_to_css(bg)));
                        self.ctx.fill_rect(x, y, self.cell_w, self.cell_h);
                    }
                }
                
                // Text
                if cell.ch != ' ' {
                    let fg_css = hl.and_then(|h| h.fg).map_or_else(
                        || DEFAULT_FG.to_string(),
                        rgb_to_css
                    );
                    self.ctx.set_fill_style(&JsValue::from_str(&fg_css));
                    
                    let mut buf = [0u8; 4];
                    let s = cell.ch.encode_utf8(&mut buf);
                    let _ = self.ctx.fill_text(s, x, y + self.ascent);
                }
            }
        }
    }
}
