//! The wgpu renderer.
//!
//! Colours are computed the way the original does it: textures and targets
//! are not sRGB formats and the shader applies gamma itself, so a render
//! target must use a non-sRGB format such as `Rgba8Unorm` or `Bgra8Unorm`.

use mi_mesh::{MeshData, Vertex};
use crate::scene::{MeshId, RenderObject, RenderScene, TextureId, Tonemapper};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Depth buffer format used by all passes.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Format of the pick pass: one object id per pixel.
const PICK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

/// Maximum number of point lights in the shaded mode (the sun is separate).
pub const MAX_POINT_LIGHTS: usize = 63;

/// Area of the target to draw into, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [f32; 16],
    camera_position: [f32; 4],
    sun_direction: [f32; 4],
    ambient_color: [f32; 4],
    fallback_color: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    tone: [f32; 4],
    lights: [[f32; 4]; 128],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniform {
    model: [f32; 16],
    blend_color: [f32; 4],
    rgb_add: [f32; 4],
    rgb_sub: [f32; 4],
    hsb_add: [f32; 4],
    hsb_sub: [f32; 4],
    hsb_mul: [f32; 4],
    mix_color: [f32; 4],
    material: [f32; 4],
    flags: [f32; 4],
}

impl ObjectUniform {
    fn new(object: &RenderObject) -> Self {
        let rgb = |c: [f32; 3], a: f32| [c[0], c[1], c[2], a];
        let colors = object.colors.as_ref();
        Self {
            model: object.model,
            blend_color: object.blend_color,
            rgb_add: colors.map_or([0.0; 4], |c| rgb(c.rgb_add, 0.0)),
            rgb_sub: colors.map_or([0.0; 4], |c| rgb(c.rgb_sub, 0.0)),
            hsb_add: colors.map_or([0.0; 4], |c| rgb(c.hsb_add, 0.0)),
            hsb_sub: colors.map_or([0.0; 4], |c| rgb(c.hsb_sub, 0.0)),
            hsb_mul: colors.map_or([1.0; 4], |c| rgb(c.hsb_mul, 1.0)),
            mix_color: colors.map_or([0.0; 4], |c| rgb(c.mix_color, c.mix_percent)),
            material: [object.metallic, object.roughness, object.emissive, colors.is_some() as u8 as f32],
            flags: [object.unlit as u8 as f32, object.sun_only as u8 as f32, object.fog as u8 as f32, object.pick as f32],
        }
    }
}

struct GpuMesh {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

/// How a texture is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFilter {
    /// Blocky, as Minecraft textures are meant to look.
    Nearest,
    Linear,
}

/// Draws [`RenderScene`]s with wgpu.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline_cull: wgpu::RenderPipeline,
    pipeline_two_sided: wgpu::RenderPipeline,
    pick_cull: wgpu::RenderPipeline,
    pick_two_sided: wgpu::RenderPipeline,
    /// 1×1 targets of the pick pass and the buffer it is read into.
    pick_target: (wgpu::Texture, wgpu::TextureView, wgpu::TextureView, wgpu::Buffer),
    frame_buffer: wgpu::Buffer,
    frame_bind: wgpu::BindGroup,
    object_layout: wgpu::BindGroupLayout,
    object_buffer: wgpu::Buffer,
    object_bind: wgpu::BindGroup,
    object_capacity: usize,
    object_stride: u64,
    texture_layout: wgpu::BindGroupLayout,
    textures: Vec<wgpu::BindGroup>,
    white: wgpu::BindGroup,
    /// `None` for removed meshes; their slots are reused.
    meshes: Vec<Option<GpuMesh>>,
    free_meshes: Vec<usize>,
}

fn object_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, stride: u64, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("object uniforms"),
        size: stride * capacity as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("object uniforms"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(size_of::<ObjectUniform>() as u64),
            }),
        }],
    });
    (buffer, bind)
}

fn texture_bind(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    rgba: &[u8],
    width: u32,
    height: u32,
    filter: TextureFilter,
) -> wgpu::BindGroup {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mode = match filter {
        TextureFilter::Nearest => wgpu::FilterMode::Nearest,
        TextureFilter::Linear => wgpu::FilterMode::Linear,
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("texture sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: mode,
        min_filter: mode,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    })
}

