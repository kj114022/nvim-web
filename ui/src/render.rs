//! Render state for RAF-based batching
//! Schedules renders via requestAnimationFrame for smooth 60fps

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::window;

use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use crate::renderer::Renderer;

/// Render state for RAF-based batching
pub struct RenderState {
    grids: Rc<RefCell<GridManager>>,
    highlights: Rc<RefCell<HighlightMap>>,
    renderer: Rc<Renderer>,
    needs_render: Rc<RefCell<bool>>,
    raf_scheduled: Rc<RefCell<bool>>,
}

impl RenderState {
    pub fn new(
        grids: Rc<RefCell<GridManager>>,
        highlights: Rc<RefCell<HighlightMap>>,
        renderer: Rc<Renderer>,
    ) -> Rc<Self> {
        Rc::new(Self {
            grids,
            highlights,
            renderer,
            needs_render: Rc::new(RefCell::new(false)),
            raf_scheduled: Rc::new(RefCell::new(false)),
        })
    }

    /// Mark that a render is needed and schedule RAF if not already scheduled
    pub fn request_render(self: &Rc<Self>) {
        *self.needs_render.borrow_mut() = true;
        
        if !*self.raf_scheduled.borrow() {
            *self.raf_scheduled.borrow_mut() = true;
            
            let state = self.clone();
            let callback = Closure::once(Box::new(move || {
                state.do_render();
            }) as Box<dyn FnOnce()>);
            
            let _ = window().unwrap().request_animation_frame(
                callback.as_ref().unchecked_ref()
            );
            callback.forget();
        }
    }

    /// Execute the actual render (called from RAF)
    fn do_render(&self) {
        *self.raf_scheduled.borrow_mut() = false;
        
        if *self.needs_render.borrow() {
            *self.needs_render.borrow_mut() = false;
            self.renderer.draw_all(&self.grids.borrow(), &self.highlights.borrow());
        }
    }

    /// Force immediate render (for resize, focus changes)
    pub fn render_now(&self) {
        *self.needs_render.borrow_mut() = false;
        self.renderer.draw_all(&self.grids.borrow(), &self.highlights.borrow());
    }
}
