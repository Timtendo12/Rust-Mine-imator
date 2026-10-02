//! Library templates (`obj_template`): the definition that timelines are
//! instances of.

use super::particles::ParticleSpawner;
use super::LoadContext;
use crate::json::{JsonObject, JsonWriter};
use crate::record::{load_obj, load_state_vars, record, save_obj, save_state_vars, StateValue};
use mi_core::version::project as fmt;
use mi_core::{Color, ObjRef, SaveId, TempType};

/// Default dye of leather armour (`other:leather` swatch).
pub const LEATHER_COLOR: Color = Color::rgb(0xA0, 0x65, 0x40);

/// Banner / shield / decorated pot pattern. Colours are dye names
/// (`white`, `light_blue`, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub base_color: String,
    pub patterns: Vec<String>,
    pub colors: Vec<String>,
}

impl Default for Pattern {
    fn default() -> Self {
        Self { base_color: "white".to_owned(), patterns: Vec::new(), colors: Vec::new() }
    }
}

impl Pattern {
    /// Reads the pattern keys of a template or timeline. Before 2.0.0 the
    /// keys were called `banner_*`.
    pub(crate) fn load(map: &JsonObject, format: i32) -> Self {
        let prefix = if format < fmt::FORMAT_200_PRE_5 { "banner" } else { "pattern" };
        let strings = |key: String| -> Vec<String> {
            map.array(&key)
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        Self {
            base_color: map.string(&format!("{prefix}_base_color")).unwrap_or("white").to_owned(),
            patterns: strings(format!("{prefix}_pattern_list")),
            colors: strings(format!("{prefix}_color_list")),
        }
    }

    pub(crate) fn has_keys(map: &JsonObject, format: i32) -> bool {
        map.contains(if format < fmt::FORMAT_200_PRE_5 { "banner_base_color" } else { "pattern_base_color" })
    }

    pub(crate) fn save_list(w: &mut JsonWriter, key: &str, items: &[String]) {
        w.array_start(Some(key));
        for item in items {
            w.array_value(item);
        }
        w.array_done();
    }
}

/// One armour slot of an `armor` model template.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmorPiece {
    pub dye: Color,
    pub trim_pattern: String,
    pub trim_material: String,
}

impl Default for ArmorPiece {
    fn default() -> Self {
        Self {
            dye: LEATHER_COLOR,
            trim_pattern: "none".to_owned(),
            // First entry of the trim material list in the Minecraft assets.
            trim_material: "amethyst".to_owned(),
        }
    }
}

/// Armour slots in file order.
pub const ARMOR_SLOTS: [&str; 4] = ["helmet", "chestplate", "leggings", "boots"];

record! {
    /// Settings of item templates (`item` object).
    pub struct ItemSettings {
        "3d" => is_3d: bool = true,
        "face_camera" => face_camera: bool = false,
        "bounce" => bounce: bool = false,
        "spin" => spin: bool = false,
    }
}

record! {
    /// Settings of shape templates (`shape` object).
    pub struct ShapeSettings {
        "tex" => tex: obj = ObjRef::Null,
        "tex_material" => tex_material: obj = ObjRef::Null,
        "tex_normal" => tex_normal: obj = ObjRef::Null,
        "tex_mapped" => tex_mapped: bool = false,
        "tex_hoffset" => tex_hoffset: num = 0,
        "tex_voffset" => tex_voffset: num = 0,
        "tex_hrepeat" => tex_hrepeat: num = 1,
        "tex_vrepeat" => tex_vrepeat: num = 1,
        "tex_hmirror" => tex_hmirror: bool = false,
        "tex_vmirror" => tex_vmirror: bool = false,
        "closed" => closed: bool = true,
        "invert" => invert: bool = false,
        "detail" => detail: num = 32,
        "face_camera" => face_camera: bool = false,
    }
}

record! {
    /// Settings of text templates (`text` object).
    pub struct TextSettings {
        "font" => font: obj = ObjRef::Null,
        "3d" => is_3d: bool = false,
        "face_camera" => face_camera: bool = false,
    }
}

