//! Render state for RAF-based batching
//! Schedules renders via requestAnimationFrame for smooth 60fps
//! Includes FPS/latency diagnostics
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use crate::renderer::Renderer;

/// Number of frame samples to average for FPS calculation
const FPS_SAMPLE_COUNT: usize = 60;

/// Diagnostics data exposed for display
#[derive(Clone, Default)]
pub struct DiagnosticsData {
    pub fps: f64,
    pub frame_time_ms: f64,
    pub render_count: u64,
    pub dropped_frames: u64,
}

/// Render state for RAF-based batching with diagnostics
pub struct RenderState {
    grids: Rc<RefCell<GridManager>>,
    highlights: Rc<RefCell<HighlightMap>>,
    renderer: Rc<RefCell<Renderer>>,
    needs_render: Rc<RefCell<bool>>,
    raf_scheduled: Rc<RefCell<bool>>,
    // Diagnostics
    frame_times: Rc<RefCell<VecDeque<f64>>>,
    last_frame_time: Rc<RefCell<f64>>,
    render_count: Rc<RefCell<u64>>,
    dropped_frames: Rc<RefCell<u64>>,
    diagnostics_enabled: Rc<RefCell<bool>>,
}

impl RenderState {
    pub fn new(
        grids: Rc<RefCell<GridManager>>,
        highlights: Rc<RefCell<HighlightMap>>,
        renderer: Rc<RefCell<Renderer>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            grids,
            highlights,
            renderer,
            needs_render: Rc::new(RefCell::new(false)),
            raf_scheduled: Rc::new(RefCell::new(false)),
            frame_times: Rc::new(RefCell::new(VecDeque::with_capacity(FPS_SAMPLE_COUNT))),
            last_frame_time: Rc::new(RefCell::new(0.0)),
            render_count: Rc::new(RefCell::new(0)),
            dropped_frames: Rc::new(RefCell::new(0)),
            diagnostics_enabled: Rc::new(RefCell::new(false)),
        })
    }

    /// Mark that a render is needed (will be executed on next RAF)
    pub fn request_render(self: &Rc<Self>) {
        *self.needs_render.borrow_mut() = true;

        if !*self.raf_scheduled.borrow() {
            *self.raf_scheduled.borrow_mut() = true;

            let state = self.clone();
            let callback = Closure::once(Box::new(move || {
                state.do_render();
            }) as Box<dyn FnOnce()>);

            if let Some(window) = web_sys::window() {
                let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
            }
            callback.forget();
        }
    }

    /// Execute the actual render (called from RAF)
    fn do_render(self: &Rc<Self>) {
        *self.raf_scheduled.borrow_mut() = false;

        if *self.needs_render.borrow() {
            *self.needs_render.borrow_mut() = false;

            // Track frame timing
            let now = js_sys::Date::now();
            let last = *self.last_frame_time.borrow();

            if last > 0.0 {
                let frame_time = now - last;
                let mut times = self.frame_times.borrow_mut();
                if times.len() >= FPS_SAMPLE_COUNT {
                    times.pop_front();
                }
                times.push_back(frame_time);

                if frame_time > 20.0 {
                    *self.dropped_frames.borrow_mut() += 1;
                }
            }
            *self.last_frame_time.borrow_mut() = now;
            *self.render_count.borrow_mut() += 1;

            // Do the actual render
            self.render_now();
        }
    }

    /// Force immediate render
    pub fn render_now(&self) {
        let mut renderer = self.renderer.borrow_mut();
        let grids = self.grids.borrow();
        let highlights = self.highlights.borrow();
        renderer.render(&grids, &highlights);
    }

    /// Get diagnostics data
    pub fn get_diagnostics(&self) -> DiagnosticsData {
        let times = self.frame_times.borrow();
        let avg_frame_time = if times.is_empty() {
            0.0
        } else {
            times.iter().sum::<f64>() / times.len() as f64
        };

        DiagnosticsData {
            fps: if avg_frame_time > 0.0 {
                1000.0 / avg_frame_time
            } else {
                0.0
            },
            frame_time_ms: avg_frame_time,
            render_count: *self.render_count.borrow(),
            dropped_frames: *self.dropped_frames.borrow(),
        }
    }

    /// Access the renderer instance
    pub fn renderer(&self) -> &Rc<RefCell<Renderer>> {
        &self.renderer
    }

    /// Enable/disable diagnostics overlay
    #[allow(dead_code)]
    pub fn set_diagnostics_enabled(&self, enabled: bool) {
        *self.diagnostics_enabled.borrow_mut() = enabled;
    }
}