impl Renderer {
    /// Creates the renderer for targets of the given colour format.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });

        let uniform_entry = |dynamic: bool, size: usize, visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: wgpu::BufferSize::new(size as u64),
            },
            count: None,
        };
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[uniform_entry(false, size_of::<FrameUniform>(), both)],
        });
        let object_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("object"),
            entries: &[uniform_entry(true, size_of::<ObjectUniform>(), both)],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[&frame_layout, &object_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4];
        let pick_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pick shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pick.wgsl").into()),
        });
        let pipeline = |cull_mode: Option<wgpu::Face>, label: &str, picking: bool| {
            let (shader, format, blend) = if picking {
                (&pick_shader, PICK_FORMAT, None)
            } else {
                (&shader, target_format, Some(wgpu::BlendState::ALPHA_BLENDING))
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // The world is left-handed, so front faces are clockwise.
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let pipeline_cull = pipeline(Some(wgpu::Face::Back), "world, culled", false);
        let pipeline_two_sided = pipeline(None, "world, two-sided", false);
        let pick_cull = pipeline(Some(wgpu::Face::Back), "pick, culled", true);
        let pick_two_sided = pipeline(None, "pick, two-sided", true);
        let pick_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pick target"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: PICK_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let pick_view = pick_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let pick_depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("pick depth"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let pick_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick readback"),
            size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniforms"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buffer.as_entire_binding() }],
        });

        let alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        let object_stride = (size_of::<ObjectUniform>() as u64).div_ceil(alignment) * alignment;
        let object_capacity = 256;
        let (object_buffer, object_bind) = object_buffer(device, &object_layout, object_stride, object_capacity);

        let white = texture_bind(device, queue, &texture_layout, &[255; 4], 1, 1, TextureFilter::Nearest);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline_cull,
            pipeline_two_sided,
            pick_cull,
            pick_two_sided,
            pick_target: (pick_texture, pick_view, pick_depth, pick_buffer),
            frame_buffer,
            frame_bind,
            object_layout,
            object_buffer,
            object_bind,
            object_capacity,
            object_stride,
            texture_layout,
            textures: Vec::new(),
            white,
            meshes: Vec::new(),
            free_meshes: Vec::new(),
        }
    }

    /// Uploads a mesh. Empty meshes are allowed and draw nothing.
    pub fn add_mesh(&mut self, mesh: &MeshData) -> MeshId {
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let gpu = GpuMesh { buffer, vertex_count: mesh.vertices.len() as u32 };
        match self.free_meshes.pop() {
            Some(slot) => {
                self.meshes[slot] = Some(gpu);
                MeshId(slot)
            }
            None => {
                self.meshes.push(Some(gpu));
                MeshId(self.meshes.len() - 1)
            }
        }
    }

    /// Frees a mesh. Its id may be handed out again by [`Renderer::add_mesh`].
    pub fn remove_mesh(&mut self, id: MeshId) {
        if let Some(slot) = self.meshes.get_mut(id.0) {
            if slot.take().is_some() {
                self.free_meshes.push(id.0);
            }
        }
    }

    /// Uploads an RGBA image of `width` × `height` pixels.
    ///
    /// # Panics
    /// If `rgba` does not hold exactly `width * height * 4` bytes or a
    /// dimension is zero.
    pub fn add_texture(&mut self, rgba: &[u8], width: u32, height: u32, filter: TextureFilter) -> TextureId {
        assert!(width > 0 && height > 0, "texture dimensions must not be zero");
        assert_eq!(rgba.len(), width as usize * height as usize * 4, "texture data does not match its size");
        let bind = texture_bind(&self.device, &self.queue, &self.texture_layout, rgba, width, height, filter);
        self.textures.push(bind);
        TextureId(self.textures.len() - 1)
    }

    /// Submits work recorded on the renderer's device.
    pub fn submit(&self, encoder: wgpu::CommandEncoder) {
        self.queue.submit([encoder.finish()]);
    }

    /// Creates a depth buffer for a target of the given size.
    pub fn create_depth_view(&self, width: u32, height: u32) -> wgpu::TextureView {
        self.device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn frame_uniform(scene: &RenderScene, aspect: f32) -> FrameUniform {
        let lighting = &scene.lighting;
        let rgb = |c: [f32; 3], a: f32| [c[0], c[1], c[2], a];
        let mut lights = [[0.0; 4]; 128];
        // The sun is light 0; its position is unused because its direction
        // is given separately.
        lights[1] = rgb(lighting.sun_color, 1.0);
        let point_lights = scene.lights.len().min(MAX_POINT_LIGHTS);
        for (i, light) in scene.lights.iter().take(point_lights).enumerate() {
            lights[(i + 1) * 2] = [light.position[0], light.position[1], light.position[2], light.range];
            lights[(i + 1) * 2 + 1] = rgb(light.color, 1.0);
        }
        let tonemapper = match scene.tonemapper {
            Tonemapper::None => 0.0,
            Tonemapper::Reinhard => 1.0,
            Tonemapper::Aces => 2.0,
        };
        FrameUniform {
            view_proj: scene.camera.view_projection(aspect).to_cols_array(),
            camera_position: scene.camera.from.extend(1.0).to_array(),
            sun_direction: lighting.sun_direction.extend(0.0).to_array(),
            ambient_color: rgb(lighting.ambient_color, 1.0),
            fallback_color: rgb(lighting.sky_color, 1.0),
            fog_color: rgb(scene.fog.color, 1.0),
            fog: [scene.fog.show as u8 as f32, scene.fog.distance, scene.fog.size, scene.fog.height],
            tone: [tonemapper, scene.exposure, scene.gamma, (point_lights + 1) as f32],
            lights,
        }
    }

    /// Writes the uniforms of the objects that can be drawn (known, non-empty
    /// meshes) and are accepted by `keep`, and returns them in that order.
    fn upload_objects<'s>(&mut self, scene: &'s RenderScene, keep: impl Fn(&RenderObject) -> bool) -> Vec<&'s RenderObject> {
        let drawable: Vec<&RenderObject> = scene
            .objects
            .iter()
            .filter(|o| keep(o) && self.meshes.get(o.mesh.0).and_then(Option::as_ref).is_some_and(|m| m.vertex_count > 0))
            .collect();

        if drawable.len() > self.object_capacity {
            self.object_capacity = drawable.len().next_power_of_two();
            (self.object_buffer, self.object_bind) =
                object_buffer(&self.device, &self.object_layout, self.object_stride, self.object_capacity);
        }
        let mut uniforms = vec![0u8; drawable.len() * self.object_stride as usize];
        for (i, object) in drawable.iter().enumerate() {
            let start = i * self.object_stride as usize;
            uniforms[start..start + size_of::<ObjectUniform>()].copy_from_slice(bytemuck::bytes_of(&ObjectUniform::new(object)));
        }
        if !uniforms.is_empty() {
            self.queue.write_buffer(&self.object_buffer, 0, &uniforms);
        }
        drawable
    }

    /// What is under pixel (`x`, `y`) of a viewport of `width` × `height`
    /// pixels showing `scene`: the pick id of the nearest object there, or
    /// `None` for nothing (or the ground).
    pub fn pick(&mut self, scene: &RenderScene, width: u32, height: u32, x: u32, y: u32) -> Result<Option<u32>, GpuError> {
        if width == 0 || height == 0 || x >= width || y >= height {
            return Ok(None);
        }
        let aspect = width as f32 / height as f32;
        let mut frame = Self::frame_uniform(scene, aspect);
        // Stretch the projection so that the pixel fills the 1×1 target.
        let (w, h) = (width as f32, height as f32);
        let cx = 2.0 * (x as f32 + 0.5) / w - 1.0;
        let cy = 1.0 - 2.0 * (y as f32 + 0.5) / h;
        let crop = glam::Mat4::from_cols(
            glam::Vec4::new(w, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, h, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
            glam::Vec4::new(-cx * w, -cy * h, 0.0, 1.0),
        );
        frame.view_proj = (crop * glam::Mat4::from_cols_array(&frame.view_proj)).to_cols_array();
        self.queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));
        let drawable = self.upload_objects(scene, |o| o.pick != 0);

        let (texture, view, depth, buffer) = &self.pick_target;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pick") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pick"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.frame_bind, &[]);
            for (i, object) in drawable.iter().enumerate() {
                let Some(mesh) = &self.meshes[object.mesh.0] else { continue };
                let pipeline = if object.backfaces { &self.pick_two_sided } else { &self.pick_cull };
                let texture = object.texture.and_then(|t| self.textures.get(t.0)).unwrap_or(&self.white);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &self.object_bind, &[(i as u64 * self.object_stride) as u32]);
                pass.set_bind_group(2, texture, &[]);
                pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                pass.draw(0..mesh.vertex_count, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.slice(..4).map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::Wait).map_err(|e| GpuError::Readback(e.to_string()))?;
        receiver.recv().map_err(|e| GpuError::Readback(e.to_string()))?.map_err(|e| GpuError::Readback(e.to_string()))?;
        let id = {
            let data = buffer.slice(..4).get_mapped_range();
            u32::from_le_bytes([data[0], data[1], data[2], data[3]])
        };
        buffer.unmap();
        Ok((id != 0).then_some(id))
    }

    /// Draws `scene` into `viewport` of the target and submits the work.
    ///
    /// With `clear` the whole target is first cleared to the sky colour and
    /// the depth buffer reset; without it the existing content is kept, for
    /// drawing a second view next to the first.
    pub fn render(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        viewport: Viewport,
        scene: &RenderScene,
        clear: bool,
    ) {
        if viewport.width == 0 || viewport.height == 0 {
            return;
        }

        let aspect = viewport.width as f32 / viewport.height as f32;
        self.queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&Self::frame_uniform(scene, aspect)));

        let drawable = self.upload_objects(scene, |o| !o.pick_only);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("world") });
        {
            let sky = scene.lighting.sky_color;
            let (color_load, depth_load) = if clear {
                let color = wgpu::Color { r: sky[0] as f64, g: sky[1] as f64, b: sky[2] as f64, a: 1.0 };
                (wgpu::LoadOp::Clear(color), wgpu::LoadOp::Clear(1.0))
            } else {
                (wgpu::LoadOp::Load, wgpu::LoadOp::Load)
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: color_load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: depth_load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(viewport.x as f32, viewport.y as f32, viewport.width as f32, viewport.height as f32, 0.0, 1.0);
            pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
            pass.set_bind_group(0, &self.frame_bind, &[]);

            for (i, object) in drawable.iter().enumerate() {
                let Some(mesh) = &self.meshes[object.mesh.0] else { continue };
                let pipeline = if object.backfaces { &self.pipeline_two_sided } else { &self.pipeline_cull };
                let texture = object.texture.and_then(|t| self.textures.get(t.0)).unwrap_or(&self.white);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &self.object_bind, &[(i as u64 * self.object_stride) as u32]);
                pass.set_bind_group(2, texture, &[]);
                pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                pass.draw(0..mesh.vertex_count, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}

/// Why no graphics device could be created.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no suitable graphics adapter was found: {0}")]
    NoAdapter(#[from] wgpu::RequestAdapterError),
    #[error("the graphics device could not be created: {0}")]
    NoDevice(#[from] wgpu::RequestDeviceError),
    #[error("reading the rendered image back failed: {0}")]
    Readback(String),
}

/// Requests a device that can run the renderer. `surface` must be given when
/// the device is going to present to a window.
pub async fn request_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), GpuError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("mine-imator"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await?;
    Ok((adapter, device, queue))
}

/// Renders into an image in memory instead of a window: used for export and
/// for tests.
pub struct OffscreenTarget {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl OffscreenTarget {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, renderer: &Renderer, width: u32, height: u32) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = renderer.create_depth_view(width, height);
        Self { device: device.clone(), queue: queue.clone(), texture, color, depth, width, height }
    }

    pub fn viewport(&self) -> Viewport {
        Viewport { x: 0, y: 0, width: self.width, height: self.height }
    }

    /// Copies the image out of the GPU as tightly packed RGBA rows, top row
    /// first.
    pub fn read_rgba(&self) -> Result<Vec<u8>, GpuError> {
        let bytes_per_row = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = bytes_per_row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: padded as u64 * self.height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(self.height) },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::Wait).map_err(|e| GpuError::Readback(e.to_string()))?;
        receiver
            .recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;

        let data = buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((bytes_per_row * self.height) as usize);
        for row in data.chunks(padded as usize) {
            pixels.extend_from_slice(&row[..bytes_per_row as usize]);
        }
        Ok(pixels)
    }
}
