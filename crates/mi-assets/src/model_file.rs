//! `.mimodel` files: character and special block models made of parts and
//! shapes (`model_file_load`, `model_file_load_part`, `model_file_load_shape`).
//!
//! Files use Minecraft's axes (Y up); everything here is converted to the
//! program's (Z up), so an axis called `"y"` in a file is Z here.

use mi_anim::scene::{BendInfo, BendPart, PartInfo};
use mi_anim::Vec3;
use mi_core::Color;
use mi_format::json::{self, Json, JsonObject};

/// Why a model file could not be used.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelError {
    #[error("could not parse the model: {0}")]
    Json(#[from] json::JsonError),
    #[error("missing or invalid \"{0}\"")]
    Missing(&'static str),
    #[error("duplicate part name \"{0}\"")]
    DuplicatePart(String),
    #[error("invalid value \"{value}\" for \"{field}\"")]
    Invalid { field: &'static str, value: String },
}

/// Colour settings of a part or shape, already combined with those of the
/// parents they inherit from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelColor {
    pub blend: Color,
    pub alpha: f64,
    pub emissive: f64,
    pub mix: Color,
    pub mix_percent: f64,
}

impl Default for ModelColor {
    fn default() -> Self {
        Self { blend: Color::WHITE, alpha: 1.0, emissive: 0.0, mix: Color::BLACK, mix_percent: 0.0 }
    }
}

/// How a part bends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    pub part: BendPart,
    pub axis: [bool; 3],
    /// Position of the joint along the bend direction, in part space.
    pub offset: f64,
    pub end_offset: f64,
    /// Length of the bent section; `None` uses the project's bend style.
    pub size: Option<f64>,
    pub invert: [bool; 3],
    pub direction_min: Vec3,
    pub direction_max: Vec3,
    /// Angle the part is bent by in its resting pose.
    pub default_angle: Vec3,
    /// Resting angle including that of the parents it inherits from.
    pub inherit_angle: Vec3,
    /// Whether the bend adds the parent part's bend (`inherit_bend`).
    pub inherit: bool,
}

impl Bend {
    /// The part of this that the transform update needs.
    pub fn info(&self) -> BendInfo {
        BendInfo {
            part: self.part,
            axis: self.axis,
            offset: self.offset,
            end_offset: self.end_offset,
            invert: self.invert,
            direction_min: self.direction_min,
            direction_max: self.direction_max,
        }
    }
}

/// Which vertices of a shape sway in the wind (`e_vertex_wave`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wave {
    #[default]
    None,
    ZOnly,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// A box.
    Block,
    /// A flat rectangle, optionally extruded from its texture ("3D plane").
    Plane,
}

/// A box or plane of a model part.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelShape {
    pub kind: ShapeKind,
    /// Name that model states use to address the shape.
    pub description: String,
    pub use_model_color: bool,
    /// Texture of this shape when it differs from its part's.
    pub texture_name: Option<String>,
    /// Size of the texture the UVs refer to, made square.
    pub texture_size: [f64; 2],
    pub color: ModelColor,
    pub texture_mirror: bool,
    pub invert: bool,
    pub hide_front: bool,
    pub hide_back: bool,
    pub face_camera: bool,
    pub item_bounce: bool,
    /// Distance the part must be moved before the shape is shown.
    pub move_required: Option<Vec3>,
    pub floor_box_uvs: bool,
    pub from_noscale: Vec3,
    pub to_noscale: Vec3,
    /// Corners in part space, with inflation and scale applied.
    pub from: Vec3,
    pub to: Vec3,
    pub is_3d: bool,
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
    pub locked: bool,
    /// Whether the shape bends with its part.
    pub bend_shape: bool,
    pub uv: [f64; 2],
    pub wave: Wave,
    pub wave_zmin: Option<f64>,
    pub wave_zmax: Option<f64>,
}