/// A library item.
///
/// Like the original object, a template carries the settings of every kind;
/// only the ones that apply to [`Template::kind`] are written to files.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub id: SaveId,
    pub kind: TempType,
    pub name: String,

    // Characters, special blocks, body parts and custom models
    /// Model resource of a custom model template.
    pub model: ObjRef,
    pub model_tex: ObjRef,
    pub model_tex_material: ObjRef,
    pub model_tex_normal: ObjRef,
    pub model_use_blend_color: bool,
    pub model_blend_color: Color,
    pub model_name: String,
    pub model_state: Vec<(String, StateValue)>,
    pub model_part_name: String,
    /// Version of the model definition the template was saved with; older
    /// versions get their states and parts updated when assets are applied.
    pub model_version: f64,
    pub pattern: Option<Pattern>,
    pub armor: [ArmorPiece; 4],

    // Items
    pub item_tex: ObjRef,
    pub item_tex_material: ObjRef,
    pub item_tex_normal: ObjRef,
    /// Item texture name when the texture comes from a resource pack.
    pub item_name: Option<String>,
    /// Slot in the item sheet, used when there is no name.
    pub item_slot: f64,
    pub item: ItemSettings,

    // Blocks and scenery
    pub block_name: String,
    pub block_state: Vec<(String, StateValue)>,
    /// Numeric block id and data of projects older than 1.2.0, to be
    /// translated with the legacy tables.
    pub legacy_block: Option<(f64, f64)>,
    pub block_tex: ObjRef,
    pub block_tex_material: ObjRef,
    pub block_tex_normal: ObjRef,
    pub block_randomize: bool,
    pub block_repeat_enable: bool,
    pub block_repeat: [f64; 3],
    pub scenery: ObjRef,

    pub shape: ShapeSettings,
    pub text: TextSettings,
    pub particles: Option<Box<ParticleSpawner>>,
}

impl Template {
    /// A template with the settings of `temp_event_create`.
    pub fn new(id: SaveId, kind: TempType) -> Self {
        Self {
            id,
            kind,
            name: String::new(),
            model: ObjRef::Null,
            model_tex: ObjRef::Null,
            model_tex_material: ObjRef::Null,
            model_tex_normal: ObjRef::Null,
            model_use_blend_color: false,
            model_blend_color: Color::WHITE,
            model_name: "human".to_owned(),
            model_state: Vec::new(),
            model_part_name: String::new(),
            model_version: 0.0,
            pattern: None,
            armor: Default::default(),
            item_tex: ObjRef::Null,
            item_tex_material: ObjRef::Null,
            item_tex_normal: ObjRef::Null,
            item_name: None,
            item_slot: 0.0,
            item: ItemSettings::default(),
            block_name: "grass_block".to_owned(),
            block_state: Vec::new(),
            legacy_block: None,
            block_tex: ObjRef::Null,
            block_tex_material: ObjRef::Null,
            block_tex_normal: ObjRef::Null,
            block_randomize: true,
            block_repeat_enable: false,
            block_repeat: [1.0; 3],
            scenery: ObjRef::Null,
            shape: ShapeSettings::default(),
            text: TextSettings::default(),
            particles: (kind == TempType::ParticleSpawner).then(Default::default),
        }
    }

    /// `project_save_template`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(None);
        w.var("id", self.id.as_str());
        w.var("type", self.kind.name());
        w.var("name", &self.name);

        match self.kind {
            TempType::Character | TempType::SpecialBlock | TempType::Bodypart => {
                save_obj(w, "model_tex", &self.model_tex);
                save_obj(w, "model_tex_material", &self.model_tex_material);
                save_obj(w, "model_tex_normal", &self.model_tex_normal);
                w.var_bool("model_use_blend_color", self.model_use_blend_color);
                w.var_color("model_blend_color", self.model_blend_color);

                w.object_start(Some("model"));
                w.var("name", &self.model_name);
                save_state_vars(w, "state", &self.model_state);
                if self.kind == TempType::Bodypart {
                    w.var("part_name", &self.model_part_name);
                } else {
                    w.var("model_version", self.model_version);
                }
                w.object_done();

                if let Some(pattern) = &self.pattern {
                    w.var("pattern_base_color", &pattern.base_color);
                    Pattern::save_list(w, "pattern_pattern_list", &pattern.patterns);
                    Pattern::save_list(w, "pattern_color_list", &pattern.colors);
                }

                if self.model_name == "armor" {
                    w.object_start(Some("armor"));
                    for (slot, piece) in ARMOR_SLOTS.iter().zip(&self.armor) {
                        w.var_color(&format!("{slot}_dye"), piece.dye);
                        w.var(&format!("{slot}_trim_pattern"), &piece.trim_pattern);
                        w.var(&format!("{slot}_trim_material"), &piece.trim_material);
                    }
                    w.object_done();
                }
            }
            TempType::Item => {
                w.object_start(Some("item"));
                save_obj(w, "tex", &self.item_tex);
                save_obj(w, "tex_material", &self.item_tex_material);
                save_obj(w, "tex_normal", &self.item_tex_normal);
                match &self.item_name {
                    Some(name) => w.var("name", name),
                    None => w.var("slot", self.item_slot),
                }
                self.item.save_fields(w);
                w.object_done();
            }
            TempType::Block => {
                w.object_start(Some("block"));
                w.var("name", &self.block_name);
                save_state_vars(w, "state", &self.block_state);
                save_obj(w, "tex", &self.block_tex);
                save_obj(w, "tex_material", &self.block_tex_material);
                save_obj(w, "tex_normal", &self.block_tex_normal);
                w.var_bool("randomize", self.block_randomize);
                w.var_bool("repeat_enable", self.block_repeat_enable);
                w.var_point3("repeat", self.block_repeat);
                w.object_done();
            }
            TempType::Scenery => {
                save_obj(w, "scenery", &self.scenery);
                w.object_start(Some("block"));
                save_obj(w, "tex", &self.block_tex);
                save_obj(w, "tex_material", &self.block_tex_material);
                save_obj(w, "tex_normal", &self.block_tex_normal);
                w.var_bool("repeat_enable", self.block_repeat_enable);
                w.var_point3("repeat", self.block_repeat);
                w.object_done();
            }
            TempType::Model => {
                save_obj(w, "model", &self.model);
                save_obj(w, "model_tex", &self.model_tex);
                save_obj(w, "model_tex_material", &self.model_tex_material);
                save_obj(w, "model_tex_normal", &self.model_tex_normal);
            }
            _ => {}
        }

