// Object ids for clicking in the viewport (the original's selection render):
// every object writes its pick id; fully transparent texels let what is
// behind them through, as they do when drawn.

struct Frame {
    view_proj: mat4x4<f32>,
}

struct Object {
    model: mat4x4<f32>,
    blend_color: vec4<f32>,
    rgb_add: vec4<f32>,
    rgb_sub: vec4<f32>,
    hsb_add: vec4<f32>,
    hsb_sub: vec4<f32>,
    hsb_mul: vec4<f32>,
    mix_color: vec4<f32>,
    material: vec4<f32>,
    // w: pick id
    flags: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> object: Object;
@group(2) @binding(0) var base_texture: texture_2d<f32>;
@group(2) @binding(1) var base_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) custom: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip = frame.view_proj * object.model * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.alpha = in.color.a * object.blend_color.a;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) u32 {
    if (in.alpha * textureSample(base_texture, base_sampler, in.uv).a == 0.0) {
        discard;
    }
    return u32(object.flags.w + 0.5);
}
