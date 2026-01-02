use super::cursor::CursorVfx;
use web_sys::CanvasRenderingContext2d;
use std::f64::consts::PI;

#[derive(Clone, Debug, PartialEq)]
pub enum TrailMode {
    Railgun,
    Torpedo,
    PixieDust,
}

#[derive(Clone)]
struct ParticleData {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    rotation_speed: f64,
    lifetime: f64,
    max_lifetime: f64,
}

pub struct ParticleTrail {
    particles: Vec<ParticleData>,
    prev_x: f64,
    prev_y: f64,
    mode: TrailMode,
    rng: RngState,
    count_reminder: f64,
}

impl ParticleTrail {
    pub fn new(mode: TrailMode) -> Self {
        Self {
            particles: Vec::new(),
            prev_x: 0.0,
            prev_y: 0.0,
            mode,
            rng: RngState::new(),
            count_reminder: 0.0,
        }
    }

    fn add_particle(&mut self, x: f64, y: f64, vx: f64, vy: f64, rotation_speed: f64, lifetime: f64) {
        self.particles.push(ParticleData {
            x,
            y,
            vx,
            vy,
            rotation_speed,
            lifetime,
            max_lifetime: lifetime,
        });
    }
}

impl CursorVfx for ParticleTrail {
    fn restart(&mut self, x: f64, y: f64) {
        self.prev_x = x;
        self.prev_y = y;
        self.count_reminder = 0.0;
    }

    fn update(&mut self, dt: f32) -> bool {
        let dt = dt as f64;
        
        // Update lifetimes
        let mut i = 0;
        while i < self.particles.len() {
            if self.particles[i].lifetime <= 0.0 {
                self.particles.swap_remove(i);
            } else {
                self.particles[i].lifetime -= dt;
                i += 1;
            }
        }

        // Update positions
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            // Rotate velocity vector
            let (sin, cos) = (dt * p.rotation_speed).sin_cos();
            let new_vx = p.vx * cos - p.vy * sin;
            let new_vy = p.vx * sin + p.vy * cos;
            p.vx = new_vx;
            p.vy = new_vy;
        }

        !self.particles.is_empty()
    }

    // Since we handle spawning in a separate step or we can combine it. 
    // Neovide combines update and spawn. We need to mirror that if we have current_cursor info.
    // For this ported trait, we might need to pass cursor pos to update.
    // But for now, let's strictly follow the trait we defined: update(dt).
    // wait, we defined `restart(x,y)` but we probably need `spawn(x,y)` or `on_move(x,y)`.
    // Let's assume the caller will handle movement detection or we update the trait.
    // Actually, Neovide passes `current_cursor_destination` to `update`.
    // Let's stick to the file for now and address the trait mismatch in a fix-up if needed.
    // I will add a `on_move` method to the implementation, even if not in the trait yet.
    
    #[allow(deprecated)]
    fn render(&self, ctx: &CanvasRenderingContext2d, cell_w: f64, _cell_h: f64, color: &str) {
        let _ = ctx.save();
        ctx.set_fill_style(&color.into());
        ctx.set_stroke_style(&color.into());
        
        for p in &self.particles {
            let life_pct = p.lifetime / p.max_lifetime;
            let alpha = life_pct * 0.5; // Base opacity 0.5
            ctx.set_global_alpha(alpha);

            let size = match self.mode {
                TrailMode::Torpedo | TrailMode::Railgun => cell_w * 0.5 * life_pct,
                TrailMode::PixieDust => cell_w * 0.2,
            };

            let _ = ctx.begin_path();
            match self.mode {
                TrailMode::Torpedo | TrailMode::Railgun => {
                     // Oval/Circle
                     let _ = ctx.arc(p.x, p.y, size, 0.0, 2.0 * PI);
                     let _ = ctx.fill();
                }
                TrailMode::PixieDust => {
                    // Rect
                    ctx.fill_rect(p.x - size/2.0, p.y - size/2.0, size, size);
                }
            }
        }
        let _ = ctx.restore();
    }

    fn on_move(&mut self, target_x: f64, target_y: f64, cell_h: f64, _dt: f64) {
        if (target_x - self.prev_x).abs() < 0.1 && (target_y - self.prev_y).abs() < 0.1 {
            return;
        }

        let dx = target_x - self.prev_x;
        let dy = target_y - self.prev_y;
        let dist = (dx * dx + dy * dy).sqrt();
        
        let density = 20.0; // configurable?
        let f_count = (dist / cell_h) * density + self.count_reminder;
        let count = f_count as usize;
        self.count_reminder = f_count - count as f64;

        for i in 0..count {
            let t = (i as f64 + 1.0) / (count as f64);
            
            // Interpolated position
            let ix = self.prev_x + dx * t;
            let iy = self.prev_y + dy * t;

            // Randomized launch params
            let (vx, vy, rot, life) = match self.mode {
                TrailMode::Railgun => {
                    let phase = t / PI * 1.5 * (dist / cell_h);
                    let (s, c) = phase.sin_cos();
                    (s * 200.0, c * 200.0, PI, 0.5)
                }
                TrailMode::Torpedo => {
                    let dir_x = dx / dist;
                    let dir_y = dy / dist;
                    let rand_dir = self.rng.rand_dir_normalized();
                    let px = rand_dir.0 - dir_x * 1.5;
                    let py = rand_dir.1 - dir_y * 1.5;
                    
                    // Normalize px, py
                    let p_len = (px*px + py*py).sqrt();
                    (px/p_len * 200.0, py/p_len * 200.0, 
                     (self.rng.next_f64() - 0.5) * PI, 0.8)
                }
                 TrailMode::PixieDust => {
                    let (bx, by) = self.rng.rand_dir_normalized();
                    let vx = bx * 0.5;
                    let vy = 0.4 + by.abs();
                    (vx * 300.0, vy * 300.0, 
                     (self.rng.next_f64() - 0.5) * PI, 0.6)
                }
            };

             // Initial position jitter
            let (jx, jy) = match self.mode {
                TrailMode::Railgun => (ix, iy),
                _ => (ix + dx * self.rng.next_f64(), iy + dy * self.rng.next_f64())
            };

            self.add_particle(jx, jy, vx, vy, rot, life * t); // simple life distribution
        }

        self.prev_x = target_x;
        self.prev_y = target_y;
    }
}

// PCG Random
struct RngState {
    state: u64,
    inc: u64,
}

impl RngState {
    fn new() -> Self {
        Self {
            state: 0x853C_49E6_748F_EA9B,
            inc: (0xDA3E_39CB_94B9_5BDB << 1) | 1,
        }
    }

    fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    fn next_f64(&mut self) -> f64 {
        let v = self.next();
        (v as f64) / (u32::MAX as f64)
    }
    
    fn rand_dir_normalized(&mut self) -> (f64, f64) {
        let theta = self.next_f64() * 2.0 * PI;
        theta.sin_cos()
    }
}
