// World geometry in the high quality mode: lit per pixel by the sun, with
// its shadows, and by point lights; one sample of the progressive render.
// Ported from shader_high_light_sun, shader_high_lighting_apply and
// shader_tonemap, drawn in one pass instead of composited from separate
// surfaces. Not ported yet: shadows of point and spot lights, subsurface
// scattering, material and normal maps.

const MAX_LIGHTS: u32 = 64u;
const PI: f32 = 3.14159265;
const CASCADES: u32 = 3u;

struct Frame {
    view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    ambient_color: vec4<f32>,
    fallback_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    tone: vec4<f32>,
    wind: vec4<f32>,
    wind_gusts: vec4<f32>,
    lights: array<vec4<f32>, 128>,
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
    flags: vec4<f32>,
    wind: vec4<f32>,
}

struct Shadows {
    // World to shadow map: xy are texture coordinates, z is the depth.
    matrices: array<mat4x4<f32>, 3>,
    // View depth each cascade ends at.
    ends: vec4<f32>,
    // Depth bias of each cascade.
    bias: vec4<f32>,
    // x: shadows on
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> object: Object;
@group(2) @binding(0) var base_texture: texture_2d<f32>;
@group(2) @binding(1) var base_sampler: sampler;
@group(3) @binding(0) var<uniform> shadows: Shadows;
@group(3) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(3) @binding(2) var shadow_sampler: sampler_comparison;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) custom: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) custom: vec4<f32>,
    @location(5) view_depth: f32,
}

fn gust_noise(v: f32) -> f32 {
    return cos(v * PI) * cos(v * 3.0 * PI) * cos(v * 5.0 * PI) * cos(v * 7.0 * PI) + sin(v * 5.0 * PI) * 0.1;
}

// The place of a vertex in the world, with the wind.
fn world_position(position: vec3<f32>, custom: vec4<f32>) -> vec3<f32> {
    var local = position;
    var pushed = vec3<f32>(0.0);
    let sway_xy = max(custom.x * object.wind.y, object.wind.x);
    let sway_z = max(custom.y * object.wind.y, object.wind.x);
    if (max((custom.x + custom.y) * object.wind.y, object.wind.x) * object.wind.z > 0.0) {
        let p = position;
        let time = frame.wind.x;
        let speed = frame.wind.y;
        local += vec3<f32>(
            sin((time + p.x * 10.0 + p.y + p.z) * (speed / 5.0)) * sway_xy,
            sin((time + p.x + p.y * 10.0 + p.z) * (speed / 7.5)) * sway_xy,
            sin((time + p.x + p.y + p.z * 10.0) * (speed / 10.0)) * sway_z,
        ) * object.wind.z;
        let direction = frame.wind.zw;
        let along = dot(p.xy / 16.0, direction) / max(dot(direction, direction), 0.0001);
        let gust = gust_noise((frame.wind_gusts.x - along / 3.0 - p.z / 64.0) * 0.075);
        if (sway_xy > 0.0) {
            pushed = vec3<f32>(direction * gust, 0.0) * object.wind.w;
        }
    }
    return (object.model * vec4<f32>(local, 1.0)).xyz + pushed;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = world_position(in.position, in.custom);
    out.normal = (object.model * vec4<f32>(in.normal, 0.0)).xyz;
    out.custom = in.custom;
    out.color = in.color * object.blend_color;
    out.uv = in.uv;
    out.clip = frame.view_proj * vec4<f32>(out.position, 1.0);
    out.view_depth = out.clip.w;
    return out;
}

// Depth only, from the sun: `frame.view_proj` is a cascade's matrix.
struct ShadowOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vs_shadow(in: VertexInput) -> ShadowOutput {
    var out: ShadowOutput;
    out.clip = frame.view_proj * vec4<f32>(world_position(in.position, in.custom), 1.0);
    out.uv = in.uv;
    out.alpha = in.color.a * object.blend_color.a;
    return out;
}

@fragment
fn fs_shadow(in: ShadowOutput) {
    // Fully transparent texels cast no shadow.
    if (in.alpha * textureSample(base_texture, base_sampler, in.uv).a == 0.0) {
        discard;
    }
}

fn rgb_to_hsb(c: vec4<f32>) -> vec4<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, k.wz), vec4<f32>(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec4<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x, c.a);
}

fn hsb_to_rgb(c: vec4<f32>) -> vec4<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + k.xyz) * 6.0 - k.www);
    return vec4<f32>(c.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y), c.a);
}

