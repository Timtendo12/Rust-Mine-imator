// Effects on the finished picture (`render_post`): bloom, chromatic
// aberration, lens distortion, colour correction, film grain and vignette.
// Ported from shader_high_bloom_threshold, shader_blur, shader_add,
// shader_ca, shader_distort, shader_color_correction, shader_noise and
// shader_vignette. Each effect is one pass from one picture to the next.

struct Post {
    // Meaning depends on the effect.
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    // xy: size of the picture in pixels
    size: vec4<f32>,
    // Blur: x is a weight, y the offset it is taken at.
    kernel: array<vec4<f32>, 19>,
}

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var extra: texture_2d<f32>;
@group(0) @binding(3) var clamped: sampler;
@group(0) @binding(4) var repeating: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn uv_of(position: vec4<f32>) -> vec2<f32> {
    return position.xy / post.size.xy;
}

// The picture as it is, at a place on a target: a.xy is where it starts.
@fragment
fn fs_present(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy - post.a.xy);
    return vec4<f32>(textureLoad(source, pixel, 0).rgb, 1.0);
}

@fragment
fn fs_copy(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(position.xy), 0);
}

// Bloom: what is brighter than a.x.
@fragment
fn fs_threshold(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(source, vec2<i32>(position.xy), 0);
    if (max(max(color.r, color.g), color.b) > post.a.x) {
        return color;
    }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// Coordinates outside the picture are mirrored back into it.
fn mirrored(uv: vec2<f32>) -> vec2<f32> {
    var at = uv;
    if (at.x > 1.0) { at.x = 2.0 - at.x; }
    if (at.x < 0.0) { at.x = -at.x; }
    if (at.y > 1.0) { at.y = 2.0 - at.y; }
    if (at.y < 0.0) { at.y = -at.y; }
    return at;
}

// Gaussian blur along a.yz with radius a.x in pixels.
@fragment
fn fs_blur(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = uv_of(position);
    let step = post.a.x / post.size.xy * post.a.yz;
    var result = vec4<f32>(0.0);
    for (var i = 0; i < 19; i++) {
        result += post.kernel[i].x * textureSampleLevel(source, clamped, mirrored(uv + post.kernel[i].y * step), 0.0);
    }
    return result;
}

// The picture plus `extra` times a.x, tinted by b.rgb.
@fragment
fn fs_add(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let base = textureLoad(source, pixel, 0);
    let added = textureLoad(extra, pixel, 0);
    let value = (added.r + added.g + added.b) / 3.0;
    return vec4<f32>(base.rgb + added.rgb * post.b.rgb * post.a.x, min(base.a + value, 1.0));
}

fn lens(coord: vec2<f32>, amount: f32) -> vec2<f32> {
    let d = dot(coord, coord);
    let distortion = amount * -0.25;
    return coord * (1.0 + distortion * d + distortion * d * d);
}

// Chromatic aberration: the colour channels are scaled (or bent, with
// a.y) by b.rgb, smeared by a.x.
@fragment
fn fs_ca(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = uv_of(position);
    let quality = 32;
    var color = vec3<f32>(0.0);
    var offset = post.b.rgb;
    for (var i = 0; i < quality; i++) {
        let uv = (coord - 0.5) * 2.0;
        var red = uv * (1.0 - offset.x * 0.25);
        var green = uv * (1.0 - offset.y * 0.25);
        var blue = uv * (1.0 - offset.z * 0.25);
        if (post.a.y > 0.5) {
            red = lens(uv, offset.x);
            green = lens(uv, offset.y);
            blue = lens(uv, offset.z);
        }
        color.r += textureSampleLevel(source, clamped, red * 0.5 + 0.5, 0.0).r;
        color.g += textureSampleLevel(source, clamped, green * 0.5 + 0.5, 0.0).g;
        color.b += textureSampleLevel(source, clamped, blue * 0.5 + 0.5, 0.0).b;
        offset = mix(post.b.rgb, post.b.rgb + post.a.x, f32(i) / f32(quality));
    }
    return vec4<f32>(color / f32(quality), textureSampleLevel(source, clamped, coord, 0.0).a);
}

// Lens distortion by a.x, zoomed by a.z; outside the picture it repeats
// (a.y) or is black.
@fragment
fn fs_distort(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var uv = (uv_of(position) - 0.5) * 2.0 / post.a.z;
    uv = lens(uv, post.a.x) * 0.5 + 0.5;
    if (post.a.y > 0.5) {
        return textureSampleLevel(source, repeating, uv, 0.0);
    }
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(source, clamped, uv, 0.0);
}

fn saturation_of(c: vec3<f32>) -> f32 {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, k.wz), vec4<f32>(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    return d / (q.x + 1.0e-10);
}

// Colour correction: a = contrast, brightness, saturation, vibrance;
// b.rgb = colour burn.
@fragment
fn fs_cc(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let base = textureLoad(source, vec2<i32>(position.xy), 0);
    var rgb = (base.rgb - vec3<f32>(0.5)) * post.a.x + vec3<f32>(post.a.y + 0.5);
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    let w = vec3<f32>(0.2125, 0.7154, 0.0721);
    rgb = clamp(mix(vec3<f32>(dot(rgb, w)), rgb, post.a.z), vec3<f32>(0.0), vec3<f32>(1.0));

    // Vibrance saturates what is not saturated yet.
    let vibrance = 1.0 - pow(pow(saturation_of(rgb), 8.0), 0.15);
    rgb = clamp(mix(vec3<f32>(dot(rgb, w)), rgb, 1.0 + vibrance * post.a.w), vec3<f32>(0.0), vec3<f32>(1.0));

    rgb = 1.0 - (1.0 - rgb) / max(post.b.rgb, vec3<f32>(0.0001));
    return vec4<f32>(rgb, base.a);
}

// Film grain: noise from `extra`, a.x strong, a.y saturated, in cells of
// a.zw pixels of the noise picture.
@fragment
fn fs_grain(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let base = textureLoad(source, vec2<i32>(position.xy), 0);
    let coords = uv_of(position) * (post.size.xy / post.a.zw);
    var noise = textureSampleLevel(extra, repeating, coords, 0.0).rgb;
    let w = vec3<f32>(0.2125, 0.7154, 0.0721);
    noise = mix(vec3<f32>(dot(noise, w)), noise, post.a.y);
    return vec4<f32>(base.rgb + noise * post.a.x, base.a);
}

// Vignette: beyond radius a.x the picture turns to b.rgb, over a.y, by a.z.
@fragment
fn fs_vignette(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let base = textureLoad(source, vec2<i32>(position.xy), 0);
    let distance = length(uv_of(position) - vec2<f32>(0.5));
    let amount = smoothstep(post.a.x, post.a.x - clamp(post.a.y, 0.005, 1.0), distance);
    return vec4<f32>(mix(base.rgb, mix(post.b.rgb, base.rgb, amount), post.a.z), base.a);
}
