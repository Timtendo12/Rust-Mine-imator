//! The high quality mode (`render_high`): the scene is drawn many times,
//! each time shifted within a pixel and with the sun moved a little within
//! its disc, and the samples are averaged. That smooths edges and softens
//! shadows the longer the view stays the same.
//!
//! Ported so far: sun shadows in three cascades, point and spot lights
//! with their shadows, lighting per pixel with specular highlights, and
//! the sample average. See
//! `docs/PORTING_STATUS.md` for what is missing.

use crate::camera::Camera;
use crate::post::{Post, POST_FORMAT, POST_TEXTURES};
use crate::renderer::{FrameUniform, Renderer, Viewport, DEPTH_FORMAT};
use crate::scene::{Layer, PointLight, RenderObject, RenderScene};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use mi_mesh::Vertex;

/// Format a sample is drawn in.
const SAMPLE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format the samples are averaged in.
const GATHER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Number of shadow cascades (`render_cascades_count`).
pub const CASCADES: usize = 3;
/// Where the cascades end, as shares of the shadowed distance
/// (`render_cascade_ends`).
const CASCADE_ENDS: [f32; CASCADES + 1] = [0.0, 0.035, 0.15, 1.0];
/// Shadows reach at most this far from the camera.
const SHADOW_DISTANCE: f32 = 7500.0;

/// Lights the high quality mode shades with.
pub const MAX_LIGHTS: usize = 64;
/// Spot lights that can cast shadows at once; further ones shine without.
/// (The original has no limit: it draws every light in a pass of its own.)
pub const SPOT_SHADOWS: usize = 8;
/// Point lights that can cast shadows at once.
pub const POINT_SHADOWS: usize = 4;

/// Settings of the high quality mode, from the project's render settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighSettings {
    /// Samples to gather before the render is done.
    pub samples: u32,
    pub shadows: bool,
    /// Width and height of each cascade's shadow map.
    pub sun_buffer_size: u32,
    /// Shift samples within a pixel to smooth edges.
    pub antialiasing: bool,
    pub antialiasing_power: f32,
    /// Size of the sun's disc in degrees, which softens its shadows.
    pub sun_angle: f32,
    /// Width and height of the shadow map of a spot light, and of each
    /// side of a point light's.
    pub spot_buffer_size: u32,
    pub point_buffer_size: u32,
}

impl Default for HighSettings {
    fn default() -> Self {
        Self {
            samples: 24,
            shadows: true,
            sun_buffer_size: 2048,
            antialiasing: true,
            antialiasing_power: 1.0,
            sun_angle: 0.526,
            spot_buffer_size: 512,
            point_buffer_size: 256,
        }
    }
}

/// The `index`th number (from 1) of the Halton sequence with `base`.
pub fn halton(mut index: u32, base: u32) -> f32 {
    let (mut result, mut fraction) = (0.0, 1.0);
    while index > 0 {
        fraction /= base as f32;
        result += fraction * (index % base) as f32;
        index /= base;
    }
    result
}

/// One cascade of the sun's shadow: its view of the scene and where it ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cascade {
    pub view_proj: Mat4,
    /// Distance between the near and far plane of the cascade's view.
    pub depth_range: f32,
    /// View depth of the camera at which the cascade ends.
    pub end: f32,
}

