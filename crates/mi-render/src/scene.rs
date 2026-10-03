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

impl TextureId {
    /// An id that does not come from a renderer, for scenes that are only
    /// inspected.
    pub fn from_raw(index: usize) -> Self {
        Self(index)
    }
}

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

/// Which part of the frame an object belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layer {
    /// Solid things, hidden by what is in front of them.
    #[default]
    World,
    /// The backdrop: drawn in order behind everything, without depth.
    Sky,
    /// Like `Sky`, but added to what is there (sun and moon).
    SkyAdd,
}

/// How an object takes the wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectWind {
    /// The whole object sways (`wind` of a timeline).
    pub whole: bool,
    /// Its vertices that are marked for it sway: leaves, plants, liquids
    /// (`wind_terrain`).
    pub marked: bool,
    /// How far it sways; 0 keeps it still.
    pub strength: f32,
    /// How far gusts push it along the wind's direction.
    pub directional_strength: f32,
}

impl Default for ObjectWind {
    /// Still.
    fn default() -> Self {
        Self { whole: false, marked: true, strength: 0.0, directional_strength: 0.0 }
    }
}

/// The wind of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Wind {
    /// Time in sixtieths of a second (`background_time`).
    pub time: f32,
    /// How fast things sway; 0 without wind.
    pub speed: f32,
    /// Unit vector the gusts travel along, on the ground plane.
    pub direction: [f32; 2],
    /// How far the gusts have travelled.
    pub gust_phase: f32,
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
    /// What clicking the object picks; 0 for nothing (the ground). Below
    /// 2^24, as it travels through a float.
    pub pick: u32,
    /// Only there to be clicked (the boxes of lights and cameras).
    pub pick_only: bool,
    /// Part of the selection, which gets an outline.
    pub selected: bool,
    pub layer: Layer,
    pub wind: ObjectWind,
    /// Casts a shadow in the high quality mode.
    pub shadows: bool,
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
            pick: 0,
            pick_only: false,
            selected: false,
            layer: Layer::World,
            wind: ObjectWind::default(),
            shadows: true,
        }
    }
}

/// The cone of a spot light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotCone {
    /// A point it shines towards.
    pub to: [f32; 3],
    /// Opening angle in degrees.
    pub radius: f32,
    /// 1 has a hard edge, 0 fades from the middle.
    pub sharpness: f32,
}

/// A point or spot light. The low quality modes use its position, range
/// and colour only, and treat spot lights as point lights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLight {
    pub position: [f32; 3],
    pub range: f32,
    /// Colour multiplied by strength.
    pub color: Rgb,
    /// Share of the range over which it fades out.
    pub fade_size: f32,
    /// Strength of its highlights.
    pub specular: f32,
    /// Size of the light, which softens its shadows.
    pub size: f32,
    /// Casts shadows in the high quality mode.
    pub shadows: bool,
    pub spot: Option<SpotCone>,
}

impl PointLight {
    /// A point light with the settings of a new light timeline.
    pub fn new(position: [f32; 3], range: f32, color: Rgb) -> Self {
        Self { position, range, color, fade_size: 0.5, specular: 1.0, size: 2.0, shadows: true, spot: None }
    }
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
    /// Colour the frame is cleared with.
    pub background: Rgb,
    pub wind: Wind,
    /// Effects of the camera, applied in the high quality mode.
    pub post: crate::post::PostEffects,
    pub fog: Fog,
    pub tonemapper: Tonemapper,
    pub exposure: f32,
    pub gamma: f32,
    /// At most 63 are used; the sun takes the first slot.
    pub lights: Vec<PointLight>,
    /// Drawn in order.
    pub objects: Vec<RenderObject>,
}
