#![allow(dead_code)]
use crate::grid::GridManager;
use crate::highlight::HighlightMap;
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use web_sys::{OffscreenCanvas, OffscreenCanvasRenderingContext2d};

// --- Constants ---
const ATLAS_SIZE: u32 = 1024; // 1024x1024 texture atlas

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GlobalUniforms {
    resolution: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct InstanceInput {
    pos_min: [f32; 2],
    size: [f32; 2],
    atlas_uv_min: [f32; 2],
    atlas_uv_max: [f32; 2],
    bg_color: [f32; 4],
    fg_color: [f32; 4],
    /// Style flags: bit 0 = bold, bit 1 = italic, bit 2 = underline, bit 3 = strikethrough
    style_flags: u32,
    _padding: [u32; 3], // Padding for 16-byte alignment
}

struct WebGpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,

    instance_buffer: wgpu::Buffer,
    current_instance_capacity: usize,
    uniform_buffer: wgpu::Buffer,

    atlas_texture: wgpu::Texture,
    current_atlas_size: u32,
    /// Glyph cache: (char, bold, italic) -> (uv_min, uv_max)
    atlas_map: HashMap<(char, bool, bool), ([f32; 2], [f32; 2])>,
    next_atlas_pos: (u32, u32),
    max_line_height: u32,

    // Canvas2D for glyph rasterization (used to populate atlas)
    glyph_canvas: OffscreenCanvas,
    glyph_ctx: OffscreenCanvasRenderingContext2d,
}

struct CanvasBackend {
    ctx: OffscreenCanvasRenderingContext2d,
    width: u32,
    height: u32,
}

enum Backend {
    WebGpu(WebGpuBackend),
    Canvas(CanvasBackend),
}

/// Cursor shape based on Neovim mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorShape {
    Block,     // Normal mode
    Beam,      // Insert mode (vertical line)
    Underline, // Replace mode
}

impl Default for CursorShape {
    fn default() -> Self {
        Self::Block
    }
}

/// Cursor state for blink animation
#[derive(Debug, Clone)]
pub struct CursorState {
    pub visible: bool,
    pub last_toggle: f64,
    pub blink_ms: f64,
    pub shape: CursorShape,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            visible: true,
            last_toggle: 0.0,
            blink_ms: 530.0, // Standard blink interval
            shape: CursorShape::Block,
        }
    }
}

pub struct Renderer {
    backend: Backend,
    size: (u32, u32),

    // Cursor blink state
    pub cursor: CursorState,

    // Cached metrics (Shared)
    pub cell_w: f64,
    pub cell_h: f64,
    pub font_size: f64,
    pub font_family: String,
    pub dpr: f64,
}