/// The shadow cascades for a camera and the direction to the sun
/// (`render_update_cascades`): each covers a slice of what the camera sees
/// with a square view from the sun, snapped to its own pixels so that
/// shadows do not shimmer when the camera moves.
pub fn cascades(camera: &Camera, aspect: f32, to_sun: Vec3, map_size: u32) -> [Cascade; CASCADES] {
    let view = camera.view();
    let start = camera.near;
    let distance = camera.far.min(SHADOW_DISTANCE) - start;
    let up = if to_sun.cross(Vec3::Z).length_squared() < 1e-6 { Vec3::Y } else { Vec3::Z };
    let sun_view = Mat4::look_at_lh(to_sun, Vec3::ZERO, up);

    std::array::from_fn(|i| {
        let near = start + CASCADE_ENDS[i] * distance;
        let far = start + CASCADE_ENDS[i + 1] * distance;
        let slice = Mat4::perspective_lh(camera.fov.max(1.0).to_radians(), aspect, near, far) * view;
        let inverse = slice.inverse();
        let corners: Vec<Vec3> = (0..8)
            .map(|c| {
                let ndc = Vec4::new(if c & 1 == 0 { -1.0 } else { 1.0 }, if c & 2 == 0 { -1.0 } else { 1.0 }, (c >> 2) as f32, 1.0);
                let world = inverse * ndc;
                world.truncate() / world.w
            })
            .collect();

        // The slice as the sun sees it.
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for corner in &corners {
            let p = sun_view.transform_point3(*corner);
            min = min.min(p);
            max = max.max(p);
        }
        // Everything towards the sun can cast a shadow into the slice.
        min.z = -30000.0;
        max.z += 100.0;

        // A square as wide as the slice's longest diagonal keeps its size
        // when the camera turns.
        let mut diagonal = 0.0f32;
        for a in &corners {
            for b in &corners {
                diagonal = diagonal.max((*a - *b).length());
            }
        }
        for axis in 0..2 {
            let extra = diagonal - (max[axis] - min[axis]);
            if extra > 0.0 {
                max[axis] += extra / 2.0;
                min[axis] -= extra / 2.0;
            }
        }
        // Moving in whole pixels of the map keeps shadow edges still.
        let pixel = diagonal / map_size.max(1) as f32;
        if pixel > 0.0 {
            for axis in 0..2 {
                max[axis] = (max[axis] / pixel).round() * pixel;
                min[axis] = (min[axis] / pixel).round() * pixel;
            }
        }

        let projection = Mat4::orthographic_lh(min.x, max.x, min.y, max.y, min.z, max.z);
        Cascade { view_proj: projection * sun_view, depth_range: max.z - min.z, end: far }
    })
}

/// The direction to the sun for a sample: after the first samples it is
/// moved within the sun's disc, which softens the shadows
/// (`render_high_shadows`). The original draws the offsets from its random
/// generator; here they follow a Halton sequence.
pub fn sample_sun_direction(to_sun: Vec3, sample: u32, sun_angle: f32, render_distance: f32) -> Vec3 {
    if sample <= 1 {
        return to_sun;
    }
    let around = halton(sample, 5) * std::f32::consts::TAU;
    let tilt = (halton(sample, 7) * 2.0 - 1.0) * std::f32::consts::PI;
    let reach = (sun_angle * (render_distance / 2.0)) / 57.2958 / 2.0;
    let offset = Vec3::new(around.cos() * tilt.cos(), -around.sin() * tilt.cos(), tilt.sin()) * reach;
    (to_sun * 5000.0 - offset).normalize_or(to_sun)
}

