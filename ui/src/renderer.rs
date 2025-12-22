use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};
use wasm_bindgen::JsCast;
use std::rc::Rc;
use std::cell::RefCell;
use crate::grid::Grid;

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

        let renderer = Self {
            canvas: Rc::new(canvas),
            ctx: Rc::new(ctx),
            cell_w,
            cell_h,
            ascent,
            dpr,
        };

        renderer
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

    pub fn draw(&self, grid: &Grid) {
        let css_width = (grid.cols as f64) * self.cell_w;
        let css_height = (grid.rows as f64) * self.cell_h;

        // D1.3: Clear entire canvas before redraw
        self.ctx.clear_rect(0.0, 0.0, css_width, css_height);

        // Set background color
        self.ctx.set_fill_style(&"white".into());
        self.ctx.fill_rect(0.0, 0.0, css_width, css_height);

        // Set text color to black
        self.ctx.set_fill_style(&"black".into());

        // Draw grid cells
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = &grid.cells[row * grid.cols + col];
                if cell.ch != ' ' {  // Only draw non-space characters
                    // Baseline-correct: y = row * cell_h + ascent
                    self.ctx.fill_text(
                        &cell.ch.to_string(),
                        (col as f64) * self.cell_w,
                        (row as f64) * self.cell_h + self.ascent,
                    ).unwrap();
                }
            }
        }

        // Draw cursor - filled rectangle with inverted color for visibility
        // On white bg -> black cursor, on dark bg -> white cursor
        self.ctx.set_fill_style(&"#000000".into());
        self.ctx.fill_rect(
            (grid.cursor_col as f64) * self.cell_w,
            (grid.cursor_row as f64) * self.cell_h,
            self.cell_w,
            self.cell_h,
        );
    }
}
