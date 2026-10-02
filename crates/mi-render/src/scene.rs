//! What the renderer draws: a self-contained description of one frame.
//!
//! The scene is produced by the animation layer; nothing in here refers to
//! timelines, keyframes or editor state.

use crate::camera::Camera;
use crate::environment::{Lighting, Rgb};

/// Handle of a mesh uploaded to the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshId(pub(crate) usize);

impl MeshId {
    /// An id that does not come from a renderer, for describing scenes that
    /// are only inspected (for example in tests).
    pub fn from_raw(index: usize) -> Self {
        Self(index)
    }
}

/// Handle of a texture uploaded to the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) usize);

/// Colour adjustments of an object beyond its blend colour (`uColorsExt`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorTransform {
    pub rgb_add: Rgb,
    pub rgb_sub: Rgb,
    pub hsb_add: Rgb,
    pub hsb_sub: Rgb,
    pub hsb_mul: Rgb,
    pub mix_color: Rgb,
    pub mix_percent: f32,
}

/// One draw: a mesh with its transform and material.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderObject {
    pub mesh: MeshId,
    /// `None` draws with a plain white texture.
    pub texture: Option<TextureId>,
    /// World matrix in GameMaker layout (translation in elements 12..14).
    pub model: [f32; 16],
    /// Multiplied with the texture: RGB_MUL and alpha.
    pub blend_color: [f32; 4],
    pub colors: Option<ColorTransform>,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: f32,
    /// Draw without lighting (sky objects, or everything in flat mode).
    pub unlit: bool,
    /// Lit by the sun only, like the ground.
    pub sun_only: bool,
    pub fog: bool,
    /// Draw both sides of every triangle.
    pub backfaces: bool,
}

impl RenderObject {
    /// An opaque white object with default material values.
    pub fn new(mesh: MeshId, model: [f32; 16]) -> Self {
        Self {
            mesh,
            texture: None,
            model,
            blend_color: [1.0; 4],
            colors: None,
            metallic: 0.0,
            roughness: 1.0,
            emissive: 0.0,
            unlit: false,
            sun_only: false,
            fog: true,
            backfaces: false,
        }
    }
}

/// A point light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLight {
    pub position: [f32; 3],
    pub range: f32,
    /// Colour multiplied by strength.
    pub color: Rgb,
}

/// Distance fog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    pub show: bool,
    pub color: Rgb,
    pub distance: f32,
    pub size: f32,
    pub height: f32,
}

/// `e_tonemapper`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tonemapper {
    None,
    Reinhard,
    Aces,
}

/// Everything needed to draw one frame of one view.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderScene {
    pub camera: Camera,
    pub lighting: Lighting,
    pub fog: Fog,
    pub tonemapper: Tonemapper,
    pub exposure: f32,
    pub gamma: f32,
    /// At most 63 are used; the sun takes the first slot.
    pub lights: Vec<PointLight>,
    /// Drawn in order.
    pub objects: Vec<RenderObject>,
}
