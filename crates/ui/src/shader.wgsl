// WebGPU Shader for nvim-web Terminal Rendering
// Uses instanced rendering for efficient cell drawing

struct GlobalUniforms {
    resolution: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) bg_color: vec4<f32>,
    @location(2) fg_color: vec4<f32>,
    @location(3) local_pos: vec2<f32>,  // Position within cell (0-1)
    @location(4) @interpolate(flat) style_flags: u32,
}

@group(0) @binding(0) var<uniform> uniforms: GlobalUniforms;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

// Quad vertices (triangle strip: 0,1,2,3)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0), // Top-left
    vec2<f32>(1.0, 0.0), // Top-right
    vec2<f32>(0.0, 1.0), // Bottom-left
    vec2<f32>(1.0, 1.0), // Bottom-right
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    // Instance attributes
    @location(5) pos_min: vec2<f32>,
    @location(6) size: vec2<f32>,
    @location(7) atlas_uv_min: vec2<f32>,
    @location(8) atlas_uv_max: vec2<f32>,
    @location(9) bg_color: vec4<f32>,
    @location(10) fg_color: vec4<f32>,
    @location(11) style_flags: u32,
) -> VertexOutput {
    var output: VertexOutput;
    
    // Get quad corner (0-3)
    let corner = QUAD[vertex_index];
    
    // Calculate pixel position
    let pixel_pos = pos_min + corner * size;
    
    // Convert to clip space (-1 to 1)
    let clip_pos = (pixel_pos / uniforms.resolution) * 2.0 - 1.0;
    // Flip Y for correct orientation
    output.position = vec4<f32>(clip_pos.x, -clip_pos.y, 0.0, 1.0);
    
    // Interpolate UV coordinates
    output.uv = mix(atlas_uv_min, atlas_uv_max, corner);
    
    // Pass colors to fragment shader
    output.bg_color = bg_color;
    output.fg_color = fg_color;
    
    // Pass local position (0-1 within cell) for decorations
    output.local_pos = corner;
    
    // Pass style flags
    output.style_flags = style_flags;
    
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample glyph from atlas (alpha channel contains glyph)
    let glyph_sample = textureSample(atlas_texture, atlas_sampler, input.uv);
    
    // Use glyph alpha to blend foreground over background
    let glyph_alpha = glyph_sample.r;
    
    // Blend: background + foreground * glyph_alpha
    var color = mix(input.bg_color, input.fg_color, glyph_alpha);
    
    // Style decorations
    let underline = (input.style_flags & 4u) != 0u;
    let strikethrough = (input.style_flags & 8u) != 0u;
    
    // Cursor type (bits 4-5): 0=none, 1=block, 2=line, 3=underline
    let cursor_type = (input.style_flags >> 4u) & 3u;
    
    // Underline: draw line at bottom 10% of cell
    if underline && input.local_pos.y > 0.88 && input.local_pos.y < 0.95 {
        color = input.fg_color;
    }
    
    // Strikethrough: draw line at middle of cell
    if strikethrough && input.local_pos.y > 0.45 && input.local_pos.y < 0.55 {
        color = input.fg_color;
    }
    
    // Cursor shapes
    if cursor_type == 2u {
        // Line cursor (bar): draw at left edge of cell
        if input.local_pos.x < 0.12 {
            color = vec4<f32>(0.9, 0.9, 0.9, 1.0);
        }
    } else if cursor_type == 3u {
        // Underline cursor: draw at bottom of cell
        if input.local_pos.y > 0.85 {
            color = vec4<f32>(0.9, 0.9, 0.9, 1.0);
        }
    }
    // Block cursor (type 1) is handled by swapping bg/fg colors in Rust
    
    return color;
}