impl Renderer {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn new(_canvas: OffscreenCanvas, _dpr: f64, _force_canvas: bool) -> Self {
        panic!("Renderer is WASM-only")
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn new(canvas: OffscreenCanvas, dpr: f64, force_canvas: bool) -> Self {
        let width = canvas.width();
        let height = canvas.height();
        let size = (width, height);

        // Check if user has disabled GPU acceleration
        if force_canvas {
            web_sys::console::log_1(
                &"Renderer: GPU acceleration disabled by user. Using Canvas2D.".into(),
            );
            return Self::create_canvas_renderer(canvas, size, dpr);
        }

        // Try to initialize WebGPU
        match Self::init_webgpu(canvas.clone(), width, height).await {
            Ok(backend) => {
                web_sys::console::log_1(&"Renderer: Initialized WebGPU backend".into());
                let mut renderer = Self {
                    backend: Backend::WebGpu(backend),
                    size,
                    dpr,
                    cursor: CursorState::default(),
                    cell_w: 10.0,
                    cell_h: 20.0,
                    font_size: 14.0,
                    font_family: "monospace".to_string(),
                };
                renderer.update_font_metric();
                renderer
            }
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("WebGPU init failed: {}. Falling back to Canvas2D.", e).into(),
                );
                Self::create_canvas_renderer(canvas, size, dpr)
            }
        }
    }

    /// Create a Canvas2D-based renderer (fallback or user-requested)
    #[cfg(target_arch = "wasm32")]
    fn create_canvas_renderer(canvas: OffscreenCanvas, size: (u32, u32), dpr: f64) -> Self {
        let ctx = canvas
            .get_context("2d")
            .expect("Failed to get 2d context")
            .expect("No 2d context")
            .dyn_into::<OffscreenCanvasRenderingContext2d>()
            .expect("Failed to cast to OffscreenCanvasRenderingContext2d");

        let mut renderer = Self {
            backend: Backend::Canvas(CanvasBackend {
                ctx,
                width: size.0,
                height: size.1,
            }),
            size,
            dpr,
            cursor: CursorState::default(),
            cell_w: 10.0,
            cell_h: 20.0,
            font_size: 14.0,
            font_family: "monospace".to_string(),
        };
        renderer.update_font_metric();
        renderer
    }

    #[cfg(target_arch = "wasm32")]
    async fn init_webgpu(
        canvas: OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<WebGpuBackend, String> {
        let instance = wgpu::Instance::default();

        // Create Surface via unsafe cast to HtmlCanvasElement
        let canvas_cast: web_sys::HtmlCanvasElement = canvas.clone().unchecked_into();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas_cast))
            .map_err(|e| format!("Failed to create surface: {}", e))?;

        // Request Adapter
        let mut adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await;

        if adapter.is_none() {
            adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false, // Don't allow software fallback yet, check explicit GL/WebGPU
                })
                .await;
        }

        let adapter = adapter.ok_or("No suitable WGPU adapter found")?;

        // Detect backend type and log
        let backend_name = match adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL",
            _ => "Unknown",
        };
        web_sys::console::log_1(&format!("WGPU Adapter found: {}", backend_name).into());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("NvimWeb Renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create device: {}", e))?;

        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("Failed to get surface config")?;
        surface.configure(&device, &config);

        // --- Resources ---
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Global Uniforms"),
            size: std::mem::size_of::<GlobalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let initial_capacity = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (initial_capacity * std::mem::size_of::<InstanceInput>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Texture Atlas
        let atlas_texture_size = wgpu::Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        };
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: atlas_texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("Atlas Texture"),
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Bind Group
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
            label: Some("Bind Group"),
        });

        // Pipeline
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<InstanceInput>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        5 => Float32x2, // pos_min
                        6 => Float32x2, // size
                        7 => Float32x2, // uv_min
                        8 => Float32x2, // uv_max
                        9 => Float32x4, // bg
                        10 => Float32x4, // fg
                        11 => Uint32,   // style_flags
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Glyph rasterization canvas
        let glyph_canvas =
            OffscreenCanvas::new(64, 64).map_err(|_| "Failed to create glyph canvas")?;
        let glyph_ctx: OffscreenCanvasRenderingContext2d = glyph_canvas
            .get_context("2d")
            .map_err(|_| "Failed to get 2d context")?
            .ok_or("No 2d context")?
            .dyn_into()
            .map_err(|_| "Failed to cast ctx")?;

        Ok(WebGpuBackend {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group,
            bind_group_layout,
            instance_buffer,
            current_instance_capacity: initial_capacity,
            uniform_buffer,
            atlas_texture,
            current_atlas_size: ATLAS_SIZE,
            atlas_map: HashMap::new(),
            next_atlas_pos: (0, 0),
            max_line_height: 20,
            glyph_canvas,
            glyph_ctx,
        })
    }

    pub fn update_font_metric(&mut self) {
        // Measure using a temporary canvas if needed, or the one we have
        let font_str = format!("{}px {}", self.font_size, self.font_family);

        // For measuring, we can use the backend's context if available, or just create a 1x1 one?
        // Actually both backends have a context we can use.
        // WebGpuBackend has glyph_ctx.
        // CanvasBackend has ctx.

        let (width, height) = match &self.backend {
            Backend::WebGpu(bg) => {
                bg.glyph_ctx.set_font(&font_str);
                let w = bg
                    .glyph_ctx
                    .measure_text("m")
                    .ok()
                    .map(|m| m.width())
                    .unwrap_or(self.font_size * 0.6);
                (w, self.font_size * 1.2)
            }
            Backend::Canvas(bg) => {
                bg.ctx.set_font(&font_str);
                let w = bg
                    .ctx
                    .measure_text("m")
                    .ok()
                    .map(|m| m.width())
                    .unwrap_or(self.font_size * 0.6);
                (w, self.font_size * 1.2)
            }
        };

        self.cell_w = width;
        self.cell_h = height;
    }

    pub fn set_font(&mut self, font_family: &str, font_size_px: Option<f64>) {
        self.font_family = font_family.to_string();
        if let Some(size) = font_size_px {
            self.font_size = size;
        }
        self.update_font_metric();

        match &mut self.backend {
            Backend::WebGpu(bg) => {
                bg.atlas_map.clear();
                bg.next_atlas_pos = (0, 0);
                bg.max_line_height = 0;
            }
            Backend::Canvas(_) => {
                // No atlas to clear for canvas
            }
        }

        web_sys::console::log_1(
            &format!(
                "Renderer: Font set to '{}' {}px",
                self.font_family, self.font_size
            )
            .into(),
        );
    }

    /// Update cursor blink state based on current timestamp
    pub fn update_cursor_blink(&mut self, timestamp: f64) {
        if timestamp - self.cursor.last_toggle >= self.cursor.blink_ms {
            self.cursor.visible = !self.cursor.visible;
            self.cursor.last_toggle = timestamp;
        }
    }

    /// Set cursor shape based on Neovim mode
    pub fn set_cursor_shape(&mut self, mode: &str) {
        self.cursor.shape = match mode.to_lowercase().as_str() {
            "insert" | "i" => CursorShape::Beam,
            "replace" | "r" | "R" => CursorShape::Underline,
            _ => CursorShape::Block,
        };
        // Reset visibility when shape changes
        self.cursor.visible = true;
        self.cursor.last_toggle = 0.0;
    }

    /// Check if cursor should be rendered (for blink)
    pub fn cursor_visible(&self) -> bool {
        self.cursor.visible
    }

    pub fn resize(&mut self, width: f64, height: f64) -> (usize, usize) {
        self.size = (width as u32, height as u32);

        match &mut self.backend {
            Backend::WebGpu(bg) => {
                bg.config.width = width as u32;
                bg.config.height = height as u32;
                bg.surface.configure(&bg.device, &bg.config);

                let resolution = [width as f32, height as f32];
                bg.queue.write_buffer(
                    &bg.uniform_buffer,
                    0,
                    bytemuck::cast_slice(&[GlobalUniforms { resolution }]),
                );
            }
            Backend::Canvas(bg) => {
                bg.width = width as u32;
                bg.height = height as u32;
                // Canvas size is likely handled by the offscreen canvas itself?
                // Actually the OffscreenCanvas object is what we drew to?
                // Wait, OffscreenCanvasRenderingContext2d is linked to the canvas.
                // We access the canvas via context.canvas
                let canvas = bg.ctx.canvas();
                canvas.set_width(width as u32);
                canvas.set_height(height as u32);
            }
        }

        // Return grid dimensions
        let cols = (width / self.cell_w).floor() as usize;
        let rows = (height / self.cell_h).floor() as usize;
        (rows.max(1), cols.max(1))
    }

    pub fn render(&mut self, grids: &GridManager, highlights: &HighlightMap) {
        match &mut self.backend {
            Backend::WebGpu(bg) => {
                Self::render_webgpu(bg, grids, highlights, self.cell_w, self.cell_h)
            }
            Backend::Canvas(bg) => Self::render_canvas(
                bg,
                grids,
                highlights,
                self.cell_w,
                self.cell_h,
                &self.font_family,
                self.font_size,
            ),
        }
    }

    fn render_webgpu(
        bg: &mut WebGpuBackend,
        grids: &GridManager,
        highlights: &HighlightMap,
        cell_w: f64,
        cell_h: f64,
    ) {
        // 1. Build Instance Data
        let mut instances: Vec<InstanceInput> = Vec::with_capacity(4096);

        // Render all visible grids in z-order
        for grid in grids.grids_in_order() {
            let offset_x = grid.col_offset as f32 * cell_w as f32;
            let offset_y = grid.row_offset as f32 * cell_h as f32;

            for (idx, cell) in grid.cells.iter().enumerate() {
                let row = idx / grid.cols;
                let col = idx % grid.cols;

                // Skip unchanged cells for performance (dirty region rendering)
                // Only skip if not cursor row (cursor always needs redraw)
                let is_cursor_row = grid.id == grids.active_grid_id() && row == grid.cursor_row;
                if !cell.dirty && !grid.dirty_all && !is_cursor_row {
                    continue;
                }

                let x = (col as f64 * cell_w) as f32 + offset_x;
                let y = (row as f64 * cell_h) as f32 + offset_y;

                // Colors
                let mut bg_col = Self::get_color(cell.hl_id, true, highlights);
                let mut fg_col = Self::get_color(cell.hl_id, false, highlights);

                // Cursor highlight
                if grid.id == grids.active_grid_id() && row == grid.cursor_row {
                    bg_col[0] = (bg_col[0] + 0.05).min(1.0);
                    bg_col[1] = (bg_col[1] + 0.05).min(1.0);
                    bg_col[2] = (bg_col[2] + 0.08).min(1.0);
                }

                let mut style_flags = Self::get_style_flags(cell.hl_id, highlights);

                // Cursor
                if grid.id == grids.active_grid_id()
                    && row == grid.cursor_row
                    && col == grid.cursor_col
                {
                    let now = web_sys::window()
                        .and_then(|w| w.performance())
                        .map(|p| p.now())
                        .unwrap_or(0.0);
                    let cursor_visible = ((now as u64 / 530) % 2) == 0;

                    if cursor_visible {
                        let mode = grids.get_mode();
                        let cursor_type = match mode {
                            "insert" | "i" => 2,
                            "replace" | "r" | "R" => 3,
                            _ => 1,
                        };
                        style_flags |= cursor_type << 4;

                        if cursor_type == 1 {
                            bg_col = [0.9, 0.9, 0.9, 1.0];
                            fg_col = [0.1, 0.1, 0.1, 1.0];
                        }
                    }
                }

                let is_bold = (style_flags & 1) != 0;
                let is_italic = (style_flags & 2) != 0;
                let (uv_min, uv_max) = Self::get_glyph_uv(
                    bg,
                    cell.ch,
                    is_bold,
                    is_italic,
                    cell_w,
                    cell_h,
                    14.0,
                    "monospace",
                );

                instances.push(InstanceInput {
                    pos_min: [x, y],
                    size: [cell_w as f32, cell_h as f32],
                    atlas_uv_min: uv_min,
                    atlas_uv_max: uv_max,
                    bg_color: bg_col,
                    fg_color: fg_col,
                    style_flags,
                    _padding: [0, 0, 0],
                });
            }

            // Borders
            if grid.id != 1 && grid.is_visible {
                let border_color = [0.35, 0.35, 0.40, 1.0];
                let border_uv =
                    Self::get_glyph_uv(bg, ' ', false, false, cell_w, cell_h, 14.0, "monospace");

                let grid_width = grid.cols as f32 * cell_w as f32;
                let grid_height = grid.rows as f32 * cell_h as f32;
                let border_thickness = 2.0;

                // Top
                instances.push(InstanceInput {
                    pos_min: [offset_x - border_thickness, offset_y - border_thickness],
                    size: [grid_width + border_thickness * 2.0, border_thickness],
                    atlas_uv_min: border_uv.0,
                    atlas_uv_max: border_uv.1,
                    bg_color: border_color,
                    fg_color: border_color,
                    style_flags: 0,
                    _padding: [0, 0, 0],
                });
                // Bottom
                instances.push(InstanceInput {
                    pos_min: [offset_x - border_thickness, offset_y + grid_height],
                    size: [grid_width + border_thickness * 2.0, border_thickness],
                    atlas_uv_min: border_uv.0,
                    atlas_uv_max: border_uv.1,
                    bg_color: border_color,
                    fg_color: border_color,
                    style_flags: 0,
                    _padding: [0, 0, 0],
                });
                // Left
                instances.push(InstanceInput {
                    pos_min: [offset_x - border_thickness, offset_y],
                    size: [border_thickness, grid_height],
                    atlas_uv_min: border_uv.0,
                    atlas_uv_max: border_uv.1,
                    bg_color: border_color,
                    fg_color: border_color,
                    style_flags: 0,
                    _padding: [0, 0, 0],
                });
                // Right
                instances.push(InstanceInput {
                    pos_min: [offset_x + grid_width, offset_y],
                    size: [border_thickness, grid_height],
                    atlas_uv_min: border_uv.0,
                    atlas_uv_max: border_uv.1,
                    bg_color: border_color,
                    fg_color: border_color,
                    style_flags: 0,
                    _padding: [0, 0, 0],
                });
            }
        }

        // Upload and Draw
        Self::ensure_instance_capacity(bg, instances.len());
        bg.queue
            .write_buffer(&bg.instance_buffer, 0, bytemuck::cast_slice(&instances));

        let output = bg.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = bg
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(&bg.pipeline);
            rpass.set_bind_group(0, &bg.bind_group, &[]);
            rpass.set_vertex_buffer(0, bg.instance_buffer.slice(..));
            rpass.draw(0..4, 0..instances.len() as u32);
        }

        bg.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    fn render_canvas(
        bg: &mut CanvasBackend,
        grids: &GridManager,
        highlights: &HighlightMap,
        cell_w: f64,
        cell_h: f64,
        font_family: &str,
        font_size: f64,
    ) {
        // Clear
        bg.ctx.set_fill_style_str("#1a1a1e"); // Background color
        bg.ctx
            .fill_rect(0.0, 0.0, bg.width as f64, bg.height as f64);

        for grid in grids.grids_in_order() {
            let offset_x = grid.col_offset as f64 * cell_w;
            let offset_y = grid.row_offset as f64 * cell_h;

            for (idx, cell) in grid.cells.iter().enumerate() {
                let row = idx / grid.cols;
                let col = idx % grid.cols;

                // Skip unchanged cells for performance (dirty region rendering)
                let is_cursor_row = grid.id == grids.active_grid_id() && row == grid.cursor_row;
                if !cell.dirty && !grid.dirty_all && !is_cursor_row {
                    continue;
                }

                let x = (col as f64 * cell_w) + offset_x;
                let y = (row as f64 * cell_h) + offset_y;

                let mut bg_col = Self::get_css_color(cell.hl_id, true, highlights);
                let mut fg_col = Self::get_css_color(cell.hl_id, false, highlights);

                // Cursor logic
                let is_cursor = grid.id == grids.active_grid_id()
                    && row == grid.cursor_row
                    && col == grid.cursor_col;
                if is_cursor {
                    let now = web_sys::window().unwrap().performance().unwrap().now();
                    let cursor_visible = ((now as u64 / 530) % 2) == 0;
                    if cursor_visible {
                        // Swap for block cursor (approximate)
                        let temp = bg_col;
                        bg_col = fg_col;
                        fg_col = temp;
                    }
                }

                if grid.id == grids.active_grid_id() && row == grid.cursor_row && !is_cursor {
                    // Cursor line (approximate)
                    // Hard to do alpha blending with simple strings, skip for now or use fillRect with alpha
                }

                // Draw BG
                bg.ctx.set_fill_style_str(&bg_col);
                bg.ctx.fill_rect(x, y, cell_w, cell_h);

                if cell.ch == ' ' {
                    continue;
                }

                // Draw FG
                bg.ctx.set_fill_style_str(&fg_col);
                let mut font = format!("{}px {}", font_size, font_family);
                let style_flags = Self::get_style_flags(cell.hl_id, highlights);
                if (style_flags & 1) != 0 {
                    font = format!("bold {}", font);
                }
                if (style_flags & 2) != 0 {
                    font = format!("italic {}", font);
                }

                bg.ctx.set_font(&font);
                bg.ctx.set_text_baseline("top");
                let _ = bg.ctx.fill_text(&cell.ch.to_string(), x, y);
            }
        }
    }

    // --- Helpers ---

    pub fn cell_size(&self) -> (f64, f64) {
        (self.cell_w, self.cell_h)
    }

    fn u32_to_rgba(color: u32) -> [f32; 4] {
        let r = ((color >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color >> 8) & 0xFF) as f32 / 255.0;
        let b = (color & 0xFF) as f32 / 255.0;
        [r, g, b, 1.0]
    }

    fn u32_to_css(color: u32) -> String {
        let r = (color >> 16) & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = color & 0xFF;
        format!("rgb({},{},{})", r, g, b)
    }

    fn get_color(hl_id: Option<u32>, is_bg: bool, map: &HighlightMap) -> [f32; 4] {
        let default_bg = map
            .default_bg
            .map(Self::u32_to_rgba)
            .unwrap_or([0.1, 0.1, 0.12, 1.0]);
        let default_fg = map
            .default_fg
            .map(Self::u32_to_rgba)
            .unwrap_or([0.85, 0.85, 0.85, 1.0]);

        if let Some(id) = hl_id {
            if let Some(hl) = map.get(id) {
                if is_bg {
                    return hl.bg.map(Self::u32_to_rgba).unwrap_or(default_bg);
                } else {
                    return hl.fg.map(Self::u32_to_rgba).unwrap_or(default_fg);
                }
            }
        }
        if is_bg {
            default_bg
        } else {
            default_fg
        }
    }

    fn get_css_color(hl_id: Option<u32>, is_bg: bool, map: &HighlightMap) -> String {
        let default_bg = map
            .default_bg
            .map(Self::u32_to_css)
            .unwrap_or(String::from("#1a1a1e"));
        let default_fg = map
            .default_fg
            .map(Self::u32_to_css)
            .unwrap_or(String::from("#dcd7ba")); // Kanagawa-ish

        if let Some(id) = hl_id {
            if let Some(hl) = map.get(id) {
                if is_bg {
                    return hl.bg.map(Self::u32_to_css).unwrap_or(default_bg);
                } else {
                    return hl.fg.map(Self::u32_to_css).unwrap_or(default_fg);
                }
            }
        }
        if is_bg {
            default_bg
        } else {
            default_fg
        }
    }

    fn get_style_flags(hl_id: Option<u32>, map: &HighlightMap) -> u32 {
        if let Some(id) = hl_id {
            if let Some(hl) = map.get(id) {
                let mut flags = 0u32;
                if hl.bold {
                    flags |= 1;
                }
                if hl.italic {
                    flags |= 2;
                }
                if hl.underline || hl.undercurl {
                    flags |= 4;
                }
                if hl.strikethrough {
                    flags |= 8;
                }
                return flags;
            }
        }
        0
    }

    // --- WGPU Helpers ---

    fn ensure_instance_capacity(bg: &mut WebGpuBackend, required_count: usize) {
        if required_count > bg.current_instance_capacity {
            let new_capacity = (bg.current_instance_capacity * 2).max(required_count);
            bg.instance_buffer = bg.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer"),
                size: (new_capacity * std::mem::size_of::<InstanceInput>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            bg.current_instance_capacity = new_capacity;
        }
    }

    fn grow_atlas(bg: &mut WebGpuBackend) {
        let new_size = (bg.current_atlas_size * 2).min(8192);
        if new_size <= bg.current_atlas_size {
            return;
        }

        let atlas_texture_size = wgpu::Extent3d {
            width: new_size,
            height: new_size,
            depth_or_array_layers: 1,
        };
        let new_texture = bg.device.create_texture(&wgpu::TextureDescriptor {
            size: atlas_texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("Atlas Texture"),
            view_formats: &[],
        });

        let new_view = new_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let new_sampler = bg.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let new_bind_group = bg.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bg.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: bg.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&new_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&new_sampler),
                },
            ],
            label: Some("Bind Group"),
        });

        bg.atlas_texture = new_texture;
        bg.bind_group = new_bind_group;
        bg.current_atlas_size = new_size;

        bg.atlas_map.clear();
        bg.next_atlas_pos = (0, 0);
        bg.max_line_height = 0;
    }

    fn get_glyph_uv(
        bg: &mut WebGpuBackend,
        ch: char,
        bold: bool,
        italic: bool,
        cell_w: f64,
        cell_h: f64,
        font_size: f64,
        font_family: &str,
    ) -> ([f32; 2], [f32; 2]) {
        let key = (ch, bold, italic);
        if let Some(&uvs) = bg.atlas_map.get(&key) {
            return uvs;
        }

        let glyph_w = cell_w.ceil() as u32;
        let glyph_h = cell_h.ceil() as u32;

        if bg.next_atlas_pos.0 + glyph_w > bg.current_atlas_size {
            bg.next_atlas_pos.0 = 0;
            bg.next_atlas_pos.1 += bg.max_line_height;
            bg.max_line_height = glyph_h;
        }

        if bg.next_atlas_pos.1 + glyph_h > bg.current_atlas_size {
            Self::grow_atlas(bg);
        }

        let atlas_x = bg.next_atlas_pos.0;
        let atlas_y = bg.next_atlas_pos.1;

        if bg.glyph_canvas.width() != glyph_w || bg.glyph_canvas.height() != glyph_h {
            bg.glyph_canvas.set_width(glyph_w);
            bg.glyph_canvas.set_height(glyph_h);
        }

        bg.glyph_ctx
            .clear_rect(0.0, 0.0, glyph_w as f64, glyph_h as f64);
        let font_style = match (bold, italic) {
            (true, true) => format!("bold italic {}px {}", font_size, font_family),
            (true, false) => format!("bold {}px {}", font_size, font_family),
            (false, true) => format!("italic {}px {}", font_size, font_family),
            (false, false) => format!("{}px {}", font_size, font_family),
        };
        bg.glyph_ctx.set_font(&font_style);
        bg.glyph_ctx.set_text_baseline("top");
        bg.glyph_ctx.set_fill_style_str("white");
        let _ = bg.glyph_ctx.fill_text(&ch.to_string(), 0.0, 0.0);

        let image_data = bg
            .glyph_ctx
            .get_image_data(0.0, 0.0, glyph_w as f64, glyph_h as f64)
            .expect("Failed to get image data");
        let pixel_data = image_data.data();

        bg.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &bg.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: atlas_x,
                    y: atlas_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixel_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(glyph_w * 4),
                rows_per_image: Some(glyph_h),
            },
            wgpu::Extent3d {
                width: glyph_w,
                height: glyph_h,
                depth_or_array_layers: 1,
            },
        );

        let uv_min = [
            atlas_x as f32 / bg.current_atlas_size as f32,
            atlas_y as f32 / bg.current_atlas_size as f32,
        ];
        let uv_max = [
            (atlas_x + glyph_w) as f32 / bg.current_atlas_size as f32,
            (atlas_y + glyph_h) as f32 / bg.current_atlas_size as f32,
        ];

        bg.next_atlas_pos.0 += glyph_w;
        bg.max_line_height = bg.max_line_height.max(glyph_h);
        bg.atlas_map.insert(key, (uv_min, uv_max));

        (uv_min, uv_max)
    }
}
