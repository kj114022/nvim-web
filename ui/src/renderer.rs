use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};
use wasm_bindgen::JsCast;
use std::rc::Rc;
use crate::grid::Grid;

// Selection color: Windows-style blue with alpha
const SELECTION_COLOR: &str = "rgba(0, 120, 215, 0.35)";
// Focus overlay: subtle dim
const FOCUS_LOST_OVERLAY: &str = "rgba(0, 0, 0, 0.08)";
// Cached style strings to avoid allocations
const BG_COLOR: &str = "white";
const TEXT_COLOR: &str = "black";
const CURSOR_COLOR: &str = "#000000";

#[derive(Clone)]
pub struct Renderer {
    canvas: Rc<HtmlCanvasElement>,
    ctx: Rc<CanvasRenderingContext2d>,
    cell_w: f64,
    cell_h: f64,
    ascent: f64,
    dpr: f64,
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
    pub fn draw(&self, grid: &Grid) {
        let css_width = (grid.cols as f64) * self.cell_w;
        let css_height = (grid.rows as f64) * self.cell_h;

        // Step 1: Background fills entire canvas (no need for clear_rect - overdraw)
        self.ctx.set_fill_style(&BG_COLOR.into());
        self.ctx.fill_rect(0.0, 0.0, css_width, css_height);

        // Step 2: Selection backgrounds (set style once, draw all)
        self.ctx.set_fill_style(&SELECTION_COLOR.into());
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                if cell.selected {
                    let x = (col as f64) * self.cell_w;
                    let y = (row as f64) * self.cell_h;
                    self.ctx.fill_rect(x, y, self.cell_w, self.cell_h);
                }
            }
        }

        // Step 3: Text (set style once, draw all non-space chars)
        self.ctx.set_fill_style(&TEXT_COLOR.into());
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                if cell.ch != ' ' {
                    // Use encode_utf8 with let-bound buffer to avoid String allocation
                    let mut buf = [0u8; 4];
                    let s = cell.ch.encode_utf8(&mut buf);
                    let _ = self.ctx.fill_text(
                        s,
                        (col as f64) * self.cell_w,
                        (row as f64) * self.cell_h + self.ascent,
                    );
                }
            }
        }

        // Step 4: Cursor (on top of everything except focus overlay)
        self.ctx.set_fill_style(&CURSOR_COLOR.into());
        self.ctx.fill_rect(
            (grid.cursor_col as f64) * self.cell_w,
            (grid.cursor_row as f64) * self.cell_h,
            self.cell_w,
            self.cell_h,
        );

        // Step 5: Focus overlay (after all drawing, if unfocused)
        if !grid.is_focused {
            self.ctx.set_fill_style(&FOCUS_LOST_OVERLAY.into());
            self.ctx.fill_rect(0.0, 0.0, css_width, css_height);
        }
    }
}
