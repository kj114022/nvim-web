use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
use wasm_bindgen::JsCast;
use std::rc::Rc;
use crate::grid::Grid;

#[derive(Clone)]
pub struct Renderer {
    ctx: Rc<CanvasRenderingContext2d>,
    cell_w: f64,
    cell_h: f64,
    ascent: f64,
}

impl Renderer {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        let ctx = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()
            .unwrap();

        // Set font and measure metrics ONCE
        ctx.set_font("14px monospace");
        let metrics = ctx.measure_text("M").unwrap();
        
        // Derive cell dimensions from actual font metrics
        let cell_w = metrics.width();
        let ascent = metrics.actual_bounding_box_ascent();
        let descent = metrics.actual_bounding_box_descent();
        let cell_h = ascent + descent;

        Self {
            ctx: Rc::new(ctx),
            cell_w,
            cell_h,
            ascent,
        }
    }

    pub fn draw(&self, grid: &Grid) {
        // Set background color
        self.ctx.set_fill_style(&"white".into());
        self.ctx.fill_rect(
            0.0,
            0.0,
            (grid.cols as f64) * self.cell_w,
            (grid.rows as f64) * self.cell_h,
        );

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

        // Draw cursor in red for visibility
        self.ctx.set_stroke_style(&"red".into());
        self.ctx.stroke_rect(
            (grid.cursor_col as f64) * self.cell_w,
            (grid.cursor_row as f64) * self.cell_h,
            self.cell_w,
            self.cell_h,
        );
    }
}