fn fresnel_schlick_roughness(cos_theta: f32, f0: f32, roughness: f32) -> f32 {
    return clamp(f0 + (max(1.0 - roughness, f0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0), 0.0, 1.0);
}

// GGX specular (https://learnopengl.com/PBR/Lighting)
fn distribution_ggx(n: vec3<f32>, h: vec3<f32>, roughness: f32) -> f32 {
    let a2 = roughness * roughness * roughness * roughness;
    let n_dot_h = max(dot(n, h), 0.0);
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = r * r / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

fn geometry_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32) -> f32 {
    return geometry_schlick_ggx(max(dot(n, v), 0.0), roughness) * geometry_schlick_ggx(max(dot(n, l), 0.0), roughness);
}

fn fog_amount(position: vec3<f32>) -> f32 {
    if (frame.fog.x < 0.5 || object.flags.z < 0.5) {
        return 0.0;
    }
    let depth = distance(position, frame.camera_position.xyz);
    var fog = clamp(1.0 - (frame.fog.y - depth) / frame.fog.z, 0.0, 1.0);
    fog *= clamp(1.0 - (position.z - frame.fog.w) / frame.fog.z, 0.0, 1.0);
    return fog;
}

fn rrt_and_odt_fit(v: vec3<f32>) -> vec3<f32> {
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return a / b;
}

fn map_aces(color: vec3<f32>) -> vec3<f32> {
    var c = vec3<f32>(
        color.r * 0.59719 + color.g * 0.35458 + color.b * 0.04823,
        color.r * 0.07600 + color.g * 0.90834 + color.b * 0.01566,
        color.r * 0.02840 + color.g * 0.13383 + color.b * 0.83777,
    );
    c = rrt_and_odt_fit(c);
    return vec3<f32>(
        c.r * 1.60475 + c.g * -0.53108 + c.b * -0.07367,
        c.r * -0.10208 + c.g * 1.10813 + c.b * -0.00605,
        c.r * -0.00327 + c.g * -0.07276 + c.b * 1.07602,
    );
}

// How much of the sun reaches a point: 0 in shadow, 1 in the light.
fn sun_shadow(position: vec3<f32>, view_depth: f32) -> f32 {
    if (shadows.params.x < 0.5) {
        return 1.0;
    }
    var cascade = CASCADES - 1u;
    for (var i = 0u; i < CASCADES; i++) {
        if (view_depth < shadows.ends[i]) {
            cascade = i;
            break;
        }
    }
    let coord = shadows.matrices[cascade] * vec4<f32>(position, 1.0);
    // Outside the map nothing is known to be in the way.
    if (coord.x < 0.0 || coord.y < 0.0 || coord.x > 1.0 || coord.y > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_maps, shadow_sampler, coord.xy, cascade, coord.z - shadows.bias[cascade]);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = in.color * textureSample(base_texture, base_sampler, in.uv);
    if (base.a == 0.0) {
        discard;
    }
    var col = base;
    if (object.material.w > 0.5) {
        col = clamp(base + object.rgb_add - object.rgb_sub, vec4<f32>(0.0), vec4<f32>(1.0));
        col = hsb_to_rgb(clamp(rgb_to_hsb(col) + object.hsb_add - object.hsb_sub, vec4<f32>(0.0), vec4<f32>(1.0)) * object.hsb_mul);
        col = mix(col, object.mix_color, object.mix_color.a);
    }

    // The sky and what is drawn unlit keep their colours.
    if (object.flags.x > 0.5) {
        let plain = mix(col.rgb, frame.fog_color.rgb, fog_amount(in.position));
        return vec4<f32>(plain, base.a);
    }

    let gamma = frame.tone.z;
    let roughness = object.material.y;
    let metallic = object.material.x;
    let emissive = max(object.material.z, in.custom.z);
    let f0 = metallic;

    let n = normalize(in.normal);
    let v = normalize(frame.camera_position.xyz - in.position);
    let reflected = normalize(v + -reflect(v, n));
    let fresnel = fresnel_schlick_roughness(max(dot(reflected, v), 0.0), f0, roughness);

    let albedo = pow(col.rgb, vec3<f32>(gamma));

    // The sun
    let l = frame.sun_direction.xyz;
    let sun_color = frame.lights[1].rgb;
    let dif = clamp(dot(n, l), 0.0, 1.0);
    var shadow = 1.0;
    if (dif > 0.0) {
        shadow = sun_shadow(in.position, in.view_depth);
    }
    var light = frame.ambient_color.rgb + sun_color * dif * shadow;
    var specular = vec3<f32>(0.0);
    if (dif * shadow > 0.0) {
        let h = normalize(v + l);
        let ndf = distribution_ggx(n, h, roughness);
        let g = geometry_smith(n, v, l, roughness);
        let f = fresnel_schlick_roughness(max(dot(h, v), 0.0), f0, roughness);
        let amount = ndf * g * f / (4.0 * max(dot(n, v), 0.0) * max(dot(n, l), 0.0) + 0.0001);
        specular = sun_color * dif * shadow * amount * mix(vec3<f32>(1.0), albedo, metallic);
    }

    // Point lights, without shadows. The ground takes the sun only.
    var count = min(u32(frame.tone.w), MAX_LIGHTS);
    if (object.flags.y > 0.5) {
        count = min(count, 1u);
    }
    for (var i = 1u; i < count; i++) {
        let data1 = frame.lights[i * 2u];
        let data2 = frame.lights[i * 2u + 1u];
        let attenuation = max(0.0, 1.0 - distance(in.position, data1.xyz) / data1.w);
        let to_light = normalize(data1.xyz - in.position);
        light += data2.rgb * max(0.0, dot(n, to_light)) * attenuation;
    }

    // Metals have no diffuse light, and what is reflected is not diffused.
    let diffuse = light * (1.0 - metallic) * (1.0 - fresnel) + emissive;
    let reflection = mix(vec3<f32>(1.0), albedo, metallic) * pow(frame.fallback_color.rgb, vec3<f32>(gamma)) * fresnel;
    var rgb = albedo * diffuse + reflection + specular;

    rgb *= frame.tone.y;
    if (frame.tone.x > 1.5) {
        rgb = map_aces(rgb);
    } else if (frame.tone.x > 0.5) {
        rgb = rgb / (1.0 + rgb);
    }
    rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma));

    rgb = mix(rgb, frame.fog_color.rgb, fog_amount(in.position));
    return vec4<f32>(rgb, mix(base.a, 1.0, fresnel));
}
