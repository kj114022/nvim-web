use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};
use wasm_bindgen::{JsCast, JsValue};
use std::rc::Rc;
use std::cell::RefCell;
use crate::grid::{Grid, GridManager};
use crate::highlight::HighlightMap;

// Default colors (Neovim dark theme)
const DEFAULT_BG: &str = "#1a1a1a";
const DEFAULT_FG: &str = "#cccccc";
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

// NOTE: CursorState and ease_out_quad removed - cursor animation handled by draw_all now

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
    // NOTE: cursor_state removed - cursor blinking handled by draw_all now
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
        
        // Use proper line height: font size * line-height factor
        // The ascent+descent gives glyph height, but we need line height for grid calculation
        // Standard line height is ~1.35 for monospace, or use max(ascent+descent, fontsize*1.2)
        let glyph_height = ascent + descent;
        let cell_h = glyph_height.max(14.0 * 1.2); // Ensure minimum line height based on font size

        // Notify JavaScript of cell dimensions (for mouse selection)
        if let Some(win) = window() {
            let _ = js_sys::Reflect::get(&win, &"updateCellSize".into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                .map(|func| {
                    let _ = func.call2(&win, &cell_w.into(), &cell_h.into());
                });
        }

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

    // NOTE: Single-grid draw() removed - replaced by draw_all() for multigrid support

    /// Draw all grids in z-order (for multigrid support)
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    pub fn draw_all(&self, grids: &GridManager, highlights: &HighlightMap) {
        // Only clear canvas if any grid needs full redraw
        if grids.grids_in_order().any(|g| g.dirty_all) {
            self.clear_canvas();
        }
        self.draw_grids(grids, highlights);
        self.draw_cursor(grids);
    }

    /// Clear entire canvas with default background
    #[allow(deprecated)]
    fn clear_canvas(&self) {
        let canvas_width = self.canvas.width() as f64 / self.dpr;
        let canvas_height = self.canvas.height() as f64 / self.dpr;
        self.ctx.set_fill_style(&DEFAULT_BG.into());
        self.ctx.fill_rect(0.0, 0.0, canvas_width, canvas_height);
    }

    /// Draw all grids in z-order
    fn draw_grids(&self, grids: &GridManager, highlights: &HighlightMap) {
        for grid in grids.grids_in_order() {
            self.draw_grid_at_offset(grid, highlights);
        }
    }

    /// Draw cursor on active grid
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    fn draw_cursor(&self, grids: &GridManager) {
        let active_id = grids.active_grid_id();
        if let Some(grid) = grids.get(active_id) {
            // In cmdline mode, cursor at (0,0) is incorrect - skip rendering
            // The cmdline text renders correctly at the bottom without explicit cursor
            if grids.is_cmdline_mode() && grid.cursor_row == 0 && grid.cursor_col == 0 {
                return;
            }
            
            // Include grid offset for split windows
            let offset_x = (grid.col_offset as f64) * self.cell_w;
            let offset_y = (grid.row_offset as f64) * self.cell_h;
            let cursor_x = (grid.cursor_col as f64) * self.cell_w + offset_x;
            let cursor_y = (grid.cursor_row as f64) * self.cell_h + offset_y;
            self.ctx.set_fill_style(&CURSOR_COLOR.into());
            self.ctx.fill_rect(cursor_x, cursor_y, self.cell_w, self.cell_h);
        }
    }

    /// Draw a single grid at its offset position (only dirty cells)
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    fn draw_grid_at_offset(&self, grid: &Grid, highlights: &HighlightMap) {
        let offset_x = (grid.col_offset as f64) * self.cell_w;
        let offset_y = (grid.row_offset as f64) * self.cell_h;

        // Draw floating window background/shadow (only on full redraw)
        if grid.is_float && grid.dirty_all {
            self.draw_float_background(offset_x, offset_y, grid.cols, grid.rows);
        }

        // Draw only dirty cells (or all cells if dirty_all)
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                // Skip clean cells unless full redraw needed
                if !grid.dirty_all && !cell.dirty {
                    continue;
                }
                let x = (col as f64).mul_add(self.cell_w, offset_x);
                let y = (row as f64).mul_add(self.cell_h, offset_y);
                let hl = cell.hl_id.and_then(|id| highlights.get(id));
                self.draw_cell(cell.ch, x, y, hl);
            }
        }
    }

    /// Draw floating window background with shadow
    #[allow(deprecated)]
    #[allow(clippy::cast_precision_loss)]
    fn draw_float_background(&self, offset_x: f64, offset_y: f64, cols: usize, rows: usize) {
        let grid_width = (cols as f64) * self.cell_w;
        let grid_height = (rows as f64) * self.cell_h;
        // Shadow
        self.ctx.set_fill_style(&"rgba(0,0,0,0.3)".into());
        self.ctx.fill_rect(offset_x + 2.0, offset_y + 2.0, grid_width, grid_height);
        // Background
        self.ctx.set_fill_style(&DEFAULT_BG.into());
        self.ctx.fill_rect(offset_x, offset_y, grid_width, grid_height);
    }

    /// Draw a single cell (background and text)
    #[allow(deprecated)]
    fn draw_cell(&self, ch: char, x: f64, y: f64, hl: Option<&crate::highlight::HighlightAttr>) {
        // Always clear background first (needed for incremental updates)
        let bg_css = hl.and_then(|h| h.bg).map_or_else(
            || DEFAULT_BG.to_string(),
            rgb_to_css
        );
        self.ctx.set_fill_style(&JsValue::from_str(&bg_css));
        self.ctx.fill_rect(x, y, self.cell_w, self.cell_h);

        // Text
        if ch != ' ' {
            let fg_css = hl.and_then(|h| h.fg).map_or_else(
                || DEFAULT_FG.to_string(),
                rgb_to_css
            );
            self.ctx.set_fill_style(&JsValue::from_str(&fg_css));

            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let _ = self.ctx.fill_text(s, x, y + self.ascent);
        }
    }
}

