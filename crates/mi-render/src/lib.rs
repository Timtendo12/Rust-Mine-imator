//! The renderer: draws a [`RenderScene`] with wgpu.
//!
//! It knows nothing about projects, timelines or the editor; the scene it is
//! given holds finished matrices, colours and lights.

pub mod camera;
pub mod environment;
mod renderer;
pub mod scene;

pub use camera::{Camera, WorkCamera};
pub use environment::{Lighting, SkySettings};
pub use mi_mesh::{ground_mesh, shape_mesh, MeshData, Shape, ShapeSettings, Vertex};
pub use renderer::{
    request_device, GpuError, OffscreenTarget, Renderer, TextureFilter, Viewport, DEPTH_FORMAT, MAX_POINT_LIGHTS,
};
pub use scene::{ColorTransform, Fog, MeshId, PointLight, RenderObject, RenderScene, TextureId, Tonemapper};

pub use wgpu;
