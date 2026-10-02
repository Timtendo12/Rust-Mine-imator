//! Blocks: the block list of the asset manifest, Minecraft blockstate and
//! block model files, and the meshes they produce (`block_load`,
//! `block_load_state_file`, `block_load_model_file`,
//! `block_load_render_model`, `block_render_model_generate`).
//!
//! Minecraft files are Y-up; positions are converted to the program's Z-up
//! axes on load, so a block occupies 0..16 on each axis with +Y south.

use crate::pack::{parse_state_vars, AssetPack};
use mi_anim::math::Mat4;
use mi_anim::Vec3;
use mi_format::json::{self, Json, JsonObject};
use mi_format::StateValue;
use mi_mesh::MeshData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const BLOCK: f64 = 16.0;

/// Face directions in the order of `e_dir`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    /// +X
    East,
    /// -X
    West,
    /// +Y
    South,
    /// -Y
    North,
    /// +Z
    Up,
    /// -Z
    Down,
}

impl Dir {
    pub const ALL: [Dir; 6] = [Dir::East, Dir::West, Dir::South, Dir::North, Dir::Up, Dir::Down];

    pub fn name(self) -> &'static str {
        match self {
            Dir::East => "east",
            Dir::West => "west",
            Dir::South => "south",
            Dir::North => "north",
            Dir::Up => "up",
            Dir::Down => "down",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::East => Dir::West,
            Dir::West => Dir::East,
            Dir::South => Dir::North,
            Dir::North => Dir::South,
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }

    /// The step to the neighbouring block on this side.
    pub fn step(self) -> [i64; 3] {
        match self {
            Dir::East => [1, 0, 0],
            Dir::West => [-1, 0, 0],
            Dir::South => [0, 1, 0],
            Dir::North => [0, -1, 0],
            Dir::Up => [0, 0, 1],
            Dir::Down => [0, 0, -1],
        }
    }
}

/// One possible value of a block state.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockStateValue {
    pub value: String,
    /// Blockstate file used when the state has this value.
    pub file: Option<String>,
    pub emissive: Option<f64>,
}

/// A block of the asset manifest (`obj_block`).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    pub name: String,
    /// Kind of block for connection rules (`stairs`, `fence`, `snowy`, ...).
    pub kind: String,
    pub file: Option<String>,
    pub emissive: f64,
    pub subsurface: f64,
    pub states: Vec<(String, Vec<BlockStateValue>)>,
    pub default_state: Vec<(String, String)>,
    /// Minecraft ids and the state they stand for (`minecraft:oak_stairs`
    /// → `variant=oak`).
    pub ids: Vec<(String, Vec<(String, String)>)>,
}

impl BlockDef {
    fn load(map: &JsonObject) -> Option<Self> {
        let states = map
            .object("states")
            .map(|states| {
                states
                    .iter()
                    .map(|(name, values)| {
                        let values = values
                            .as_array()
                            .unwrap_or_default()
                            .iter()
                            .map(|v| match v {
                                Json::Object(o) => BlockStateValue {
                                    value: o.string("value").unwrap_or("").to_owned(),
                                    file: o.string("file").map(str::to_owned),
                                    emissive: o.real("emissive"),
                                },
                                other => BlockStateValue {
                                    value: match other {
                                        Json::String(s) => s.clone(),
                                        Json::Bool(b) => b.to_string(),
                                        o => o.as_real().map(json::format_number).unwrap_or_default(),
                                    },
                                    file: None,
                                    emissive: None,
                                },
                            })
                            .collect();
                        (name.to_owned(), values)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ids = match map.get("id") {
            Some(Json::String(id)) => vec![(id.clone(), Vec::new())],
            Some(Json::Object(ids)) => {
                ids.iter().map(|(id, state)| (id.to_owned(), parse_state_vars(state.as_str().unwrap_or("")))).collect()
            }
            _ => Vec::new(),
        };
        Some(Self {
            name: map.string("name")?.to_owned(),
            kind: map.string("type").unwrap_or("").to_owned(),
            file: map.string("file").map(str::to_owned),
            emissive: map.real("emissive").unwrap_or(0.0),
            subsurface: map.real("subsurface").unwrap_or(0.0),
            states,
            default_state: map.string("default_state").map(parse_state_vars).unwrap_or_default(),
            ids,
        })
    }

    /// The value of every declared state: given, else the default, else the
    /// first possible value.
    pub fn full_state(&self, given: &[(String, StateValue)]) -> Vec<(String, String)> {
        self.states
            .iter()
            .map(|(name, values)| {
                let value = given
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.to_text())
                    .or_else(|| self.default_state.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()))
                    .or_else(|| values.first().map(|v| v.value.clone()))
                    .unwrap_or_default();
                (name.clone(), value)
            })
            .collect()
    }
}

/// One face of a render element.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFace {
    /// Texture coordinates in pixels (0..16) of the four corners.
    pub uv: [[f64; 2]; 4],
    /// Texture name, e.g. `block/stone`.
    pub texture: String,
    /// Whether the face lies on the boundary of the block, where a
    /// neighbouring block can hide it.
    pub edge: bool,
}

/// A box of a block model, ready to be turned into faces.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderElement {
    pub from: Vec3,
    pub to: Vec3,
    /// Transform of rotated elements; others are axis aligned.
    pub matrix: Option<Mat4>,
    pub faces: [Option<RenderFace>; 6],
}