/// The shift of a sample within its pixel, in clip space
/// (`render_high_update_taa`).
pub fn sample_jitter(sample: u32, width: u32, height: u32, power: f32) -> [f32; 2] {
    let x = 2.0 * halton(sample + 1, 2) - 1.0;
    let y = 2.0 * halton(sample + 1, 3) - 1.0;
    [x / width.max(1) as f32 * power, y / height.max(1) as f32 * power]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowUniform {
    matrices: [[f32; 16]; CASCADES],
    ends: [f32; 4],
    bias: [f32; 4],
    params: [f32; 4],
}

/// Where a light's shadow is cast from for a sample: after the first
/// samples it is moved within the light's size, which softens the shadow.
pub fn sample_light_position(position: Vec3, sample: u32, size: f32) -> Vec3 {
    if sample <= 1 {
        return position;
    }
    let around = halton(sample, 11) * std::f32::consts::TAU;
    let tilt = (halton(sample, 13) * 2.0 - 1.0) * std::f32::consts::PI;
    position + Vec3::new(around.cos() * tilt.cos(), -around.sin() * tilt.cos(), tilt.sin()) * (size / 2.0)
}

/// The views a point light's shadow is drawn with: the six sides of a cube
/// map, in the order and orientation cube maps are sampled in.
pub fn cube_views(from: Vec3, range: f32) -> [Mat4; 6] {
    let projection = Mat4::perspective_lh(std::f32::consts::FRAC_PI_2, 1.0, 1.0, range.max(2.0));
    let sides = [
        (Vec3::X, Vec3::Y),
        (-Vec3::X, Vec3::Y),
        (Vec3::Y, -Vec3::Z),
        (-Vec3::Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (-Vec3::Z, Vec3::Y),
    ];
    sides.map(|(look, up)| projection * Mat4::look_at_lh(from, from + look, up))
}

/// The view of a spot light from `from`.
pub fn spot_view(from: Vec3, light: &PointLight) -> Mat4 {
    let cone = light.spot.expect("a spot light");
    let position = Vec3::from(light.position);
    let direction = (Vec3::from(cone.to) - position).normalize_or(Vec3::Y);
    let up = if direction.cross(Vec3::Z).length_squared() < 1e-6 { Vec3::Y } else { Vec3::Z };
    let projection = Mat4::perspective_lh(cone.radius.clamp(1.0, 179.0).to_radians(), 1.0, 1.0, light.range.max(2.0));
    projection * Mat4::look_at_lh(from, from + direction, up)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightUniform {
    position: [f32; 4],
    color: [f32; 4],
    params: [f32; 4],
    shadow_position: [f32; 4],
    cone: [f32; 16],
    shadow: [f32; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightsUniform {
    count: [f32; 4],
    lights: [LightUniform; MAX_LIGHTS],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ResolveUniform {
    params: [f32; 4],
}

/// The textures of a view being rendered progressively.
struct Target {
    width: u32,
    height: u32,
    sample: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// Two textures that take turns holding the average.
    gathered: [wgpu::TextureView; 2],
    /// Pictures the effects of the camera work in.
    post: [wgpu::TextureView; POST_TEXTURES],
    /// Samples gathered so far.
    count: u32,
    /// What the samples are of; a change starts over.
    scene: Option<(RenderScene, HighSettings)>,
}

/// What the high quality mode adds to the renderer.
pub(crate) struct High {
    pipeline_cull: wgpu::RenderPipeline,
    pipeline_two_sided: wgpu::RenderPipeline,
    pipeline_sky: wgpu::RenderPipeline,
    pipeline_sky_add: wgpu::RenderPipeline,
    pipeline_shadow: wgpu::RenderPipeline,
    pipeline_gather: wgpu::RenderPipeline,
    pipeline_show: wgpu::RenderPipeline,
    /// Brings the average into the format the effects work in.
    pipeline_import: wgpu::RenderPipeline,
    post: Post,
    shadow_layout: wgpu::BindGroupLayout,
    resolve_layout: wgpu::BindGroupLayout,
    shadow_buffer: wgpu::Buffer,
    shadow_sampler: wgpu::Sampler,
    lights_buffer: wgpu::Buffer,
    point_sampler: wgpu::Sampler,
    shadow_maps: Option<ShadowMaps>,
    /// A frame uniform and its bind group per view a shadow map is drawn
    /// from: the cascades, then the spot lights, then the sides of the
    /// point lights.
    shadow_frames: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// Views being rendered, by size: the viewport and an export can go on
    /// side by side.
    targets: Vec<Target>,
}

/// The shadow maps: of the sun's cascades, of spot lights and of point
/// lights, with the group they are bound with.
struct ShadowMaps {
    sizes: [u32; 3],
    sun: [wgpu::TextureView; CASCADES],
    spot: Vec<wgpu::TextureView>,
    /// Six sides per point light.
    point: Vec<wgpu::TextureView>,
    bind: wgpu::BindGroup,
}

const ATTRIBUTES: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4];

impl High {
    fn new(renderer: &Renderer) -> Self {
        let device = &renderer.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high quality shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/high.wgsl").into()),
        });
        let world_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });
        let resolve_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("resolve shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/resolve.wgsl").into()),
        });

        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadows"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<ShadowUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<LightsUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high quality"),
            bind_group_layouts: &[&renderer.frame_layout, &renderer.object_layout, &renderer.texture_layout, &shadow_layout],
            push_constant_ranges: &[],
        });
        let plain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high quality, without shadows"),
            bind_group_layouts: &[&renderer.frame_layout, &renderer.object_layout, &renderer.texture_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }];
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        // A pipeline that draws geometry into a sample.
        let geometry = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        module: &wgpu::ShaderModule,
                        cull_mode: Option<wgpu::Face>,
                        backdrop: bool,
                        blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &vertex_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: !backdrop,
                    depth_compare: if backdrop { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::LessEqual },
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: SAMPLE_FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let alpha = wgpu::BlendState::ALPHA_BLENDING;
        let pipeline_cull = geometry("high, culled", &world_layout, &shader, Some(wgpu::Face::Back), false, alpha);
        let pipeline_two_sided = geometry("high, two-sided", &world_layout, &shader, None, false, alpha);
        let pipeline_sky = geometry("high, sky", &plain_layout, &world_shader, None, true, alpha);
        let pipeline_sky_add =
            geometry("high, sky, additive", &plain_layout, &world_shader, None, true, wgpu::BlendState { color: add, alpha: add });

        let pipeline_shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sun shadow"),
            layout: Some(&plain_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            // Both sides cast shadows.
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview: None,
            cache: None,
        });

        let unfiltered = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resolve"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<ResolveUniform>() as u64),
                    },
                    count: None,
                },
                unfiltered(1),
                unfiltered(2),
            ],
        });
        let resolve_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("resolve"),
            bind_group_layouts: &[&resolve_layout],
            push_constant_ranges: &[],
        });
        let fullscreen = |label: &str, entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&resolve_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &resolve_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &resolve_shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let pipeline_gather = fullscreen("gather samples", "fs_gather", GATHER_FORMAT);
        let pipeline_show = fullscreen("show samples", "fs_show", renderer.target_format);
        let pipeline_import = fullscreen("import samples", "fs_show", POST_FORMAT);

        let shadow_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow uniforms"),
            size: size_of::<ShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light uniforms"),
            size: size_of::<LightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let point_sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("point shadow sampler"), ..Default::default() });
        let shadow_frames = (0..CASCADES + SPOT_SHADOWS + POINT_SHADOWS * 6).map(|_| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow view frame uniforms"),
                size: size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow view frame"),
                layout: &renderer.frame_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
            });
            (buffer, bind)
        });
        let shadow_frames = shadow_frames.collect();

        Self {
            pipeline_cull,
            pipeline_two_sided,
            pipeline_sky,
            pipeline_sky_add,
            pipeline_shadow,
            pipeline_gather,
            pipeline_show,
            pipeline_import,
            post: Post::new(renderer),
            shadow_layout,
            resolve_layout,
            shadow_buffer,
            shadow_sampler,
            lights_buffer,
            point_sampler,
            shadow_maps: None,
            shadow_frames,
            targets: Vec::new(),
        }
    }

    /// The shadow maps in the given sizes (sun, spot, point).
    fn ensure_shadow_maps(&mut self, device: &wgpu::Device, sizes: [u32; 3]) {
        if self.shadow_maps.as_ref().is_some_and(|maps| maps.sizes == sizes) {
            return;
        }
        let texture = |label, size: u32, layers: usize| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers as u32 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let layer = |texture: &wgpu::Texture, layer: usize| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        };
        let whole = |texture: &wgpu::Texture, dimension| {
            texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(dimension), ..Default::default() })
        };
        let sun = texture("sun shadow maps", sizes[0], CASCADES);
        let spot = texture("spot light shadow maps", sizes[1], SPOT_SHADOWS);
        let point = texture("point light shadow maps", sizes[2], POINT_SHADOWS * 6);
        let (sun_all, spot_all, point_all) = (
            whole(&sun, wgpu::TextureViewDimension::D2Array),
            whole(&spot, wgpu::TextureViewDimension::D2Array),
            whole(&point, wgpu::TextureViewDimension::CubeArray),
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadows"),
            layout: &self.shadow_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.shadow_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&sun_all) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.shadow_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: self.lights_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&spot_all) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&point_all) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&self.point_sampler) },
            ],
        });
        self.shadow_maps = Some(ShadowMaps {
            sizes,
            sun: std::array::from_fn(|i| layer(&sun, i)),
            spot: (0..SPOT_SHADOWS).map(|i| layer(&spot, i)).collect(),
            point: (0..POINT_SHADOWS * 6).map(|i| layer(&point, i)).collect(),
            bind,
        });
    }

    /// The textures of a view of the given size; at most two views are kept.
    fn target(&mut self, renderer: &Renderer, width: u32, height: u32) -> usize {
        if let Some(index) = self.targets.iter().position(|t| t.width == width && t.height == height) {
            return index;
        }
        let device = &renderer.device;
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let target = Target {
            width,
            height,
            sample: texture("sample", SAMPLE_FORMAT),
            depth: renderer.create_depth_view(width, height),
            gathered: [texture("gathered samples", GATHER_FORMAT), texture("gathered samples", GATHER_FORMAT)],
            post: std::array::from_fn(|_| texture("post effects", POST_FORMAT)),
            count: 0,
            scene: None,
        };
        if self.targets.len() >= 2 {
            self.targets.remove(0);
        }
        self.targets.push(target);
        self.targets.len() - 1
    }
}

