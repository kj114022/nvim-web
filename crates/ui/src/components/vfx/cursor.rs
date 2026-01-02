use web_sys::CanvasRenderingContext2d;

pub trait CursorVfx {
    fn update(&mut self, dt: f32) -> bool;
    fn render(&self, ctx: &CanvasRenderingContext2d, cell_w: f64, cell_h: f64, color: &str);
    fn restart(&mut self, x: f64, y: f64);
    fn on_move(&mut self, x: f64, y: f64, cell_h: f64, dt: f64);
}