/// A block model with its variant rotation applied (`obj_block_render_model`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderModel {
    pub elements: Vec<RenderElement>,
    pub weight: f64,
    /// Whether each face direction is fully covered, for hiding the faces
    /// of neighbours.
    pub face_full: [bool; 6],
}

/// A parsed block model file.
#[derive(Debug, Clone)]
struct ModelJson {
    parent: Option<String>,
    textures: Vec<(String, String)>,
    elements: Option<Vec<ElementJson>>,
}

#[derive(Debug, Clone)]
struct FaceJson {
    uv: Option<([f64; 2], [f64; 2])>,
    texture: String,
    rotation: f64,
}

#[derive(Debug, Clone)]
struct ElementJson {
    from: Vec3,
    to: Vec3,
    matrix: Option<Mat4>,
    faces: [Option<FaceJson>; 6],
}

/// `value_get_point3D`: Minecraft [x, y, z] is [x, z, y] here.
fn point(json: Option<&Json>) -> Option<Vec3> {
    let list = json?.as_array()?;
    if list.len() < 3 {
        return None;
    }
    Some([list[0].as_real()?, list[2].as_real()?, list[1].as_real()?])
}

fn snap(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

fn strip_namespace(name: &str) -> String {
    name.trim_start_matches("minecraft:").to_owned()
}

fn parse_model(bytes: &[u8]) -> Option<ModelJson> {
    let root = json::parse(bytes).ok()?;
    let map = root.as_object()?;
    let textures = match map.get("textures") {
        Some(Json::Object(textures)) => textures
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.to_owned(), strip_namespace(v).to_lowercase())))
            .collect(),
        Some(Json::Array(list)) => list
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_str().map(|v| (i.to_string(), strip_namespace(v).to_lowercase())))
            .collect(),
        _ => Vec::new(),
    };
    let elements = map.array("elements").map(|list| {
        let mut elements: Vec<ElementJson> = list
            .iter()
            .filter_map(Json::as_object)
            .filter_map(|e| {
                let from = point(e.get("from"))?;
                let to = point(e.get("to"))?;
                let matrix = e.object("rotation").map(|rotation| {
                    let origin = point(rotation.get("origin")).unwrap_or([8.0; 3]);
                    let angle = snap(rotation.real("angle").unwrap_or(0.0).clamp(-45.0, 45.0), 22.5);
                    let rescale = rotation.flag("rescale").unwrap_or(false);
                    let mut scale = [if rescale { 1.0 / angle.abs().to_radians().cos() } else { 1.0 }; 3];
                    let mut rot = [0.0; 3];
                    let axis = match rotation.string("axis") {
                        Some("x") => Some(0),
                        Some("z") => Some(1),
                        Some("y") => Some(2),
                        _ => None,
                    };
                    if let Some(axis) = axis {
                        rot[axis] = angle;
                        scale[axis] = 1.0;
                    }
                    Mat4::translation(origin.map(|c| -c))
                        .then(&Mat4::scaling(scale))
                        .then(&Mat4::build(origin, rot, [1.0; 3]))
                });
                let faces_map = e.object("faces");
                let faces = Dir::ALL.map(|dir| {
                    let face = faces_map?.object(dir.name())?;
                    let uv = face.array("uv").and_then(|uv| {
                        let n = |i: usize| uv.get(i).and_then(Json::as_real);
                        let limit = |v: f64| v.rem_euclid(BLOCK + 0.1);
                        Some(([limit(n(0)?), limit(n(1)?)], [limit(n(2)?), limit(n(3)?)]))
                    });
                    Some(FaceJson {
                        uv,
                        texture: face.string("texture").unwrap_or("").to_owned(),
                        rotation: face.real("rotation").unwrap_or(0.0),
                    })
                });
                Some(ElementJson { from, to, matrix, faces })
            })
            .collect();
        // Smaller elements first, as the original sorts them.
        let volume = |e: &ElementJson| (0..3).map(|i| e.to[i] - e.from[i]).product::<f64>();
        elements.sort_by(|a, b| volume(a).total_cmp(&volume(b)));
        elements
    });
    Some(ModelJson { parent: map.string("parent").map(strip_namespace), textures, elements })
}

