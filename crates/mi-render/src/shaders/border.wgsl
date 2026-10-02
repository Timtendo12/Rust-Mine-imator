// The outline around selected objects (shader_border): pixels outside the
// selection mask with a masked pixel two pixels away diagonally.

struct Border {
    // x, y: where the viewport starts on the target; z, w: its size
    viewport: vec4<f32>,
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> border: Border;
@group(0) @binding(1) var mask: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the whole target.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn masked(pixel: vec2<i32>) -> bool {
    let size = vec2<i32>(border.viewport.zw);
    if (pixel.x < 0 || pixel.y < 0 || pixel.x >= size.x || pixel.y >= size.y) {
        return false;
    }
    return textureLoad(mask, pixel, 0).r > 0.0;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy - border.viewport.xy);
    let size = 2;
    if (masked(pixel)) {
        discard;
    }
    if (masked(pixel + vec2<i32>(size, size)) || masked(pixel + vec2<i32>(-size, size))
        || masked(pixel + vec2<i32>(size, -size)) || masked(pixel + vec2<i32>(-size, -size))) {
        return border.color;
    }
    discard;
    return vec4<f32>(0.0);
}