/// A part of a model: it becomes a body part timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPart {
    pub name: String,
    /// Draw order among the parts of the model.
    pub depth: f64,
    pub texture_name: Option<String>,
    pub texture_size: [f64; 2],
    pub color: ModelColor,
    /// Whether a shape of this part mixes in a colour.
    pub part_mixing_shapes: bool,
    /// Position relative to the parent part (or the model).
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
    pub show_position: bool,
    pub locked: bool,
    /// Whether the part follows the bent half of its parent.
    pub lock_bend: bool,
    pub backfaces: bool,
    pub bend: Option<Bend>,
    pub shapes: Vec<ModelShape>,
    pub parts: Vec<ModelPart>,
    pub has_3d_plane: bool,
}

impl ModelPart {
    /// What the transform update needs to know about this part.
    pub fn info(&self) -> PartInfo {
        PartInfo { position: self.position, rotation: self.rotation, bend: self.bend.as_ref().map(Bend::info) }
    }

    /// Finds a part by name in this part and its descendants.
    pub fn find(&self, name: &str) -> Option<&ModelPart> {
        if self.name == name {
            return Some(self);
        }
        self.parts.iter().find_map(|p| p.find(name))
    }
}

/// A model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelFile {
    pub name: String,
    pub description: String,
    pub texture_name: String,
    pub texture_size: [f64; 2],
    pub player_skin: bool,
    pub scale: Vec3,
    /// Name of the palette colour the model can be tinted with.
    pub model_color: String,
    pub floor_box_uvs: bool,
    pub parts: Vec<ModelPart>,
    pub has_3d_plane: bool,
}

/// What parts and shapes inherit from the element containing them.
#[derive(Clone)]
struct Parent<'a> {
    texture_size: [f64; 2],
    color: ModelColor,
    scale: Vec3,
    floor_box_uvs: bool,
    /// The containing part, if the container is a part and not the model.
    part: Option<&'a PartContext>,
}

/// The fields of a part that its children depend on.
struct PartContext {
    locked: bool,
    bend: Option<Bend>,
}

fn square(size: [f64; 2]) -> [f64; 2] {
    let side = size[0].max(size[1]);
    [side, side]
}