fn rotate_uv(uv: [f64; 2], degrees: f64) -> [f64; 2] {
    let m = Mat4::translation([-BLOCK / 2.0, -BLOCK / 2.0, 0.0])
        .then(&Mat4::build([BLOCK / 2.0, BLOCK / 2.0, 0.0], [0.0, 0.0, degrees], [1.0; 3]));
    let p = m.transform_point([uv[0], uv[1], 0.0]);
    [p[0], p[1]]
}

/// Variant or multipart part of a blockstate file.
#[derive(Debug, Clone, PartialEq)]
struct VariantModel {
    model: String,
    x: f64,
    y: f64,
    uvlock: bool,
    weight: f64,
}

fn variant_models(json: &Json) -> Vec<VariantModel> {
    let one = |map: &JsonObject| -> Option<VariantModel> {
        Some(VariantModel {
            model: strip_namespace(map.string("model")?),
            x: snap(map.real("x").unwrap_or(0.0), 90.0).clamp(0.0, 270.0),
            y: snap(map.real("y").unwrap_or(0.0), 90.0).clamp(0.0, 270.0),
            uvlock: match map.get("uvlock") {
                Some(Json::String(s)) => s == "true",
                Some(other) => other.as_flag().unwrap_or(false),
                None => false,
            },
            weight: map.real("weight").unwrap_or(1.0),
        })
    };
    match json {
        Json::Array(list) => list.iter().filter_map(Json::as_object).filter_map(one).collect(),
        Json::Object(map) => one(map).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn condition_value(json: &Json) -> String {
    match json {
        Json::Bool(b) => b.to_string(),
        Json::String(s) => s.clone(),
        other => other.as_real().map(json::format_number).unwrap_or_default(),
    }
}

/// Whether every condition holds; values may list alternatives with `|`.
/// Conditions on states the block does not declare fail.
fn conditions_match(conditions: &JsonObject, state: &[(String, String)]) -> bool {
    conditions.iter().all(|(name, value)| {
        let wanted = condition_value(value);
        match state.iter().find(|(n, _)| n == name) {
            Some((_, actual)) => wanted.split('|').any(|w| w == actual),
            None => false,
        }
    })
}

/// The models a blockstate file gives for a state.
fn state_file_models(map: &JsonObject, state: &[(String, String)]) -> Vec<Vec<VariantModel>> {
    if let Some(variants) = map.object("variants") {
        // Variables of the key that the block does not declare are ignored,
        // as in the original.
        let matches = |key: &str| {
            parse_state_vars(key).iter().all(|(name, value)| match state.iter().find(|(n, _)| n == name) {
                Some((_, actual)) => actual == value,
                None => true,
            })
        };
        let chosen = variants
            .iter()
            .find(|(key, _)| !key.is_empty() && *key != "normal" && matches(key))
            .or_else(|| variants.iter().find(|(key, _)| key.is_empty() || *key == "normal"))
            .or_else(|| variants.iter().next());
        return chosen.map(|(_, models)| vec![variant_models(models)]).unwrap_or_default();
    }
    let Some(parts) = map.array("multipart") else { return Vec::new() };
    parts
        .iter()
        .filter_map(Json::as_object)
        .filter(|part| match part.object("when") {
            None => true,
            Some(when) if when.is_empty() => true,
            Some(when) => {
                if let Some(any) = when.array("OR") {
                    any.iter().filter_map(Json::as_object).any(|c| conditions_match(c, state))
                } else if let Some(all) = when.array("AND") {
                    all.iter().filter_map(Json::as_object).all(|c| conditions_match(c, state))
                } else {
                    conditions_match(when, state)
                }
            }
        })
        .filter_map(|part| part.get("apply").map(variant_models))
        .collect()
}

/// Block data of the asset pack, loaded on demand and cached.
pub struct Blocks {
    defs: HashMap<String, BlockDef>,
    /// `minecraft:` id → block name and state.
    ids: HashMap<String, (String, Vec<(String, String)>)>,
    models: Mutex<HashMap<String, Option<Arc<ModelJson>>>>,
    state_files: Mutex<HashMap<String, Option<Arc<JsonObject>>>>,
}

impl std::fmt::Debug for Blocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blocks").field("blocks", &self.defs.len()).finish_non_exhaustive()
    }
}

