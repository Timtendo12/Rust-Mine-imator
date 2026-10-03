//! Effects on the finished picture of the high quality mode
//! (`render_post`): the effects of the camera the scene is seen through.

use crate::environment::Rgb;
use crate::renderer::Renderer;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Format the effects work in.
pub(crate) const POST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Pictures the effects pass the image between.
pub(crate) const POST_TEXTURES: usize = 5;

/// Glow around what is bright (`render_high_bloom`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bloom {
    /// What is brighter than this glows.
    pub threshold: f32,
    pub radius: f32,
    pub intensity: f32,
    /// Share of the glow that goes into streaks instead of a round glow.
    pub ratio: f32,
    pub blend: Rgb,
    /// Aperture blades, which give the streaks, and how they are turned.
    pub blade_amount: f32,
    pub blade_angle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaticAberration {
    pub blur_amount: f32,
    /// How far red, green and blue are moved apart.
    pub offsets: [f32; 3],
    /// Bend the channels like a lens instead of scaling them.
    pub distort_channels: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distort {
    pub amount: f32,
    /// Repeat the picture where the lens looks past its edge.
    pub repeat: bool,
    pub zoom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorCorrection {
    /// 0 leaves the contrast as it is.
    pub contrast: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub color_burn: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grain {
    pub strength: f32,
    pub saturation: f32,
    pub size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vignette {
    pub radius: f32,
    pub softness: f32,
    pub strength: f32,
    pub color: Rgb,
}

/// The effects of a camera, in the order they are applied.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PostEffects {
    pub bloom: Option<Bloom>,
    pub chromatic_aberration: Option<ChromaticAberration>,
    pub distort: Option<Distort>,
    pub color_correction: Option<ColorCorrection>,
    pub grain: Option<Grain>,
    pub vignette: Option<Vignette>,
}

impl PostEffects {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// The weights and offsets of the blur (`render_generate_gaussian_kernel`
/// for 19 samples): binomial weights, the middle one first, with offsets
/// from -1 to 1 that skip the middle.
pub fn blur_kernel() -> [[f32; 2]; 19] {
    let mut row = [0.0f64; 19];
    row[0] = 1.0;
    for n in 1..19 {
        for k in (1..=n).rev() {
            row[k] += row[k - 1];
        }
    }
    let sum: f64 = row.iter().sum();
    let weight = |k: usize| (row[k] / sum) as f32;
    let mut kernel = [[0.0; 2]; 19];
    kernel[0] = [weight(9), 0.0];
    for (i, entry) in kernel.iter_mut().enumerate().skip(1) {
        // The first nine are left of the middle, the rest right of it.
        let (source, offset) = if i <= 9 { (i - 1, i as f32 - 10.0) } else { (i, i as f32 - 9.0) };
        *entry = [weight(source), offset / 9.0];
    }
    kernel
}

/// How many blur directions the streaks of a bloom have
/// (`render_high_bloom`): half the blades, or all of them when their
/// number is odd.
pub fn streak_count(blade_amount: f32) -> u32 {
    let half = (blade_amount / 2.0).max(1.0);
    if half.fract() > 0.0 {
        blade_amount.max(0.0) as u32
    } else {
        half as u32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    size: [f32; 4],
    kernel: [[f32; 4]; 19],
}

/// A pass of the chain.
#[derive(Clone, Copy)]
enum Effect {
    Copy,
    Threshold,
    Blur,
    Add,
    Ca,
    Distort,
    ColorCorrection,
    Grain,
    Vignette,
}

const EFFECTS: [(Effect, &str); 9] = [
    (Effect::Copy, "fs_copy"),
    (Effect::Threshold, "fs_threshold"),
    (Effect::Blur, "fs_blur"),
    (Effect::Add, "fs_add"),
    (Effect::Ca, "fs_ca"),
    (Effect::Distort, "fs_distort"),
    (Effect::ColorCorrection, "fs_cc"),
    (Effect::Grain, "fs_grain"),
    (Effect::Vignette, "fs_vignette"),
];

/// Pipelines of the effect chain.
pub(crate) struct Post {
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    present: wgpu::RenderPipeline,
    clamped: wgpu::Sampler,
    repeating: wgpu::Sampler,
    kernel: [[f32; 4]; 19],
    /// The grain's noise: its size, what it was seeded with, its view.
    noise: Option<(u32, u32, wgpu::TextureView)>,
}

/// A small generator for the grain's noise.
fn noise_pixels(size: u32, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453) | 1;
    let mut next = || {
        // xorshift
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 24) as u8
    };
    (0..size * size).flat_map(|_| [next(), next(), next(), 255]).collect()
}

impl Post {
    pub(crate) fn new(renderer: &Renderer) -> Self {
        let device = &renderer.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<PostUniform>() as u64),
                    },
                    count: None,
                },
                texture(1),
                texture(2),
                sampler(3),
                sampler(4),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry: &str, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let pipelines = EFFECTS.iter().map(|(_, entry)| pipeline(entry, POST_FORMAT)).collect();
        let present = pipeline("fs_present", renderer.target_format);
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post sampler"),
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        Self {
            layout,
            pipelines,
            present,
            clamped: sampler(wgpu::AddressMode::ClampToEdge),
            repeating: sampler(wgpu::AddressMode::Repeat),
            kernel: blur_kernel().map(|[weight, offset]| [weight, offset, 0.0, 0.0]),
            noise: None,
        }
    }

    fn bind(&self, renderer: &Renderer, uniform: &PostUniform, source: &wgpu::TextureView, extra: &wgpu::TextureView) -> wgpu::BindGroup {
        let buffer = renderer.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post uniforms"),
            contents: bytemuck::bytes_of(uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        renderer.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(source) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(extra) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.clamped) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.repeating) },
            ],
        })
    }

    /// The noise of the grain: `size` pixels wide, different for every
    /// `seed`.
    fn noise(&mut self, renderer: &Renderer, size: u32, seed: u32) -> &wgpu::TextureView {
        if !self.noise.as_ref().is_some_and(|(have, seeded, _)| *have == size && *seeded == seed) {
            let texture = renderer.device.create_texture_with_data(
                &renderer.queue,
                &wgpu::TextureDescriptor {
                    label: Some("grain noise"),
                    size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &noise_pixels(size, seed),
            );
            self.noise = Some((size, seed, texture.create_view(&wgpu::TextureViewDescriptor::default())));
        }
        &self.noise.as_ref().expect("made above").2
    }

    /// Applies `effects` to the picture in `textures[0]`, using the other
    /// textures to work in. Returns the index of the texture the result
    /// is in. `seed` varies the grain from frame to frame.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply(
        &mut self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        textures: &[wgpu::TextureView; POST_TEXTURES],
        (width, height): (u32, u32),
        effects: &PostEffects,
        seed: u32,
    ) -> usize {
        let size = [width as f32, height as f32, 0.0, 0.0];
        let kernel = self.kernel;
        let uniform = |a: [f32; 4], b: [f32; 4]| PostUniform { a, b, c: [0.0; 4], size, kernel };
        let rgb = |c: Rgb| [c[0], c[1], c[2], 1.0];
        let run = |post: &Post,
                   encoder: &mut wgpu::CommandEncoder,
                   effect: Effect,
                   uniform: PostUniform,
                   source: &wgpu::TextureView,
                   extra: &wgpu::TextureView,
                   to: &wgpu::TextureView| {
            let bind = post.bind(renderer, &uniform, source, extra);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post effect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: to,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&post.pipelines[effect as usize]);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        };

        // The picture moves between textures 0 and 1; 2 to 4 are for bloom.
        let mut current = 0;
        let step = |post: &Post, encoder: &mut wgpu::CommandEncoder, effect: Effect, u: PostUniform, extra: usize, current: &mut usize| {
            let next = 1 - *current;
            run(post, encoder, effect, u, &textures[*current], &textures[extra], &textures[next]);
            *current = next;
        };

        if let Some(bloom) = &effects.bloom {
            let (bright, blurred, scratch) = (2, 3, 4);
            let radius = bloom.radius * 10.0 * height as f32 / 500.0;
            run(self, encoder, Effect::Threshold, uniform([bloom.threshold, 0.0, 0.0, 0.0], [0.0; 4]), &textures[current], &textures[current], &textures[bright]);
            let pass_radius = |i: u32| radius / (1.0 + 1.333 * i as f32);

            // Streaks along the aperture's blades.
            let blades = streak_count(bloom.blade_amount);
            if bloom.ratio > 0.0 && blades > 0 {
                let turn = std::f32::consts::TAU / (360.0 / bloom.blade_angle);
                for blade in 0..blades {
                    let angle = (180.0 / blades as f32 * blade as f32).to_radians() + if turn.is_finite() { turn } else { 0.0 };
                    run(self, encoder, Effect::Copy, uniform([0.0; 4], [0.0; 4]), &textures[bright], &textures[bright], &textures[scratch]);
                    for i in 0..3 {
                        let along = uniform([pass_radius(i), angle.cos(), angle.sin(), 0.0], [0.0; 4]);
                        run(self, encoder, Effect::Blur, along, &textures[scratch], &textures[scratch], &textures[blurred]);
                        run(self, encoder, Effect::Copy, uniform([0.0; 4], [0.0; 4]), &textures[blurred], &textures[blurred], &textures[scratch]);
                    }
                    let strength = 1.0 / blades as f32 * bloom.ratio * bloom.intensity;
                    step(self, encoder, Effect::Add, uniform([strength, 0.0, 0.0, 0.0], rgb(bloom.blend)), blurred, &mut current);
                }
            }
            // The round glow. As in the original, the last of its blurs is
            // only applied sideways.
            if bloom.ratio < 1.0 {
                run(self, encoder, Effect::Copy, uniform([0.0; 4], [0.0; 4]), &textures[bright], &textures[bright], &textures[scratch]);
                for i in 0..3 {
                    let sideways = uniform([pass_radius(i), 1.0, 0.0, 0.0], [0.0; 4]);
                    let upright = uniform([pass_radius(i), 0.0, 1.0, 0.0], [0.0; 4]);
                    run(self, encoder, Effect::Blur, sideways, &textures[scratch], &textures[scratch], &textures[blurred]);
                    run(self, encoder, Effect::Blur, upright, &textures[blurred], &textures[blurred], &textures[scratch]);
                }
                let strength = (1.0 - bloom.ratio) * bloom.intensity;
                step(self, encoder, Effect::Add, uniform([strength, 0.0, 0.0, 0.0], rgb(bloom.blend)), blurred, &mut current);
            }
        }
        if let Some(ca) = &effects.chromatic_aberration {
            let u = uniform([ca.blur_amount, ca.distort_channels as u8 as f32, 0.0, 0.0], [ca.offsets[0], ca.offsets[1], ca.offsets[2], 0.0]);
            step(self, encoder, Effect::Ca, u, current, &mut current);
        }
        if let Some(distort) = &effects.distort {
            let u = uniform([distort.amount, distort.repeat as u8 as f32, distort.zoom.max(0.0001), 0.0], [0.0; 4]);
            step(self, encoder, Effect::Distort, u, current, &mut current);
        }
        if let Some(cc) = &effects.color_correction {
            let u = uniform([cc.contrast + 1.0, cc.brightness, cc.saturation, cc.vibrance], rgb(cc.color_burn));
            step(self, encoder, Effect::ColorCorrection, u, current, &mut current);
        }
        if let Some(grain) = &effects.grain {
            let noise_size = (width.div_ceil(8)).max(height.div_ceil(8)).max(1);
            let cell = noise_size as f32 * grain.size.max(0.0001);
            let u = uniform([grain.strength, grain.saturation, cell, cell], [0.0; 4]);
            let noise = self.noise(renderer, noise_size, seed).clone();
            let next = 1 - current;
            run(self, encoder, Effect::Grain, u, &textures[current], &noise, &textures[next]);
            current = next;
        }
        if let Some(vignette) = &effects.vignette {
            let u = uniform([vignette.radius, vignette.softness, vignette.strength, 0.0], rgb(vignette.color));
            step(self, encoder, Effect::Vignette, u, current, &mut current);
        }
        current
    }

    /// Draws a picture into a viewport of the target.
    pub(crate) fn present(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        picture: &wgpu::TextureView,
        color: &wgpu::TextureView,
        viewport: crate::renderer::Viewport,
    ) {
        let uniform = PostUniform {
            a: [viewport.x as f32, viewport.y as f32, 0.0, 0.0],
            b: [0.0; 4],
            c: [0.0; 4],
            size: [viewport.width as f32, viewport.height as f32, 0.0, 0.0],
            kernel: self.kernel,
        };
        let bind = self.bind(renderer, &uniform, picture, picture);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("show picture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(viewport.x as f32, viewport.y as f32, viewport.width as f32, viewport.height as f32, 0.0, 1.0);
        pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        pass.set_pipeline(&self.present);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blur_kernel_is_a_bell_around_the_middle() {
        let kernel = blur_kernel();
        let total: f32 = kernel.iter().map(|k| k[0]).sum();
        assert!((total - 1.0).abs() < 1e-5);
        // The middle sample comes first and weighs most.
        assert_eq!(kernel[0][1], 0.0);
        assert!(kernel.iter().skip(1).all(|k| k[0] < kernel[0][0]));
        // Offsets run from -1 to 1 without the middle, weights mirror.
        assert_eq!((kernel[1][1], kernel[9][1], kernel[10][1], kernel[18][1]), (-1.0, -1.0 / 9.0, 1.0 / 9.0, 1.0));
        assert!((kernel[1][0] - kernel[18][0]).abs() < 1e-9 && (kernel[9][0] - kernel[10][0]).abs() < 1e-9);
        assert!(kernel[9][0] > kernel[1][0]);
    }

    #[test]
    fn streaks_follow_the_aperture_blades() {
        // An even number of blades gives half as many streaks (each blade
        // lines up with the one opposite), an odd number one each.
        assert_eq!(streak_count(6.0), 3);
        assert_eq!(streak_count(5.0), 5);
        assert_eq!(streak_count(0.0), 1);
        assert_eq!(streak_count(2.0), 1);
    }

    #[test]
    fn noise_differs_by_seed_and_uses_its_range() {
        let a = noise_pixels(16, 1);
        assert_eq!(a.len(), 16 * 16 * 4);
        assert_ne!(a, noise_pixels(16, 2));
        assert_eq!(a, noise_pixels(16, 1));
        let (low, high) = a.chunks(4).fold((255u8, 0u8), |(l, h), p| (l.min(p[0]), h.max(p[0])));
        assert!(low < 40 && high > 215);
        assert!(!PostEffects::default().any());
    }
}