fn mul(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn color_multiply(a: Color, b: Color) -> Color {
    let channel = |x: u8, y: u8| ((x as f64 / 255.0) * (y as f64 / 255.0) * 255.0) as u8;
    Color::rgb(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}

fn color_add(a: Color, b: Color) -> Color {
    Color::rgb(a.r.saturating_add(b.r), a.g.saturating_add(b.g), a.b.saturating_add(b.b))
}

/// Colour settings of a part or shape. `color_emissive` was called
/// `color_brightness` in older files; the original only ever reads the old
/// name because of a typo, here both work.
fn load_color(map: &JsonObject, parent: &ModelColor) -> ModelColor {
    let mut color = ModelColor {
        blend: map.color("color_blend").unwrap_or(Color::WHITE),
        alpha: map.real("color_alpha").unwrap_or(1.0),
        emissive: map.real("color_emissive").or(map.real("color_brightness")).unwrap_or(0.0),
        mix: map.color("color_mix").unwrap_or(Color::BLACK),
        mix_percent: map.real("color_mix_percent").unwrap_or(0.0),
    };
    if map.flag("color_inherit").unwrap_or(true) {
        color.blend = color_multiply(color.blend, parent.blend);
        color.alpha *= parent.alpha;
        color.emissive = (color.emissive + parent.emissive).clamp(0.0, 1.0);
        color.mix = color_add(color.mix, parent.mix);
        color.mix_percent = (color.mix_percent + parent.mix_percent).clamp(0.0, 1.0);
    }
    color
}

/// Own texture of a part or shape, or the inherited size.
fn load_texture(map: &JsonObject, parent: &Parent) -> Result<(Option<String>, [f64; 2]), ModelError> {
    match map.string("texture") {
        Some(name) => {
            let size = map.point2("texture_size").ok_or(ModelError::Missing("texture_size"))?;
            Ok((Some(name.to_owned()), square(size)))
        }
        None => Ok((None, parent.texture_size)),
    }
}

/// Maps a file axis name to an axis index: files are Y-up.
fn axis_index(name: &str) -> Result<usize, ModelError> {
    match name {
        "x" => Ok(0),
        "z" => Ok(1),
        "y" => Ok(2),
        other => Err(ModelError::Invalid { field: "axis", value: other.to_owned() }),
    }
}

/// Reads a value that is a single item for one axis or a list with one item
/// per listed axis.
fn per_axis<T: Copy>(
    json: Option<&Json>,
    axes: &[usize],
    target: &mut [T; 3],
    convert: impl Fn(&Json) -> Option<T>,
) {
    match json {
        Some(Json::Array(items)) => {
            for (item, &axis) in items.iter().zip(axes) {
                if let Some(value) = convert(item) {
                    target[axis] = value;
                }
            }
        }
        Some(single) if axes.len() == 1 => {
            if let Some(value) = convert(single) {
                target[axes[0]] = value;
            }
        }
        _ => {}
    }
}

fn load_bend(map: &JsonObject, scale: Vec3, parent: Option<&PartContext>) -> Result<Bend, ModelError> {
    let mut offset = map.real("offset").ok_or(ModelError::Missing("offset"))?;
    let mut size = map.real("size");
    let part_name = map.string("part").ok_or(ModelError::Missing("part"))?;
    let (part, scale_axis) = match part_name {
        "right" => (BendPart::Right, 0),
        "left" => (BendPart::Left, 0),
        "front" => (BendPart::Front, 1),
        "back" => (BendPart::Back, 1),
        "upper" => (BendPart::Upper, 2),
        "lower" => (BendPart::Lower, 2),
        other => return Err(ModelError::Invalid { field: "part", value: other.to_owned() }),
    };
    offset *= scale[scale_axis];
    if let Some(size) = size.as_mut() {
        *size *= scale[scale_axis];
    }

    let axes: Vec<usize> = match map.get("axis") {
        Some(Json::String(name)) => vec![axis_index(name)?],
        Some(Json::Array(names)) => names
            .iter()
            .map(|n| n.as_str().ok_or(ModelError::Missing("axis")).and_then(axis_index))
            .collect::<Result<_, _>>()?,
        _ => return Err(ModelError::Missing("axis")),
    };
    let mut axis = [false; 3];
    for &a in &axes {
        axis[a] = true;
    }

    let mut direction_min = [-180.0; 3];
    let mut direction_max = [180.0; 3];
    per_axis(map.get("direction_min"), &axes, &mut direction_min, Json::as_real);
    per_axis(map.get("direction_max"), &axes, &mut direction_max, Json::as_real);

    let mut invert = [false; 3];
    match map.get("invert") {
        // A single value applies to every axis.
        Some(single @ (Json::Bool(_) | Json::Number(_))) if axes.len() == 1 => {
            invert = [single.as_flag().unwrap_or(false); 3];
        }
        other => per_axis(other, &axes, &mut invert, Json::as_flag),
    }

    // Older files give a direction instead of limits.
    #[derive(Clone, Copy, PartialEq)]
    enum Direction {
        Forward,
        Backward,
        Both,
    }
    let parse_direction = |json: &Json| match json.as_str() {
        Some("forward") => Some(Direction::Forward),
        Some("backward") => Some(Direction::Backward),
        Some("both") => Some(Direction::Both),
        _ => None,
    };
    if let Some(direction) = map.get("direction") {
        let invalid = match direction {
            Json::String(_) => parse_direction(direction).is_none(),
            Json::Array(items) => items.iter().any(|i| parse_direction(i).is_none()),
            _ => false,
        };
        if invalid {
            return Err(ModelError::Invalid { field: "direction", value: format!("{direction:?}") });
        }
        if matches!(direction, Json::String(_) | Json::Array(_)) {
            // Axes without a direction count as "forward", the first value
            // of the original's enumeration.
            let mut directions = [Direction::Forward; 3];
            per_axis(Some(direction), &axes, &mut directions, parse_direction);
            for i in 0..3 {
                match directions[i] {
                    Direction::Both => {
                        direction_min[i] = -180.0;
                        direction_max[i] = 180.0;
                    }
                    Direction::Forward => {
                        direction_min[i] = 0.0;
                        direction_max[i] = 180.0;
                        // "forward" used to invert the angle.
                        invert[i] = !invert[i];
                    }
                    Direction::Backward => {
                        direction_min[i] = 0.0;
                        direction_max[i] = 180.0;
                    }
                }
            }
        }
    }

    let mut default_angle = [0.0; 3];
    match map.get("angle") {
        Some(Json::Number(angle)) if axes.len() == 1 => default_angle = [*angle; 3],
        other => per_axis(other, &axes, &mut default_angle, Json::as_real),
    }

    let inherit_bend = map.flag("inherit_bend").unwrap_or(false);
    let mut inherit_angle = default_angle;
    if inherit_bend {
        if let Some(parent_bend) = parent.and_then(|p| p.bend.as_ref()) {
            for (angle, parent_angle) in inherit_angle.iter_mut().zip(parent_bend.inherit_angle) {
                *angle += parent_angle;
            }
        }
    }

    Ok(Bend {
        part,
        axis,
        offset,
        end_offset: map.real("end_offset").unwrap_or(0.0),
        size,
        invert,
        direction_min,
        direction_max,
        default_angle,
        inherit_angle,
        inherit: inherit_bend,
    })
}

fn load_shape(map: &JsonObject, parent: &Parent) -> Result<Option<ModelShape>, ModelError> {
    if map.get("visible").is_some_and(|v| v.as_flag() == Some(false)) {
        return Ok(None);
    }
    let kind = match map.string("type").ok_or(ModelError::Missing("type"))? {
        "block" => ShapeKind::Block,
        "plane" => ShapeKind::Plane,
        other => return Err(ModelError::Invalid { field: "type", value: other.to_owned() }),
    };
    let from_noscale = map.point3("from").ok_or(ModelError::Missing("from"))?;
    let mut to_noscale = map.point3("to").ok_or(ModelError::Missing("to"))?;
    let uv = map.point2("uv").ok_or(ModelError::Missing("uv"))?;
    let (texture_name, texture_size) = load_texture(map, parent)?;

    let mut inflate = [map.real("inflate").unwrap_or(0.0); 3];
    let mut is_3d = false;
    if kind == ShapeKind::Plane {
        // Planes have no depth, except 3D planes, which are one pixel thick.
        to_noscale[1] = from_noscale[1];
        is_3d = map.flag("3d").unwrap_or(false);
        if is_3d {
            to_noscale[1] += 1.0;
        } else {
            inflate[1] = 0.0;
        }
    }

    let scale = mul(map.point3("scale").unwrap_or([1.0; 3]), parent.scale);
    let position = mul(map.point3("position").unwrap_or([0.0; 3]), parent.scale);
    let sub = |a: Vec3, b: Vec3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let add = |a: Vec3, b: Vec3| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

    let (mut wave, mut wave_zmin, mut wave_zmax) = (Wave::None, None, None);
    if let Some(wind) = map.object("wind") {
        if let Some(axis) = wind.string("axis") {
            wave = if axis == "y" { Wave::ZOnly } else { Wave::All };
        }
        wave_zmin = wind.real("ymin");
        wave_zmax = wind.real("ymax");
    }

    let mut hide_back = map.flag("hide_back").unwrap_or(false);
    // Old name of the setting.
    if let Some(Json::Bool(hide)) = map.get("hide_backface") {
        hide_back = *hide;
    }

    Ok(Some(ModelShape {
        kind,
        description: map.string("description").unwrap_or("").to_owned(),
        use_model_color: map.flag("use_model_color").unwrap_or(false),
        texture_name,
        texture_size,
        color: load_color(map, &parent.color),
        texture_mirror: map.flag("texture_mirror").unwrap_or(false),
        invert: map.flag("invert").unwrap_or(false),
        hide_front: map.flag("hide_front").unwrap_or(false),
        hide_back,
        face_camera: map.flag("face_camera").unwrap_or(false),
        item_bounce: map.flag("item_bounce").unwrap_or(false),
        move_required: map.point3("move_required").filter(|m| *m != [-1.0; 3]),
        floor_box_uvs: parent.floor_box_uvs,
        from_noscale,
        to_noscale,
        from: mul(sub(from_noscale, inflate), scale),
        to: mul(add(to_noscale, inflate), scale),
        is_3d,
        position,
        rotation: map.point3("rotation").unwrap_or([0.0; 3]),
        scale,
        locked: map.flag("locked").unwrap_or(false),
        bend_shape: map.flag("bend").unwrap_or(true),
        uv,
        wave,
        wave_zmin,
        wave_zmax,
    }))
}

fn load_part(map: &JsonObject, parent: &Parent, names: &mut Vec<String>) -> Result<Option<ModelPart>, ModelError> {
    if map.get("visible").is_some_and(|v| v.as_flag() == Some(false)) {
        return Ok(None);
    }
    let name = map.string("name").ok_or(ModelError::Missing("name"))?.to_owned();
    let position_noscale = map.point3("position").ok_or(ModelError::Missing("position"))?;
    // Names must be unique in the whole file. (The original registers a
    // part only after its children, which lets a child reuse the name of an
    // ancestor; lookups by name then find the wrong part.)
    if names.contains(&name) {
        return Err(ModelError::DuplicatePart(name));
    }
    names.push(name.clone());

    let (texture_name, texture_size) = load_texture(map, parent)?;
    let color = load_color(map, &parent.color);
    let mut position = mul(position_noscale, parent.scale);
    let scale = mul(map.point3("scale").unwrap_or([1.0; 3]), parent.scale);

    let locked = parent.part.is_some_and(|p| p.locked) || map.flag("locked").unwrap_or(false);

    // Parts attached to the bent half of their parent are positioned
    // relative to its joint.
    let mut lock_bend = true;
    if let Some(parent_bend) = parent.part.and_then(|p| p.bend.as_ref()) {
        if let Some(value) = map.get("lock_bend").and_then(Json::as_flag) {
            lock_bend = value;
        }
        if lock_bend {
            let axis = match parent_bend.part {
                BendPart::Left | BendPart::Right => 0,
                BendPart::Back | BendPart::Front => 1,
                BendPart::Lower | BendPart::Upper => 2,
            };
            position[axis] -= parent_bend.offset;
        }
    }

    let bend = match map.object("bend") {
        Some(bend) => Some(load_bend(bend, scale, parent.part)?),
        None => None,
    };

    let context = PartContext { locked, bend };
    let child_parent = Parent {
        texture_size,
        color,
        scale,
        floor_box_uvs: parent.floor_box_uvs,
        part: Some(&context),
    };

    let mut shapes = Vec::new();
    for entry in map.array("shapes").unwrap_or_default() {
        if let Some(shape_map) = entry.as_object() {
            if let Some(shape) = load_shape(shape_map, &child_parent)? {
                shapes.push(shape);
            }
        }
    }

    let mut parts = Vec::new();
    for entry in map.array("parts").unwrap_or_default() {
        if let Some(part_map) = entry.as_object() {
            if let Some(part) = load_part(part_map, &child_parent, names)? {
                parts.push(part);
            }
        }
    }
    let has_3d_plane = shapes.iter().any(|s| s.is_3d);
    let part_mixing_shapes = color.mix_percent > 0.0 || shapes.iter().any(|s| s.color.mix_percent > 0.0);
    Ok(Some(ModelPart {
        name,
        depth: map.real("depth").unwrap_or(0.0),
        texture_name,
        texture_size,
        color,
        part_mixing_shapes,
        position,
        rotation: map.point3("rotation").unwrap_or([0.0; 3]),
        scale,
        show_position: map.flag("show_position").unwrap_or(false),
        locked,
        lock_bend,
        backfaces: map.flag("backfaces").unwrap_or(false),
        bend: context.bend,
        shapes,
        parts,
        has_3d_plane,
    }))
}

impl ModelFile {
    /// Parses a `.mimodel` file.
    pub fn load(bytes: &[u8]) -> Result<Self, ModelError> {
        let root = json::parse(bytes)?;
        let map = root.as_object().ok_or(ModelError::Missing("root object"))?;
        let name = map.string("name").ok_or(ModelError::Missing("name"))?.to_owned();
        let texture_name = map.string("texture").ok_or(ModelError::Missing("texture"))?.to_owned();
        let texture_size = square(map.point2("texture_size").ok_or(ModelError::Missing("texture_size"))?);
        let part_list = map.array("parts").ok_or(ModelError::Missing("parts"))?;

        let scale = map.point3("scale").unwrap_or([1.0; 3]);
        let floor_box_uvs = map.flag("floor_box_uvs").unwrap_or(false);
        let parent = Parent { texture_size, color: ModelColor::default(), scale, floor_box_uvs, part: None };

        let mut names = Vec::new();
        let mut parts = Vec::new();
        for entry in part_list {
            if let Some(part_map) = entry.as_object() {
                if let Some(part) = load_part(part_map, &parent, &mut names)? {
                    parts.push(part);
                }
            }
        }

        fn any_3d(parts: &[ModelPart]) -> bool {
            parts.iter().any(|p| p.has_3d_plane || any_3d(&p.parts))
        }
        let has_3d_plane = any_3d(&parts);

        Ok(Self {
            name,
            description: map.string("description").unwrap_or("").to_owned(),
            texture_name,
            texture_size,
            player_skin: map.flag("player_skin").unwrap_or(false),
            scale,
            model_color: map.string("model_color").unwrap_or("none").to_owned(),
            floor_box_uvs,
            parts,
            has_3d_plane,
        })
    }

    /// Finds a part by name anywhere in the model.
    pub fn find_part(&self, name: &str) -> Option<&ModelPart> {
        self.parts.iter().find_map(|p| p.find(name))
    }

    /// All parts, parents before children.
    pub fn all_parts(&self) -> Vec<&ModelPart> {
        fn collect<'a>(parts: &'a [ModelPart], out: &mut Vec<&'a ModelPart>) {
            for part in parts {
                out.push(part);
                collect(&part.parts, out);
            }
        }
        let mut out = Vec::new();
        collect(&self.parts, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &[u8] = br##"{
        "name": "test",
        "texture": "entity/test",
        "texture_size": [64, 32],
        "scale": [2, 2, 2],
        "parts": [
            {
                "name": "arm",
                "position": [4, 22, 0],
                "color_blend": "#804020",
                "bend": { "offset": -6, "end_offset": 6, "part": "lower", "axis": "x", "direction_max": 150, "invert": true },
                "shapes": [
                    { "type": "block", "from": [-2, -10, -2], "to": [2, 2, 2], "uv": [44, 20], "color_brightness": 0.5 },
                    { "type": "plane", "from": [-2, 0, 0], "to": [2, 4, 9], "uv": [0, 0], "3d": true, "color_inherit": false },
                    { "type": "block", "from": [0, 0, 0], "to": [1, 1, 1], "uv": [0, 0], "visible": false }
                ],
                "parts": [
                    { "name": "hand", "position": [0, -10, 0], "shapes": [
                        { "type": "plane", "from": [0, 0, 0], "to": [2, 2, 5], "uv": [1, 2], "inflate": 1,
                          "texture": "other", "texture_size": [16, 8], "wind": { "axis": "y", "ymin": 1 } } ] },
                    { "name": "loose", "position": [0, -10, 0], "lock_bend": false }
                ]
            },
            { "name": "hidden", "position": [0, 0, 0], "visible": false }
        ]
    }"##;

    #[test]
    fn parses_parts_shapes_and_bends() {
        let model = ModelFile::load(MODEL).unwrap();
        assert_eq!(model.name, "test");
        assert_eq!(model.texture_size, [64.0, 64.0], "made square");
        assert_eq!(model.parts.len(), 1, "invisible parts are skipped");
        assert!(model.has_3d_plane);

        let arm = &model.parts[0];
        // File [x, y, z] is [x, z, y] here, times the model scale.
        assert_eq!(arm.position, [8.0, 0.0, 44.0]);
        assert_eq!(arm.color.blend, Color::from_hex("#804020"));
        assert_eq!(arm.shapes.len(), 2, "invisible shapes are skipped");

        let bend = arm.bend.unwrap();
        assert_eq!(bend.part, BendPart::Lower);
        assert_eq!(bend.axis, [true, false, false]);
        assert_eq!(bend.offset, -12.0, "scaled along the bend direction");
        assert_eq!(bend.end_offset, 6.0);
        assert_eq!(bend.direction_max, [150.0, 180.0, 180.0]);
        assert_eq!(bend.invert, [true; 3]);
        assert!(bend.info().supports_ik());

        let block = &arm.shapes[0];
        assert_eq!(block.kind, ShapeKind::Block);
        assert_eq!(block.from, [-4.0, -4.0, -20.0]);
        assert_eq!(block.to, [4.0, 4.0, 4.0]);
        assert_eq!(block.color.emissive, 0.5, "the legacy key is read");
        assert_eq!(block.color.blend, Color::from_hex("#804020"), "inherited from the part");

        let plane = &arm.shapes[1];
        assert!(plane.is_3d);
        assert_eq!(plane.color.blend, Color::WHITE, "inheritance turned off");
        // Planes are flattened onto their front; 3D planes are one unit deep.
        assert_eq!((plane.from_noscale[1], plane.to_noscale[1]), (0.0, 1.0));
    }

    #[test]
    fn children_follow_the_bent_half_unless_unlocked() {
        let model = ModelFile::load(MODEL).unwrap();
        let hand = model.find_part("hand").unwrap();
        let loose = model.find_part("loose").unwrap();
        // File position [0, -10, 0] scaled is z = -20; the locked child is
        // relative to the joint at z = -12.
        assert_eq!(hand.position, [0.0, 0.0, -8.0]);
        assert!(hand.lock_bend);
        assert_eq!(loose.position, [0.0, 0.0, -20.0]);
        assert!(!loose.lock_bend);

        let shape = &hand.shapes[0];
        assert_eq!(shape.texture_name.as_deref(), Some("other"));
        assert_eq!(shape.texture_size, [16.0, 16.0]);
        assert_eq!(shape.wave, Wave::ZOnly);
        assert_eq!(shape.wave_zmin, Some(1.0));
        // Inflation applies to width and height of a flat plane, not depth.
        assert_eq!(shape.from, [-2.0, 0.0, -2.0]);
        assert_eq!(shape.to, [6.0, 0.0, 6.0]);

        assert_eq!(model.all_parts().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["arm", "hand", "loose"]);
        assert!(model.find_part("hidden").is_none());
    }

    #[test]
    fn emissive_key_is_read() {
        let doc = br#"{ "name": "m", "texture": "t", "texture_size": [16, 16], "parts": [
            { "name": "p", "position": [0, 0, 0], "color_emissive": 0.75 } ] }"#;
        assert_eq!(ModelFile::load(doc).unwrap().parts[0].color.emissive, 0.75);
    }

    #[test]
    fn legacy_bend_direction() {
        let doc = br#"{ "name": "m", "texture": "t", "texture_size": [16, 16], "parts": [
            { "name": "p", "position": [0, 0, 0],
              "bend": { "offset": 0, "part": "upper", "axis": ["x", "y"], "direction": ["forward", "both"] } } ] }"#;
        let bend = ModelFile::load(doc).unwrap().parts[0].bend.unwrap();
        assert_eq!(bend.axis, [true, false, true]);
        assert_eq!((bend.direction_min[0], bend.direction_max[0]), (0.0, 180.0));
        assert!(bend.invert[0], "forward inverts");
        assert_eq!((bend.direction_min[2], bend.direction_max[2]), (-180.0, 180.0));
    }

    #[test]
    fn errors() {
        let load = |text: &str| ModelFile::load(text.as_bytes()).unwrap_err();
        assert_eq!(load(r#"{ "texture": "t", "texture_size": [1, 1], "parts": [] }"#), ModelError::Missing("name"));
        assert_eq!(load(r#"{ "name": "m", "texture": "t", "parts": [] }"#), ModelError::Missing("texture_size"));
        let duplicate = r#"{ "name": "m", "texture": "t", "texture_size": [1, 1], "parts": [
            { "name": "a", "position": [0, 0, 0] }, { "name": "a", "position": [0, 0, 0] } ] }"#;
        assert_eq!(load(duplicate), ModelError::DuplicatePart("a".into()));
        let bad_shape = r#"{ "name": "m", "texture": "t", "texture_size": [1, 1], "parts": [
            { "name": "a", "position": [0, 0, 0], "shapes": [ { "type": "ball", "from": [0,0,0], "to": [1,1,1], "uv": [0,0] } ] } ] }"#;
        assert!(matches!(load(bad_shape), ModelError::Invalid { field: "type", .. }));
        assert!(matches!(load("nope"), ModelError::Json(_)));
    }
}