impl Blocks {
    pub fn load(pack: &AssetPack) -> Self {
        let mut defs = HashMap::new();
        let mut ids = HashMap::new();
        for def in pack.manifest().array("blocks").unwrap_or_default().iter().filter_map(Json::as_object) {
            if let Some(def) = BlockDef::load(def) {
                for (id, state) in &def.ids {
                    ids.insert(id.clone(), (def.name.clone(), state.clone()));
                }
                defs.insert(def.name.clone(), def);
            }
        }
        Self { defs, ids, models: Mutex::default(), state_files: Mutex::default() }
    }

    pub fn def(&self, name: &str) -> Option<&BlockDef> {
        self.defs.get(name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }

    /// The block and state a Minecraft id such as `minecraft:oak_stairs`
    /// stands for.
    pub fn by_id(&self, id: &str) -> Option<(&BlockDef, &[(String, String)])> {
        let key = if id.contains(':') { id.to_owned() } else { format!("minecraft:{id}") };
        let (name, state) = self.ids.get(&key)?;
        Some((self.defs.get(name)?, state))
    }

    fn model(&self, pack: &AssetPack, name: &str) -> Option<Arc<ModelJson>> {
        if let Some(model) = self.models.lock().unwrap_or_else(|e| e.into_inner()).get(name) {
            return model.clone();
        }
        let model = pack.read(&format!("models/{name}.json")).and_then(|b| parse_model(&b)).map(Arc::new);
        self.models.lock().unwrap_or_else(|e| e.into_inner()).insert(name.to_owned(), model.clone());
        model
    }

    fn state_file(&self, pack: &AssetPack, file: &str) -> Option<Arc<JsonObject>> {
        if let Some(map) = self.state_files.lock().unwrap_or_else(|e| e.into_inner()).get(file) {
            return map.clone();
        }
        let map = pack
            .read(&format!("blockstates/{file}"))
            .and_then(|bytes| json::parse(&bytes).ok())
            .and_then(|json| match json {
                Json::Object(map) => Some(Arc::new(map)),
                _ => None,
            });
        self.state_files.lock().unwrap_or_else(|e| e.into_inner()).insert(file.to_owned(), map.clone());
        map
    }

    /// `block_load_render_model`: a model with the rotation of a variant.
    // Face directions index several parallel tables, as in the original.
    #[allow(clippy::needless_range_loop)]
    fn render_model(&self, pack: &AssetPack, variant: &VariantModel) -> Option<RenderModel> {
        // Textures are inherited down the parent chain, children winning;
        // elements come from the nearest model that has any.
        let mut textures: HashMap<String, String> = HashMap::new();
        let mut elements = None;
        let mut current = Some(variant.model.clone());
        let mut depth = 0;
        while let Some(name) = current {
            depth += 1;
            if depth > 32 {
                break;
            }
            let model = self.model(pack, &name)?;
            for (key, value) in &model.textures {
                textures.entry(key.clone()).or_insert_with(|| value.clone());
            }
            if elements.is_none() && model.elements.as_ref().is_some_and(|e| !e.is_empty()) {
                elements = model.elements.clone();
            }
            current = model.parent.clone();
        }
        let elements = elements.unwrap_or_default();

        let (rot_x, rot_z) = (variant.x, variant.y);
        let rotated_variant = rot_x > 0.0 || rot_z > 0.0;
        let rotmat = if rotated_variant {
            Mat4::translation([-BLOCK / 2.0; 3]).then(&Mat4::build([BLOCK / 2.0; 3], [-rot_x, 0.0, -rot_z], [1.0; 3]))
        } else {
            Mat4::IDENTITY
        };

        let resolve_texture = |name: &str| -> Option<String> {
            let mut name = name.to_owned();
            let mut guard = 0;
            while let Some(reference) = name.strip_prefix('#') {
                guard += 1;
                name = textures.get(reference)?.clone();
                if guard > 16 {
                    return None;
                }
            }
            (!name.is_empty()).then_some(name)
        };

        let mut model = RenderModel { elements: Vec::new(), weight: variant.weight, face_full: [false; 6] };
        let mut face_min: [Option<f64>; 6] = [None; 6];
        let mut face_max: [Option<f64>; 6] = [None; 6];

        for element in &elements {
            let mut from = element.from;
            let mut to = element.to;
            let mut new_dir: [usize; 6] = [0, 1, 2, 3, 4, 5];
            let mut uv_rot = [0.0f64; 6];
            let rotated = element.matrix.is_some();
            let matrix = element.matrix.map(|m| m.then(&rotmat));

            if !rotated && rotated_variant {
                let a = rotmat.transform_point(from);
                let b = rotmat.transform_point(to);
                for i in 0..3 {
                    from[i] = snap(a[i].min(b[i]), 0.01);
                    to[i] = snap(a[i].max(b[i]), 0.01);
                }
                let (e, w, s, n, u, d) = (0, 1, 2, 3, 4, 5);
                for _ in 0..(rot_z / 90.0) as usize {
                    let east = new_dir[e];
                    new_dir[e] = new_dir[s];
                    new_dir[s] = new_dir[w];
                    new_dir[w] = new_dir[n];
                    new_dir[n] = east;
                }
                for _ in 0..(rot_x / 90.0) as usize {
                    let up = new_dir[u];
                    new_dir[u] = new_dir[n];
                    new_dir[n] = new_dir[d];
                    new_dir[d] = new_dir[s];
                    new_dir[s] = up;
                }
                match rot_x as i64 {
                    0 => {
                        uv_rot[u] = rot_z;
                        uv_rot[d] = -rot_z;
                    }
                    90 => {
                        uv_rot[new_dir[e]] = 90.0;
                        uv_rot[new_dir[w]] = -90.0;
                        uv_rot[new_dir[u]] = 180.0;
                        uv_rot[u] = rot_z;
                        uv_rot[d] = 180.0 - rot_z;
                    }
                    180 => {
                        for dir in [e, w, s, n] {
                            uv_rot[dir] = 180.0;
                        }
                        uv_rot[u] = rot_z;
                        uv_rot[d] = -rot_z;
                    }
                    270 => {
                        uv_rot[new_dir[e]] = -90.0;
                        uv_rot[new_dir[w]] = 90.0;
                        uv_rot[new_dir[d]] = 180.0;
                        uv_rot[u] = 180.0 + rot_z;
                        uv_rot[d] = -rot_z;
                    }
                    _ => {}
                }
            }

            let mut faces: [Option<RenderFace>; 6] = Default::default();
            for f in 0..6 {
                let nd = new_dir[f];
                let Some(face) = &element.faces[f] else { continue };
                let mut face_rot = face.rotation;
                if !variant.uvlock {
                    face_rot += uv_rot[nd].rem_euclid(360.0);
                }

                let mut uv = match face.uv {
                    Some((uv_from, uv_to)) => {
                        let (uv_from, uv_to) = if variant.uvlock && uv_rot[nd] != 0.0 {
                            let a = rotate_uv(uv_from, -uv_rot[nd]);
                            let b = rotate_uv(uv_to, -uv_rot[nd]);
                            (
                                [snap(a[0].min(b[0]), 0.01), snap(a[1].min(b[1]), 0.01)],
                                [snap(a[0].max(b[0]), 0.01), snap(a[1].max(b[1]), 0.01)],
                            )
                        } else {
                            (uv_from, uv_to)
                        };
                        let mut uv = [uv_from, [uv_to[0], uv_from[1]], uv_to, [uv_from[0], uv_to[1]]];
                        for _ in 0..(face_rot / 90.0).max(0.0) as usize {
                            uv.rotate_right(1);
                        }
                        uv
                    }
                    None => {
                        let b = BLOCK;
                        let p = |u: f64, v: f64| [u, v];
                        let mut uv = match Dir::ALL[nd] {
                            Dir::East => [p(b - to[1], b - to[2]), p(b - from[1], b - to[2]), p(b - from[1], b - from[2]), p(b - to[1], b - from[2])],
                            Dir::West => [p(from[1], b - to[2]), p(to[1], b - to[2]), p(to[1], b - from[2]), p(from[1], b - from[2])],
                            Dir::South => [p(from[0], b - to[2]), p(to[0], b - to[2]), p(to[0], b - from[2]), p(from[0], b - from[2])],
                            Dir::North => [p(b - to[0], b - to[2]), p(b - from[0], b - to[2]), p(b - from[0], b - from[2]), p(b - to[0], b - from[2])],
                            Dir::Up => [p(from[0], from[1]), p(to[0], from[1]), p(to[0], to[1]), p(from[0], to[1])],
                            Dir::Down => [p(from[0], b - to[1]), p(to[0], b - to[1]), p(to[0], b - from[1]), p(from[0], b - from[1])],
                        };
                        if face_rot > 0.0 {
                            uv = uv.map(|c| rotate_uv(c, face_rot));
                        }
                        uv
                    }
                };
                // Keep neighbouring texels out of the face.
                for corner in &mut uv {
                    corner[0] = corner[0].min(BLOCK - 1.0 / 256.0);
                    corner[1] = corner[1].min(BLOCK - 1.0 / 256.0);
                }

                // With UV lock the face keeps the texture of the direction
                // it ends up in, as in the original.
                let texture_source = match (&element.faces[nd], variant.uvlock) {
                    (Some(target), true) => &target.texture,
                    _ => &face.texture,
                };
                let Some(texture) = resolve_texture(texture_source) else { continue };

                let edge = !rotated
                    && match Dir::ALL[nd] {
                        Dir::East => to[0] == BLOCK,
                        Dir::West => from[0] == 0.0,
                        Dir::South => to[1] == BLOCK,
                        Dir::North => from[1] == 0.0,
                        Dir::Up => to[2] == BLOCK,
                        Dir::Down => from[2] == 0.0,
                    };
                if edge && !model.face_full[nd] {
                    // The two axes across the face; it is full when it spans
                    // the first and covers 0..16 along the second.
                    let (across, along) = match Dir::ALL[nd] {
                        Dir::East | Dir::West => (1, 2),
                        Dir::South | Dir::North => (0, 2),
                        Dir::Up | Dir::Down => (0, 1),
                    };
                    if from[across] == 0.0 && to[across] == BLOCK {
                        if face_min[nd].is_none_or(|m| from[along] <= m) {
                            face_min[nd] = Some(from[along]);
                        }
                        if face_max[nd].is_none_or(|m| to[along] >= m) {
                            face_max[nd] = Some(to[along]);
                        }
                        if face_min[nd] == Some(0.0) && face_max[nd] == Some(BLOCK) {
                            model.face_full[nd] = true;
                        }
                    }
                }
                faces[nd] = Some(RenderFace { uv, texture, edge });
            }
            model.elements.push(RenderElement { from, to, matrix, faces });
        }
        Some(model)
    }

    /// The render models of a block in a state: one list of weighted
    /// alternatives per model that applies (variants give one list,
    /// multipart blocks one per matching part).
    pub fn models(&self, pack: &AssetPack, block: &BlockDef, state: &[(String, String)]) -> Vec<Vec<RenderModel>> {
        // A state value can switch to another blockstate file.
        let mut file = block.file.clone();
        for (name, values) in &block.states {
            let Some((_, value)) = state.iter().find(|(n, _)| n == name) else { continue };
            if let Some(option) = values.iter().find(|v| &v.value == value) {
                if option.file.is_some() {
                    file = option.file.clone();
                }
            }
        }
        let Some(file) = file else { return Vec::new() };
        let Some(map) = self.state_file(pack, &file) else { return Vec::new() };
        state_file_models(&map, state)
            .iter()
            .map(|choices| choices.iter().filter_map(|v| self.render_model(pack, v)).collect::<Vec<_>>())
            .filter(|choices| !choices.is_empty())
            .collect()
    }

    /// Meshes of a block in a state repeated `size` times along each axis,
    /// grouped by texture (`temp_update_block`). Faces between neighbours
    /// that cover each other are left out; the outer faces are kept.
    pub fn grid_meshes(
        &self,
        pack: &AssetPack,
        block: &BlockDef,
        state: &[(String, String)],
        size: [usize; 3],
        randomize: bool,
    ) -> Vec<(String, MeshData)> {
        let models = self.models(pack, block, state);
        if models.is_empty() {
            return Vec::new();
        }
        let emissive = self.emissive(block, state);
        let size = size.map(|s| s.max(1));
        let seed = |p: [i64; 3]| (p[0] as u64).wrapping_mul(3_129_871) ^ (p[1] as u64).wrapping_mul(116_129_781) ^ (p[2] as u64);
        let chosen = |p: [i64; 3]| -> Vec<&RenderModel> {
            models.iter().filter_map(|choices| pick_weighted(choices, seed(p), randomize)).collect()
        };
        let inside = |p: [i64; 3]| (0..3).all(|i| p[i] >= 0 && p[i] < size[i] as i64);

        let mut out: HashMap<String, MeshData> = HashMap::new();
        for z in 0..size[2] as i64 {
            for y in 0..size[1] as i64 {
                for x in 0..size[0] as i64 {
                    let p = [x, y, z];
                    let here = chosen(p);
                    let hidden = |dir: Dir| {
                        let s = dir.step();
                        let n = [p[0] + s[0], p[1] + s[1], p[2] + s[2]];
                        inside(n) && chosen(n).iter().any(|m| m.face_full[dir.opposite().index()])
                    };
                    let offset = [x as f64 * BLOCK, y as f64 * BLOCK, z as f64 * BLOCK];
                    block_mesh(&here, offset, emissive, &hidden, &mut out);
                }
            }
        }
        let mut meshes: Vec<(String, MeshData)> = out.into_iter().collect();
        meshes.sort_by(|a, b| a.0.cmp(&b.0));
        meshes
    }

    /// Emission of a block in a state.
    pub fn emissive(&self, block: &BlockDef, state: &[(String, String)]) -> f64 {
        let mut emissive = block.emissive;
        for (name, values) in &block.states {
            let Some((_, value)) = state.iter().find(|(n, _)| n == name) else { continue };
            if let Some(e) = values.iter().find(|v| &v.value == value).and_then(|v| v.emissive) {
                emissive = e;
            }
        }
        emissive
    }
}

/// Faces of a block grouped by texture, positioned at `offset` (in units,
/// one block is 16). `hidden(dir)` tells whether faces on that side of the
/// block are covered by a neighbour.
pub fn block_mesh(
    models: &[&RenderModel],
    offset: Vec3,
    emissive: f64,
    hidden: &dyn Fn(Dir) -> bool,
    out: &mut HashMap<String, MeshData>,
) {
    let custom = [0.0, 0.0, emissive as f32, 0.0];
    for model in models {
        for element in &model.elements {
            let (mut f, mut t) = (element.from, element.to);
            let matrix = element.matrix.map(|m| m.then(&Mat4::translation(offset)));
            if matrix.is_none() {
                for i in 0..3 {
                    f[i] += offset[i];
                    t[i] += offset[i];
                }
            }
            let (x1, y1, z1, x2, y2, z2) = (f[0], f[1], f[2], t[0], t[1], t[2]);
            for dir in Dir::ALL {
                let Some(face) = &element.faces[dir.index()] else { continue };
                if face.edge && hidden(dir) {
                    continue;
                }
                let corners = match dir {
                    Dir::East => [[x2, y2, z2], [x2, y1, z2], [x2, y1, z1], [x2, y2, z1]],
                    Dir::West => [[x1, y1, z2], [x1, y2, z2], [x1, y2, z1], [x1, y1, z1]],
                    Dir::South => [[x1, y2, z2], [x2, y2, z2], [x2, y2, z1], [x1, y2, z1]],
                    Dir::North => [[x2, y1, z2], [x1, y1, z2], [x1, y1, z1], [x2, y1, z1]],
                    Dir::Up => [[x1, y1, z2], [x2, y1, z2], [x2, y2, z2], [x1, y2, z2]],
                    Dir::Down => [[x1, y2, z1], [x2, y2, z1], [x2, y1, z1], [x1, y1, z1]],
                };
                let corners = match &matrix {
                    Some(m) => corners.map(|c| m.transform_point(c)),
                    None => corners,
                };
                let p = corners.map(|c| c.map(|v| v as f32));
                let uv = face.uv.map(|c| [(c[0] / BLOCK) as f32, (c[1] / BLOCK) as f32]);
                let mesh = out.entry(face.texture.clone()).or_default();
                mesh.triangle_with([p[0], p[1], p[2]], [uv[0], uv[1], uv[2]], None, false, [custom; 3]);
                mesh.triangle_with([p[2], p[3], p[0]], [uv[2], uv[3], uv[0]], None, false, [custom; 3]);
            }
        }
    }
}

/// Picks one of several weighted alternatives from a seed (the block
/// position), as the builder's randomised variants do.
pub fn pick_weighted(choices: &[RenderModel], seed: u64, randomize: bool) -> Option<&RenderModel> {
    if !randomize || choices.len() == 1 {
        return choices.first();
    }
    let total: f64 = choices.iter().map(|c| c.weight.max(0.0)).sum();
    if total <= 0.0 {
        return choices.first();
    }
    // A small hash so neighbouring blocks differ.
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    let mut target = (h % 1_000_000) as f64 / 1_000_000.0 * total;
    for choice in choices {
        target -= choice.weight.max(0.0);
        if target < 0.0 {
            return Some(choice);
        }
    }
    choices.last()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_keys_match_declared_states_only() {
        let doc = json::parse(
            br#"{ "variants": {
                "facing=east,half=bottom": { "model": "minecraft:block/a" },
                "facing=north,half=top": { "model": "block/b", "x": 180, "y": 90, "uvlock": true }
            } }"#,
        )
        .unwrap();
        let map = doc.as_object().unwrap();
        let state = vec![("facing".to_owned(), "north".to_owned()), ("half".to_owned(), "top".to_owned())];
        let models = state_file_models(map, &state);
        assert_eq!(models[0][0].model, "block/b");
        assert_eq!((models[0][0].x, models[0][0].y, models[0][0].uvlock), (180.0, 90.0, true));
        // "half" not declared: ignored.
        let state = vec![("facing".to_owned(), "east".to_owned())];
        assert_eq!(state_file_models(map, &state)[0][0].model, "block/a");
    }

    #[test]
    fn multipart_conditions() {
        let doc = json::parse(
            br#"{ "multipart": [
                { "apply": { "model": "post" } },
                { "when": { "north": "true" }, "apply": { "model": "side" } },
                { "when": { "OR": [ { "east": "low|tall" }, { "west": true } ] }, "apply": [ { "model": "x" }, { "model": "y", "weight": 3 } ] }
            ] }"#,
        )
        .unwrap();
        let map = doc.as_object().unwrap();
        let s = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let names = |state: &[(String, String)]| {
            state_file_models(map, state).iter().map(|c| c[0].model.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names(&s(&[("north", "false"), ("east", "none"), ("west", "false")])), ["post"]);
        assert_eq!(names(&s(&[("north", "true"), ("east", "tall"), ("west", "false")])), ["post", "side", "x"]);
        assert_eq!(names(&s(&[("north", "false"), ("east", "none"), ("west", "true")])), ["post", "x"]);
    }

    #[test]
    fn weighted_choice_is_stable() {
        let choices = vec![
            RenderModel { weight: 1.0, ..Default::default() },
            RenderModel { weight: 3.0, ..Default::default() },
        ];
        assert!(std::ptr::eq(pick_weighted(&choices, 5, false).unwrap(), &choices[0]));
        let a = pick_weighted(&choices, 42, true).unwrap() as *const _;
        let b = pick_weighted(&choices, 42, true).unwrap() as *const _;
        assert_eq!(a, b);
        let second = (0..1000).filter(|&i| std::ptr::eq(pick_weighted(&choices, i, true).unwrap(), &choices[1])).count();
        assert!((600..900).contains(&second), "{second}");
    }
}
