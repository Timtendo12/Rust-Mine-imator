// World geometry in the low quality modes: unlit ("flat") or lit per vertex
// by the sun and point lights ("shaded"), with fog.
// Ported from shader_color_fog_lights.vsh / .fsh. Not ported yet: wind,
// material maps, enchantment glint, alpha hashing.

const MAX_LIGHTS: u32 = 64u;

struct Frame {
    view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    ambient_color: vec4<f32>,
    // Colour reflections fall back to (the sky).
    fallback_color: vec4<f32>,
    fog_color: vec4<f32>,
    // x: show, y: distance, z: size, w: height
    fog: vec4<f32>,
    // x: tonemapper (0 none, 1 Reinhard, 2 ACES), y: exposure, z: gamma, w: light count
    tone: vec4<f32>,
    // Two entries per light: position + range, colour. Light 0 is the sun.
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
    // x: metallic, y: roughness, z: emissive, w: extended colours on
    material: vec4<f32>,
    // x: unlit, y: ground (sun only), z: fog, w: unused
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
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) diffuse: vec3<f32>,
    @location(5) custom: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = object.model * vec4<f32>(in.position, 1.0);
    out.position = world.xyz;
    out.normal = normalize((object.model * vec4<f32>(in.normal, 0.0)).xyz);
    out.custom = in.custom;

    if (object.flags.x > 0.5) {
        // Negative diffuse marks "no shading".
        out.diffuse = vec3<f32>(-1.0);
    } else {
        var diffuse = vec3<f32>(0.0);
        var count = min(u32(frame.tone.w), MAX_LIGHTS);
        if (object.flags.y > 0.5) {
            count = min(count, 1u);
        }
        for (var i = 0u; i < count; i++) {
            let data1 = frame.lights[i * 2u];
            let data2 = frame.lights[i * 2u + 1u];
            var attenuation = 1.0;
            var to_light = frame.sun_direction.xyz;
            if (i > 0u) {
                attenuation = max(0.0, 1.0 - distance(out.position, data1.xyz) / data1.w);
                to_light = normalize(data1.xyz - out.position);
            }
            diffuse += data2.rgb * max(0.0, dot(out.normal, to_light)) * attenuation;
        }
        out.diffuse = diffuse;
    }

    out.color = in.color * object.blend_color;
    out.uv = in.uv;
    out.clip = frame.view_proj * world;
    return out;
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

fn fog_amount(position: vec3<f32>) -> f32 {
    if (frame.fog.x < 0.5 || object.flags.z < 0.5) {
        return 0.0;
    }
    let depth = distance(position, frame.camera_position.xyz);
    var fog = clamp(1.0 - (frame.fog.y - depth) / frame.fog.z, 0.0, 1.0);
    fog *= clamp(1.0 - (position.z - frame.fog.w) / frame.fog.z, 0.0, 1.0);
    return fog;
}

// ACES fit by Stephen Hill.
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = in.color * textureSample(base_texture, base_sampler, in.uv);
    if (base.a == 0.0) {
        discard;
    }

    let gamma = frame.tone.z;
    let shaded = in.diffuse.r >= 0.0;

    // Material from uniforms; per-vertex emissive from blocks.
    let roughness = object.material.y;
    let metallic = object.material.x;
    let emissive = max(object.material.z, in.custom.z);
    let f0 = metallic;

    // Fresnel
    let n = in.normal;
    let v = normalize(frame.camera_position.xyz - in.position);
    let h = normalize(v + -reflect(v, n));
    var fresnel = fresnel_schlick_roughness(max(dot(h, v), 0.0), f0, roughness);

    var diffuse = vec3<f32>(1.0);
    if (shaded) {
        diffuse = in.diffuse + frame.ambient_color.rgb;
    } else {
        fresnel = 0.0;
    }
    diffuse = max(vec3<f32>(0.0), diffuse * (1.0 - fresnel));

    var col = base;
    if (object.material.w > 0.5) {
        col = clamp(base + object.rgb_add - object.rgb_sub, vec4<f32>(0.0), vec4<f32>(1.0));
        col = hsb_to_rgb(clamp(rgb_to_hsb(col) + object.hsb_add - object.hsb_sub, vec4<f32>(0.0), vec4<f32>(1.0)) * object.hsb_mul);
        col = mix(col, object.mix_color, object.mix_color.a);
    }

    var rgb = col.rgb;
    if (shaded) {
        rgb = pow(rgb, vec3<f32>(gamma));
    }

    let specular = mix(vec3<f32>(1.0), rgb, metallic) * pow(frame.fallback_color.rgb, vec3<f32>(gamma)) * fresnel;
    diffuse = diffuse * (1.0 - metallic) + emissive;
    rgb = rgb * diffuse + specular;

    if (shaded) {
        rgb *= frame.tone.y;
        if (frame.tone.x > 1.5) {
            rgb = map_aces(rgb);
        } else if (frame.tone.x > 0.5) {
            rgb = rgb / (1.0 + rgb);
        }
        rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma));
    }

    rgb = mix(rgb, frame.fog_color.rgb, fog_amount(in.position));
    let alpha = mix(base.a, 1.0, fresnel);
    return vec4<f32>(rgb, alpha);
}