/// A shadow map to draw into.
#[derive(Clone, Copy)]
enum MapTarget {
    Sun(usize),
    Spot(usize),
    Point(usize),
}

/// Whether an object casts a shadow: solid things do; the sky, the clouds
/// and the ground do not (`render_world` leaves them out of the depth
/// passes), nor does what is only there to be clicked.
fn casts_shadow(object: &RenderObject) -> bool {
    object.layer == Layer::World && object.shadows && !object.unlit && !object.sun_only && !object.pick_only
}

impl Renderer {
    /// How many samples of `scene` have been gathered for a view of this
    /// size.
    pub fn high_samples(&self, width: u32, height: u32) -> u32 {
        self.high
            .as_ref()
            .and_then(|high| high.targets.iter().find(|t| t.width == width && t.height == height))
            .map_or(0, |target| target.count)
    }

    /// Draws `scene` in high quality into `viewport` of `color`: one more
    /// sample is gathered, unless there are enough already, and the average
    /// so far is shown. Returns whether more samples are wanted. Samples
    /// start over when the scene, the settings or the size change.
    pub fn render_high(&mut self, color: &wgpu::TextureView, viewport: Viewport, scene: &RenderScene, settings: &HighSettings) -> bool {
        if viewport.width == 0 || viewport.height == 0 {
            return false;
        }
        let mut high = self.high.take().unwrap_or_else(|| High::new(self));
        let (width, height) = (viewport.width, viewport.height);
        let index = high.target(self, width, height);
        let wanted = settings.samples.max(1);
        {
            let target = &mut high.targets[index];
            if !target.scene.as_ref().is_some_and(|(s, h)| s == scene && h == settings) {
                target.scene = Some((scene.clone(), *settings));
                target.count = 0;
            }
        }
        let count = high.targets[index].count;
        let drawable = self.upload_objects(scene, |o| !o.pick_only);
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("high quality") });

        if count < wanted {
            self.draw_sample(&mut high, index, &mut encoder, scene, settings, &drawable);
            high.targets[index].count += 1;
        }

        // Show the average, through the effects of the camera if it has any.
        let shown_index = (high.targets[index].count % 2) as usize;
        let with_effects = scene.post.any();
        {
            let target = &high.targets[index];
            let shown = &target.gathered[shown_index];
            let (to, offset, pipeline) = if with_effects {
                (&target.post[0], [0.0, 0.0], &high.pipeline_import)
            } else {
                (color, [viewport.x as f32, viewport.y as f32], &high.pipeline_show)
            };
            let uniform = ResolveUniform { params: [0.0, offset[0], offset[1], 0.0] };
            let bind = self.resolve_bind(&high, &uniform, shown, &target.sample);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("show samples"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: to,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !with_effects {
                pass.set_viewport(viewport.x as f32, viewport.y as f32, width as f32, height as f32, 0.0, 1.0);
                pass.set_scissor_rect(viewport.x, viewport.y, width, height);
            }
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if with_effects {
            let High { post, targets, .. } = &mut high;
            let textures = &targets[index].post;
            // The grain changes with the animation's time.
            let seed = scene.wind.time as u32;
            let result = post.apply(self, &mut encoder, textures, (width, height), &scene.post, seed);
            post.present(self, &mut encoder, &textures[result], color, viewport);
        }

        if drawable.iter().any(|o| o.selected) {
            self.outline_selection(&mut encoder, color, viewport, &drawable);
        }
        self.queue.submit([encoder.finish()]);
        let more = high.targets[index].count < wanted;
        self.high = Some(high);
        more
    }

    fn resolve_bind(&self, high: &High, uniform: &ResolveUniform, gathered: &wgpu::TextureView, sample: &wgpu::TextureView) -> wgpu::BindGroup {
        use wgpu::util::DeviceExt;
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("resolve uniforms"),
            contents: bytemuck::bytes_of(uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolve"),
            layout: &high.resolve_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(gathered) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(sample) },
            ],
        })
    }

    /// Draws one sample and averages it into what was gathered.
    fn draw_sample(
        &mut self,
        high: &mut High,
        index: usize,
        encoder: &mut wgpu::CommandEncoder,
        scene: &RenderScene,
        settings: &HighSettings,
        drawable: &[&RenderObject],
    ) {
        let (width, height, count) = {
            let target = &high.targets[index];
            (target.width, target.height, target.count)
        };
        let aspect = width as f32 / height as f32;

        // The camera, shifted within the pixel.
        let mut frame = Self::frame_uniform(scene, aspect);
        if settings.antialiasing {
            let [x, y] = sample_jitter(count, width, height, settings.antialiasing_power);
            let jittered = Mat4::from_translation(Vec3::new(x, y, 0.0)) * scene.camera.view_projection(aspect);
            frame.view_proj = jittered.to_cols_array();
        }
        self.queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        // Shadows: the scene as the sun and the lights see it. Every view
        // gets a frame uniform of its own, as all are drawn in one go.
        let sun_up = scene.lighting.sun_color.iter().any(|&c| c > 0.0);
        let sizes = [
            settings.sun_buffer_size.clamp(16, 8192),
            settings.spot_buffer_size.clamp(16, 4096),
            settings.point_buffer_size.clamp(16, 2048),
        ];
        high.ensure_shadow_maps(&self.device, sizes);
        // (frame uniform, target of the maps) of every view to draw.
        let mut views: Vec<(usize, MapTarget)> = Vec::new();
        let view_frame = |slot: usize, view_proj: Mat4| {
            let mut uniform = frame;
            uniform.view_proj = view_proj.to_cols_array();
            self.queue.write_buffer(&high.shadow_frames[slot].0, 0, bytemuck::bytes_of(&uniform));
        };

        let mut shadow = ShadowUniform::zeroed();
        if settings.shadows && sun_up {
            let to_sun = sample_sun_direction(scene.lighting.sun_direction, count, settings.sun_angle, scene.camera.far);
            let all = cascades(&scene.camera, aspect, to_sun, sizes[0]);
            // Clip space to texture coordinates: y points down in textures.
            let to_texture = Mat4::from_translation(Vec3::new(0.5, 0.5, 0.0)) * Mat4::from_scale(Vec3::new(0.5, -0.5, 1.0));
            for (i, cascade) in all.iter().enumerate() {
                shadow.matrices[i] = (to_texture * cascade.view_proj).to_cols_array();
                shadow.ends[i] = cascade.end;
                // One unit for the nearest cascade, more for the coarser ones.
                shadow.bias[i] = (1.0 + i as f32 * 2.0) / cascade.depth_range;
                view_frame(i, cascade.view_proj);
                views.push((i, MapTarget::Sun(i)));
            }
            shadow.params[0] = 1.0;
        }
        self.queue.write_buffer(&high.shadow_buffer, 0, bytemuck::bytes_of(&shadow));

        // Point and spot lights; the first ones that cast shadows get a map.
        let mut lights = LightsUniform::zeroed();
        let (mut spots, mut points) = (0, 0);
        for (i, light) in scene.lights.iter().take(MAX_LIGHTS).enumerate() {
            let position = Vec3::from(light.position);
            let from = sample_light_position(position, count, light.size);
            let mut uniform = LightUniform {
                position: [position.x, position.y, position.z, light.range],
                color: [light.color[0], light.color[1], light.color[2], light.fade_size],
                params: [light.specular, 0.0, 0.0, -1.0],
                shadow_position: [from.x, from.y, from.z, 0.0],
                cone: Mat4::IDENTITY.to_cols_array(),
                shadow: Mat4::IDENTITY.to_cols_array(),
            };
            let shadows = settings.shadows && light.shadows;
            match light.spot {
                Some(cone) => {
                    uniform.params[1] = cone.sharpness;
                    uniform.params[2] = 1.0;
                    uniform.cone = spot_view(position, light).to_cols_array();
                    if shadows && spots < SPOT_SHADOWS {
                        let view = spot_view(from, light);
                        uniform.shadow = view.to_cols_array();
                        uniform.params[3] = spots as f32;
                        view_frame(CASCADES + spots, view);
                        views.push((CASCADES + spots, MapTarget::Spot(spots)));
                        spots += 1;
                    }
                }
                None if shadows && points < POINT_SHADOWS => {
                    uniform.params[3] = points as f32;
                    for (side, view) in cube_views(from, light.range).into_iter().enumerate() {
                        let slot = CASCADES + SPOT_SHADOWS + points * 6 + side;
                        view_frame(slot, view);
                        views.push((slot, MapTarget::Point(points * 6 + side)));
                    }
                    points += 1;
                }
                None => {}
            }
            lights.lights[i] = uniform;
        }
        lights.count[0] = scene.lights.len().min(MAX_LIGHTS) as f32;
        self.queue.write_buffer(&high.lights_buffer, 0, bytemuck::bytes_of(&lights));

        let maps = high.shadow_maps.as_ref().expect("made above");
        let shadow_bind = &maps.bind;
        for (slot, map) in &views {
            let view = match *map {
                MapTarget::Sun(i) => &maps.sun[i],
                MapTarget::Spot(i) => &maps.spot[i],
                MapTarget::Point(i) => &maps.point[i],
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow map"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&high.pipeline_shadow);
            pass.set_bind_group(0, &high.shadow_frames[*slot].1, &[]);
            for (n, object) in drawable.iter().enumerate() {
                if !casts_shadow(object) {
                    continue;
                }
                let Some(mesh) = &self.meshes[object.mesh.0] else { continue };
                let texture = object.texture.and_then(|t| self.textures.get(t.0)).unwrap_or(&self.white);
                pass.set_bind_group(1, &self.object_bind, &[(n as u64 * self.object_stride) as u32]);
                pass.set_bind_group(2, texture, &[]);
                pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                pass.draw(0..mesh.vertex_count, 0..1);
            }
        }

        // The sample.
        let target = &high.targets[index];
        {
            let sky = scene.background;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("high quality sample"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.sample,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: sky[0] as f64, g: sky[1] as f64, b: sky[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.frame_bind, &[]);
            for (n, object) in drawable.iter().enumerate() {
                let Some(mesh) = &self.meshes[object.mesh.0] else { continue };
                let pipeline = match object.layer {
                    Layer::Sky => &high.pipeline_sky,
                    Layer::SkyAdd => &high.pipeline_sky_add,
                    Layer::World if object.backfaces => &high.pipeline_two_sided,
                    Layer::World => &high.pipeline_cull,
                };
                let texture = object.texture.and_then(|t| self.textures.get(t.0)).unwrap_or(&self.white);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &self.object_bind, &[(n as u64 * self.object_stride) as u32]);
                pass.set_bind_group(2, texture, &[]);
                if object.layer == Layer::World {
                    pass.set_bind_group(3, shadow_bind, &[]);
                }
                pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                pass.draw(0..mesh.vertex_count, 0..1);
            }
        }

        // Average it in: read one of the two textures, write the other.
        let (from, to) = (&target.gathered[(count % 2) as usize], &target.gathered[((count + 1) % 2) as usize]);
        let uniform = ResolveUniform { params: [count as f32, 0.0, 0.0, 0.0] };
        let bind = self.resolve_bind(high, &uniform, from, &target.sample);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gather samples"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: to,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&high.pipeline_gather);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera { from: Vec3::new(0.0, -100.0, 50.0), to: Vec3::ZERO, up: Vec3::Z, fov: 45.0, near: 1.0, far: 10000.0 }
    }

    #[test]
    fn halton_fills_the_unit_interval() {
        assert_eq!([halton(1, 2), halton(2, 2), halton(3, 2), halton(4, 2)], [0.5, 0.25, 0.75, 0.125]);
        assert!((halton(1, 3) - 1.0 / 3.0).abs() < 1e-6 && (halton(2, 3) - 2.0 / 3.0).abs() < 1e-6);
        // Samples are shifted by at most a pixel (2 / size in clip space).
        for sample in 0..64 {
            let [x, y] = sample_jitter(sample, 200, 100, 1.0);
            assert!(x.abs() <= 1.0 / 200.0 && y.abs() <= 1.0 / 100.0);
        }
        assert_ne!(sample_jitter(0, 200, 100, 1.0), sample_jitter(1, 200, 100, 1.0));
    }

    #[test]
    fn the_sun_moves_within_its_disc_after_the_first_samples() {
        let sun = Vec3::new(0.3, 0.2, 0.9).normalize();
        assert_eq!(sample_sun_direction(sun, 0, 0.526, 30000.0), sun);
        assert_eq!(sample_sun_direction(sun, 1, 0.526, 30000.0), sun);
        let mut widest = 0.0f32;
        for sample in 2..64 {
            let moved = sample_sun_direction(sun, sample, 0.526, 30000.0);
            assert!((moved.length() - 1.0).abs() < 1e-5);
            widest = widest.max(moved.angle_between(sun).to_degrees());
        }
        // Up to about 0.8 degrees with these settings, and never far off.
        assert!(widest > 0.2 && widest < 1.0, "{widest}");
        // No disc, no movement.
        assert!(sample_sun_direction(sun, 9, 0.0, 30000.0).angle_between(sun) < 1e-4);
    }

    #[test]
    fn cascades_cover_what_the_camera_sees() {
        let camera = camera();
        let to_sun = Vec3::new(0.4, -0.3, 0.8).normalize();
        let all = cascades(&camera, 16.0 / 9.0, to_sun, 2048);
        // They end at 3.5%, 15% and all of the shadowed distance.
        let distance = SHADOW_DISTANCE - 1.0;
        assert!((all[0].end - (1.0 + 0.035 * distance)).abs() < 0.01);
        assert!((all[2].end - SHADOW_DISTANCE).abs() < 0.01);
        // A point the camera looks at, near it, is inside the first
        // cascade's map; the cascades get coarser with distance.
        let near_point = Vec3::new(0.0, -80.0, 40.0);
        for cascade in &all {
            let clip = cascade.view_proj * near_point.extend(1.0);
            assert!(clip.x.abs() <= 1.0 && clip.y.abs() <= 1.0 && (0.0..=1.0).contains(&clip.z), "{clip:?}");
        }
        let width = |c: &Cascade| 2.0 / c.view_proj.x_axis.truncate().length();
        assert!(width(&all[0]) < width(&all[1]) && width(&all[1]) < width(&all[2]));
        // Depth grows away from the sun.
        let towards = all[0].view_proj * (near_point + to_sun * 100.0).extend(1.0);
        let here = all[0].view_proj * near_point.extend(1.0);
        assert!(towards.z < here.z);
        // The sun straight overhead still gives a usable view.
        let overhead = cascades(&camera, 1.0, Vec3::Z, 1024);
        assert!(overhead.iter().all(|c| c.view_proj.is_finite()));
    }

    #[test]
    fn turning_the_camera_keeps_the_cascade_size() {
        let to_sun = Vec3::new(0.4, -0.3, 0.8).normalize();
        let size = |to: Vec3| {
            let camera = Camera { to, ..camera() };
            let first = cascades(&camera, 1.5, to_sun, 2048)[1];
            2.0 / first.view_proj.x_axis.truncate().length()
        };
        let a = size(Vec3::ZERO);
        let b = size(Vec3::new(300.0, 100.0, 20.0));
        assert!((a - b).abs() / a < 0.01, "{a} {b}");
    }
}