        if self.kind.is_shape() {
            w.object_start(Some("shape"));
            self.shape.save_fields(w);
            w.object_done();
        } else if self.kind == TempType::Text {
            w.object_start(Some("text"));
            self.text.save_fields(w);
            w.object_done();
        } else if self.kind == TempType::ParticleSpawner {
            match &self.particles {
                Some(particles) => particles.save(w),
                None => ParticleSpawner::default().save(w),
            }
        }

        w.object_done();
    }

    /// `project_load_template`. Returns `None` (with a warning) when the
    /// entry cannot be used.
    pub(crate) fn load(map: &JsonObject, ctx: &mut LoadContext) -> Option<Self> {
        let format = ctx.format;
        let type_name = map.string("type").unwrap_or("");
        let Some(kind) = TempType::from_name(type_name) else {
            ctx.warn(format!("Skipped a template of unknown type \"{type_name}\""));
            return None;
        };
        let id = match map.string("id") {
            Some(id) => SaveId::new(id),
            None => ctx.new_id(),
        };
        let mut temp = Template::new(id, kind);
        if let Some(name) = map.string("name") {
            temp.name = name.to_owned();
        }

        let has_material_maps = format >= fmt::FORMAT_200_PRE_5;
        let load_into = |map: &JsonObject, key: &str, target: &mut ObjRef| {
            if let Some(v) = load_obj(map, key) {
                *target = v;
            }
        };

        match kind {
            TempType::Character | TempType::SpecialBlock | TempType::Bodypart => {
                let tex_key = if format == fmt::FORMAT_110_PRE_1 { "skin" } else { "model_tex" };
                load_into(map, tex_key, &mut temp.model_tex);
                if has_material_maps {
                    load_into(map, "model_tex_material", &mut temp.model_tex_material);
                    load_into(map, "model_tex_normal", &mut temp.model_tex_normal);
                } else {
                    temp.model_tex_material = ObjRef::default_resource();
                    temp.model_tex_normal = ObjRef::default_resource();
                }
                if let Some(v) = map.flag("model_use_blend_color") {
                    temp.model_use_blend_color = v;
                }
                if let Some(v) = map.color("model_blend_color") {
                    temp.model_blend_color = v;
                }

                if let Some(model) = map.object("model") {
                    if let Some(name) = model.string("name") {
                        temp.model_name = name.to_owned();
                    }
                    temp.model_state = load_state_vars(model, "state");
                    temp.model_version = model.real("model_version").unwrap_or(0.0);
                    if kind == TempType::Bodypart {
                        if let Some(part) = model.string("part_name") {
                            temp.model_part_name = part.to_owned();
                        }
                    }
                    if Pattern::has_keys(map, format) {
                        temp.pattern = Some(Pattern::load(map, format));
                    }
                    if temp.model_name == "armor" {
                        if let Some(armor) = map.object("armor") {
                            for (slot, piece) in ARMOR_SLOTS.iter().zip(&mut temp.armor) {
                                if let Some(v) = armor.color(&format!("{slot}_dye")) {
                                    piece.dye = v;
                                }
                                if let Some(v) = armor.string(&format!("{slot}_trim_pattern")) {
                                    piece.trim_pattern = v.to_owned();
                                }
                                if let Some(v) = armor.string(&format!("{slot}_trim_material")) {
                                    piece.trim_material = v.to_owned();
                                }
                            }
                        }
                    }
                }
            }
            TempType::Item => {
                if let Some(item) = map.object("item") {
                    load_into(item, "tex", &mut temp.item_tex);
                    load_into(item, "tex_material", &mut temp.item_tex_material);
                    load_into(item, "tex_normal", &mut temp.item_tex_normal);
                    match item.string("name") {
                        Some(name) if format < fmt::FORMAT_120_PRE_1 => {
                            temp.item_name = Some(name.replacen("items/", "item/", 1));
                        }
                        Some(name) => temp.item_name = Some(name.to_owned()),
                        None => {
                            if let Some(slot) = item.real("slot") {
                                temp.item_slot = slot;
                            }
                        }
                    }
                    temp.item.load_fields(item);
                }
            }
            TempType::Block => {
                if let Some(block) = map.object("block") {
                    if format < fmt::FORMAT_120_PRE_1 {
                        temp.legacy_block = Some((
                            block.real("legacy_id").unwrap_or(2.0),
                            block.real("legacy_data").unwrap_or(0.0),
                        ));
                    } else {
                        if let Some(name) = block.string("name") {
                            temp.block_name = name.to_owned();
                        }
                        temp.block_state = load_state_vars(block, "state");
                    }
                    load_into(block, "tex", &mut temp.block_tex);
                    if has_material_maps {
                        load_into(block, "tex_material", &mut temp.block_tex_material);
                        load_into(block, "tex_normal", &mut temp.block_tex_normal);
                    } else {
                        temp.block_tex_material = ObjRef::default_resource();
                        temp.block_tex_normal = ObjRef::default_resource();
                    }
                    if let Some(v) = block.flag("randomize") {
                        temp.block_randomize = v;
                    }
                    if let Some(v) = block.flag("repeat_enable") {
                        temp.block_repeat_enable = v;
                    }
                    if let Some(v) = block.point3("repeat") {
                        temp.block_repeat = v;
                    }
                }
            }
            TempType::Scenery => {
                load_into(map, "scenery", &mut temp.scenery);
                if let Some(block) = map.object("block") {
                    load_into(block, "tex", &mut temp.block_tex);
                    if has_material_maps {
                        load_into(block, "tex_material", &mut temp.block_tex_material);
                        load_into(block, "tex_normal", &mut temp.block_tex_normal);
                    }
                    // Missing before 2.0.0, and null in scenery imported by
                    // 2.0.0 pre-release 5.
                    if !has_material_maps || temp.block_tex_material.is_null() {
                        temp.block_tex_material = ObjRef::default_resource();
                    }
                    if !has_material_maps || temp.block_tex_normal.is_null() {
                        temp.block_tex_normal = ObjRef::default_resource();
                    }
                    if let Some(v) = block.flag("repeat_enable") {
                        temp.block_repeat_enable = v;
                    }
                    if let Some(v) = block.point3("repeat") {
                        temp.block_repeat = v;
                    }
                }
            }
            TempType::Model => {
                load_into(map, "model", &mut temp.model);
                load_into(map, "model_tex", &mut temp.model_tex);
                if has_material_maps {
                    load_into(map, "model_tex_material", &mut temp.model_tex_material);
                    load_into(map, "model_tex_normal", &mut temp.model_tex_normal);
                } else {
                    temp.model_tex_material = ObjRef::default_resource();
                    temp.model_tex_normal = ObjRef::default_resource();
                }
            }
            _ => {}
        }

        if kind.is_shape() {
            if let Some(shape) = map.object("shape") {
                temp.shape.load_fields(shape);
                // Texture mapping only exists for these three shapes.
                if !matches!(kind, TempType::Cube | TempType::Cylinder | TempType::Cone) {
                    temp.shape.tex_mapped = false;
                }
            }
        } else if kind == TempType::Text {
            if let Some(text) = map.object("text") {
                temp.text.load_fields(text);
            }
        } else if kind == TempType::ParticleSpawner {
            if let Some(particles) = map.object("particles") {
                let spawner = ParticleSpawner::load(particles, format, &mut || ctx.new_id());
                temp.particles = Some(Box::new(spawner));
            }
        }

        Some(temp)
    }
}
