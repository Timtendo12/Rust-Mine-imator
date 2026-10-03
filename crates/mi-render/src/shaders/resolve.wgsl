// The progressive render: every sample is averaged into what has been
// gathered so far (`render_high_samples_add`), and the average is shown
// (`render_high_samples_unpack`).

struct Resolve {
    // x: samples gathered before this one, yz: where the view is on the target
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> resolve: Resolve;
@group(0) @binding(1) var gathered: texture_2d<f32>;
@group(0) @binding(2) var sample_image: texture_2d<f32>;

// One triangle that covers the target.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_gather(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let before = textureLoad(gathered, pixel, 0);
    let now = textureLoad(sample_image, pixel, 0);
    let count = resolve.params.x;
    return (before * count + now) / (count + 1.0);
}

@fragment
fn fs_show(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy - resolve.params.yz);
    return vec4<f32>(textureLoad(gathered, pixel, 0).rgb, 1.0);
}
