//! Timelines (`obj_timeline`): the objects of a scene and their keyframes.

use super::template::Pattern;
use super::{Background, LoadContext};
use crate::json::{JsonObject, JsonWriter};
use crate::record::{load_obj, load_state_vars, record, save_obj, save_state_vars, StateValue};
use crate::values::ValueSet;
use mi_core::types::ValueType;
use mi_core::version::project as fmt;
use mi_core::{ObjRef, SaveId, TlType, Value, ValueId};

/// A keyframe: the complete set of values at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// Frame number.
    pub position: i64,
    pub values: ValueSet,
}

record! {
    /// What a timeline takes over from its parent (`inherit` object).
    pub struct Inherit {
        "position" => position: bool = true,
        "rotation" => rotation: bool = true,
        "scale" => scale: bool = true,
        "alpha" => alpha: bool = false,
        "color" => color: bool = false,
        "texture" => texture: bool = false,
        "surface" => surface: bool = true,
        "subsurface" => subsurface: bool = true,
        "visibility" => visibility: bool = true,
        "bend" => bend: bool = false,
        "rot_point" => rot_point: bool = false,
        "glow_color" => glow_color: bool = true,
        "select" => select: bool = false,
        "pose" => pose: bool = false,
    }
}

record! {
    /// Rendering options of a timeline (first part, up to the fog flag).
    pub struct Appearance {
        "backfaces" => backfaces: bool = false,
        "texture_blur" => texture_blur: bool = false,
        "texture_filtering" => texture_filtering: bool = false,
        "shadows" => shadows: bool = true,
        "ssao" => ssao: bool = true,
        "glow" => glow: bool = false,
        "glow_texture" => glow_texture: bool = true,
        "only_render_glow" => only_render_glow: bool = false,
        /// Index into [`mi_core::GlintMode`].
        "glint_mode" => glint_mode: num = 0,
        "glint_scale" => glint_scale: num = 1,
        "glint_speed" => glint_speed: num = 1,
        "glint_strength" => glint_strength: num = 1,
        "glint_tex" => glint_tex: obj = ObjRef::default_resource(),
        "fog" => fog: bool = true,
    }
}

record! {
    /// Settings of path timelines (`path` object).
    pub struct PathSettings {
        "smooth" => smooth: bool = true,
        "closed" => closed: bool = false,
        "detail" => detail: num = 6,
        "shape_generate" => shape_generate: bool = false,
        "shape_radius" => shape_radius: num = 8,
        "shape_tex_length" => shape_tex_length: num = 16,
        "shape_invert" => shape_invert: bool = false,
        "shape_tube" => shape_tube: bool = false,
        "shape_detail" => shape_detail: num = 6,
        "shape_smooth_segments" => shape_smooth_segments: bool = true,
        "shape_smooth_ring" => shape_smooth_ring: bool = false,
    }
}

/// A scene object.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pub id: SaveId,
    pub kind: TlType,
    pub name: String,
    /// Template this timeline is an instance of.
    pub temp: ObjRef,
    /// Colour tag index, if tagged.
    pub color_tag: Option<f64>,
    pub hide: bool,
    pub lock: bool,
    pub ghost: bool,
    /// Render order; lower draws first.
    pub depth: f64,
    /// Name of the model part, for body parts.
    pub model_part_name: String,
    /// Displayed text, for text timelines.
    pub text: String,

    /// The character, scenery or model timeline this one is a part of.
    pub part_of: ObjRef,
    /// The outermost timeline of a scenery this special block belongs to.
    pub part_root: ObjRef,
    /// Model of a special block that is part of scenery.
    pub part_model: Option<(String, Vec<(String, StateValue)>)>,
    /// Block of a block timeline that is part of scenery.
    pub part_block: Option<(String, Vec<(String, StateValue)>)>,
    /// Numeric block id and data in projects older than 1.2.0.
    pub legacy_block: Option<(f64, f64)>,
    /// `"banner"` etc. for special blocks in scenery that carry a pattern.
    pub pattern_type: String,
    pub pattern: Option<Pattern>,
    /// Timelines that are parts of this one.
    pub parts: Option<Vec<ObjRef>>,

    pub default_values: ValueSet,
    /// Sorted by position, one per position.
    pub keyframes: Vec<Keyframe>,

    /// Parent in the timeline tree; [`SaveId::root`] for top level.
    pub parent: SaveId,
    /// Position among the parent's children.
    pub parent_tree_index: Option<i64>,
    pub lock_bend: bool,
    /// Whether the children are shown in the timeline list.
    pub tree_extend: bool,
    pub inherit: Inherit,
    pub scale_resize: bool,
    pub rot_point_custom: bool,
    pub rot_point: [f64; 3],

    pub appearance: Appearance,
    pub wind: bool,
    pub wind_terrain: bool,
    /// Hidden in high quality renders.
    pub hq_hiding: bool,
    /// Hidden in the low quality viewport.
    pub lq_hiding: bool,
    /// `"normal"`, `"add"`, `"subtract"`, `"multiply"` or `"screen"`.
    pub blend_mode: String,
    /// Index into [`mi_core::AlphaMode`].
    pub alpha_mode: f64,

    pub path: PathSettings,
}

impl Timeline {
    /// A timeline with the settings of `tl_event_create`, starting from the
    /// project's default values.
    pub fn new(id: SaveId, kind: TlType, defaults: &ValueSet) -> Self {
        Self {
            id,
            kind,
            name: String::new(),
            temp: ObjRef::Null,
            color_tag: None,
            hide: false,
            lock: false,
            ghost: false,
            depth: 0.0,
            model_part_name: String::new(),
            text: String::new(),
            part_of: ObjRef::Null,
            part_root: ObjRef::Null,
            part_model: None,
            part_block: None,
            legacy_block: None,
            pattern_type: String::new(),
            pattern: None,
            parts: None,
            default_values: defaults.clone(),
            keyframes: Vec::new(),
            parent: SaveId::root(),
            parent_tree_index: None,
            lock_bend: true,
            tree_extend: false,
            inherit: Inherit::default(),
            scale_resize: true,
            rot_point_custom: false,
            rot_point: [0.0; 3],
            appearance: Appearance::default(),
            wind: false,
            wind_terrain: true,
            hq_hiding: false,
            lq_hiding: false,
            blend_mode: "normal".to_owned(),
            alpha_mode: mi_core::AlphaMode::Default.index() as f64,
            path: PathSettings::default(),
        }
    }

    /// `project_save_timeline`. `project_defaults` are the project's default
    /// values, against which the timeline's own defaults are diffed.
    pub(crate) fn save(&self, w: &mut JsonWriter, project_defaults: &ValueSet) {
        w.object_start(None);
        w.var("id", self.id.as_str());
        w.var("type", self.kind.name());
        w.var("name", &self.name);
        save_obj(w, "temp", &self.temp);
        w.var_nullable("color_tag", self.color_tag);
        w.var_bool("hide", self.hide);
        w.var_bool("lock", self.lock);
        w.var_bool("ghost", self.ghost);
        w.var("depth", self.depth);

        if self.kind == TlType::Bodypart {
            w.var("model_part_name", &self.model_part_name);
        }
        if self.kind == TlType::Text {
            w.var("text", &self.text);
        }

        if !self.part_of.is_null() {
            if self.kind == TlType::SpecialBlock {
                let (name, state) = self.part_model.clone().unwrap_or_default();
                w.object_start(Some("model"));
                w.var("name", &name);
                save_state_vars(w, "state", &state);
                w.object_done();
            } else if self.kind == TlType::Block {
                let (name, state) = self.part_block.clone().unwrap_or_default();
                w.object_start(Some("block"));
                w.var("name", &name);
                save_state_vars(w, "state", &state);
                w.object_done();
            }

            save_obj(w, "part_of", &self.part_of);
            if !self.part_root.is_null() {
                save_obj(w, "part_root", &self.part_root);
            }

            if let (false, Some(pattern)) = (self.pattern_type.is_empty(), &self.pattern) {
                w.var("pattern_type", &self.pattern_type);
                w.var("pattern_base_color", &pattern.base_color);
                if !pattern.patterns.is_empty() {
                    Pattern::save_list(w, "pattern_pattern_list", &pattern.patterns);
                }
                if !pattern.colors.is_empty() {
                    Pattern::save_list(w, "pattern_color_list", &pattern.colors);
                }
            }
        }

        if let Some(parts) = &self.parts {
            w.array_start(Some("parts"));
            for part in parts {
                // `save_id_get` yields an empty string for a missing object.
                w.array_value(part.as_id().map_or("", SaveId::as_str));
            }
            w.array_done();
        }

        self.default_values.save_diff(w, "default_values", project_defaults);

        w.object_start(Some("keyframes"));
        for keyframe in &self.keyframes {
            keyframe.values.save_diff(w, &keyframe.position.to_string(), &self.default_values);
        }
        w.object_done();

        w.var("parent", self.parent.as_str());
        w.var("parent_tree_index", self.parent_tree_index.unwrap_or(-1) as f64);

        let types = self.kind.value_types(false);

        if types.has(ValueType::Hierarchy) {
            w.var_bool("lock_bend", self.lock_bend);
            w.var_bool("tree_extend", self.tree_extend);
            w.object_start(Some("inherit"));
            self.inherit.save_fields(w);
            w.object_done();
            w.var_bool("scale_resize", self.scale_resize);
        }

        if types.has(ValueType::RotPoint) {
            w.var_bool("rot_point_custom", self.rot_point_custom);
            w.var_point3("rot_point", self.rot_point);
        }

        if types.has(ValueType::Appearance) {
            self.appearance.save_fields(w);
            if self.kind.has_wind_settings() {
                w.var_bool("wind", self.wind);
                w.var_bool("wind_terrain", self.wind_terrain);
            }
            w.var_bool("hq_hiding", self.hq_hiding);
            w.var_bool("lq_hiding", self.lq_hiding);
            w.var("blend_mode", &self.blend_mode);
            w.var("alpha_mode", self.alpha_mode);
        }

        if types.has(ValueType::Path) {
            w.object_start(Some("path"));
            self.path.save_fields(w);
            w.object_done();
        }

        w.object_done();
    }

    /// `project_load_timeline`
    pub(crate) fn load(
        map: &JsonObject,
        ctx: &mut LoadContext,
        project_defaults: &ValueSet,
        background: &Background,
    ) -> Option<Self> {
        let format = ctx.format;
        let type_name = map.string("type").unwrap_or("");
        let Some(kind) = TlType::from_name(type_name) else {
            ctx.warn(format!("Skipped a timeline of unknown type \"{type_name}\""));
            return None;
        };
        let id = match map.string("id") {
            Some(id) => SaveId::new(id),
            None => ctx.new_id(),
        };
        let mut tl = Timeline::new(id, kind, project_defaults);

        if let Some(v) = map.string("name") {
            tl.name = v.to_owned();
        }
        if let Some(v) = load_obj(map, "temp") {
            tl.temp = v;
        }
        if let Some(v) = map.nullable_real("color_tag") {
            tl.color_tag = v;
        }
        let flag = |key: &str, target: &mut bool| {
            if let Some(v) = map.flag(key) {
                *target = v;
            }
        };
        let real = |key: &str, target: &mut f64| {
            if let Some(v) = map.real(key) {
                *target = v;
            }
        };
        flag("hide", &mut tl.hide);
        flag("lock", &mut tl.lock);
        flag("ghost", &mut tl.ghost);
        real("depth", &mut tl.depth);

        if kind == TlType::Bodypart {
            if let Some(v) = map.string("model_part_name") {
                tl.model_part_name = v.to_owned();
            }
        }
        if kind == TlType::Text {
            if let Some(v) = map.string("text") {
                tl.text = v.to_owned();
            }
        }

        if let Some(v) = load_obj(map, "part_of") {
            tl.part_of = v;
        }
        if !tl.part_of.is_null() {
            if let Some(v) = load_obj(map, "part_root") {
                tl.part_root = v;
            }
            if kind == TlType::SpecialBlock {
                if let Some(model) = map.object("model") {
                    tl.part_model = Some((
                        model.string("name").unwrap_or("").to_owned(),
                        load_state_vars(model, "state"),
                    ));
                }
                if let Some(v) = map.string("pattern_type") {
                    tl.pattern_type = v.to_owned();
                }
                if format < fmt::FORMAT_200_PRE_5 && map.flag("is_banner").unwrap_or(false) {
                    tl.pattern_type = "banner".to_owned();
                }
                if !tl.pattern_type.is_empty() {
                    tl.pattern = Some(Pattern::load(map, format));
                }
            } else if kind == TlType::Block {
                if let Some(block) = map.object("block") {
                    if format < fmt::FORMAT_120_PRE_1 {
                        tl.legacy_block = Some((
                            block.real("legacy_id").unwrap_or(2.0),
                            block.real("legacy_data").unwrap_or(0.0),
                        ));
                    } else {
                        tl.part_block = Some((
                            block.string("name").unwrap_or("").to_owned(),
                            load_state_vars(block, "state"),
                        ));
                    }
                }
            }
        }

        if let Some(parts) = map.array("parts") {
            tl.parts = Some(
                parts
                    .iter()
                    .map(|p| match p.as_str() {
                        Some("") | Some("null") | None => ObjRef::Null,
                        Some(id) => ObjRef::id(id),
                    })
                    .collect(),
            );
        }

        if let Some(defaults) = map.object("default_values") {
            tl.default_values.load_from(defaults, format);
        }

        if let Some(keyframes) = map.object("keyframes") {
            for (key, entry) in keyframes.iter() {
                let (Some(position), Some(values_map)) = (parse_position(key), entry.as_object()) else {
                    ctx.warn(format!("Skipped keyframe \"{key}\" of timeline {}", tl.id));
                    continue;
                };
                let mut values = tl.default_values.clone();
                values.load_from(values_map, format);
                upgrade_keyframe(&mut values, kind, format, background);
                tl.keyframes.push(Keyframe { position, values });
            }
            // One keyframe per position, the last one in the file winning,
            // ordered by position.
            tl.keyframes.reverse();
            tl.keyframes.sort_by_key(|k| k.position);
            tl.keyframes.dedup_by_key(|k| k.position);
        }

        if let Some(v) = map.string("parent") {
            // "null" and missing both mean top level.
            tl.parent = if v == "null" { SaveId::root() } else { SaveId::new(v) };
        }
        tl.parent_tree_index = map.real("parent_tree_index").filter(|i| *i >= 0.0).map(|i| i as i64);

        flag("lock_bend", &mut tl.lock_bend);
        flag("tree_extend", &mut tl.tree_extend);
        if let Some(inherit) = map.object("inherit") {
            tl.inherit.load_fields(inherit);
        }
        flag("scale_resize", &mut tl.scale_resize);
        flag("rot_point_custom", &mut tl.rot_point_custom);
        if let Some(v) = map.point3("rot_point") {
            tl.rot_point = v;
        }

        tl.appearance.load_fields(map);
        // A missing or unusable glint texture means the built-in one.
        tl.appearance.glint_tex = match load_obj(map, "glint_tex") {
            Some(ObjRef::Id(id)) => ObjRef::Id(id),
            _ => ObjRef::default_resource(),
        };
        if kind.has_wind_settings() {
            flag("wind", &mut tl.wind);
            flag("wind_terrain", &mut tl.wind_terrain);
        }
        flag("hq_hiding", &mut tl.hq_hiding);
        flag("lq_hiding", &mut tl.lq_hiding);
        if let Some(v) = map.string("blend_mode") {
            tl.blend_mode = v.to_owned();
        }
        if format < fmt::FORMAT_200_PRE_5 {
            tl.alpha_mode = mi_core::AlphaMode::Blend.index() as f64;
        } else {
            real("alpha_mode", &mut tl.alpha_mode);
        }

        if let Some(path) = map.object("path") {
            tl.path.load_fields(path);
        }

        Some(tl)
    }
}

/// Keyframe keys are frame numbers written with `string()`.
fn parse_position(key: &str) -> Option<i64> {
    let value: f64 = key.trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value as i64)
}

/// `project_load_values_update`: brings keyframe values of older formats up
/// to date. The ground slot fix of projects older than 1.2.5 needs the
/// Minecraft assets and is applied when the project is bound to them.
fn upgrade_keyframe(values: &mut ValueSet, kind: TlType, format: i32, background: &Background) {
    use ValueId::*;
    let from_background = |values: &mut ValueSet, id: ValueId| {
        if let Some(v) = background.value(id) {
            values[id] = v;
        }
    };

    // More background values became keyframable in 1.2.0.
    if format < fmt::FORMAT_120_PRE_3 && kind == TlType::Background {
        for id in [
            BgImageShow,
            BgSunlightStrength,
            BgSkyCloudsShow,
            BgSkyCloudsHeight,
            BgGroundShow,
            BgGrassColor,
            BgFoliageColor,
            BgWaterColor,
            BgFogShow,
            BgFogSky,
            BgFogCustomColor,
            BgFogCustomObjectColor,
            BgWind,
        ] {
            from_background(values, id);
        }
    }

    // Anamorphic ratios can no longer be negative (1.2.5).
    if format < fmt::FORMAT_125 && kind == TlType::Camera {
        for id in [CamBloomRatio, CamDofBlurRatio] {
            values[id] = Value::Number(values.number(id).max(0.0));
        }
    }

    if format < fmt::FORMAT_200_PRE_5 {
        // Separate leaf colours (2.0.0).
        let foliage = values[BgFoliageColor].clone();
        for id in [BgLeavesOakColor, BgLeavesJungleColor, BgLeavesAcaciaColor, BgLeavesDarkOakColor, BgLeavesMangroveColor] {
            values[id] = foliage.clone();
        }
        for id in [BgLeavesSpruceColor, BgLeavesBirchColor] {
            values[id] = Value::Color(super::settings::colors::PLAINS_BIOME_FOLIAGE_2);
        }

        if kind == TlType::Background {
            from_background(values, BgBiome);
            values[BgSunlightStrength] = Value::Number(values.number(BgSunlightStrength) + 1.0);
        }

        // Camera shake became per-axis and ten times slower per unit.
        if kind == TlType::Camera && values.flag(CamShake) {
            values[CamShakeMode] = Value::Number(1.0);
            values[CamShakeSpeedX] = Value::Number(values.number(CamShakeSpeedX) * 10.0);
            values[CamShakeSpeedY] = Value::Number(values.number(CamShakeSpeedY) * 10.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        assert_eq!(parse_position("0"), Some(0));
        assert_eq!(parse_position("120"), Some(120));
        assert_eq!(parse_position("12.0"), Some(12));
        assert_eq!(parse_position("-1"), None);
        assert_eq!(parse_position("abc"), None);
    }

    #[test]
    fn camera_shake_upgrade() {
        let bg = Background::default();
        let mut v = ValueSet::project_defaults(&bg, 0.0, 1.0);
        v[ValueId::CamShake] = Value::Bool(true);
        v[ValueId::CamShakeSpeedX] = Value::Number(2.0);
        v[ValueId::CamBloomRatio] = Value::Number(-0.5);
        upgrade_keyframe(&mut v, TlType::Camera, fmt::FORMAT_122, &bg);
        assert_eq!(v[ValueId::CamShakeMode], Value::Number(1.0));
        assert_eq!(v[ValueId::CamShakeSpeedX], Value::Number(20.0));
        assert_eq!(v[ValueId::CamBloomRatio], Value::Number(0.0));

        let mut current = ValueSet::project_defaults(&bg, 0.0, 1.0);
        current[ValueId::CamShake] = Value::Bool(true);
        let before = current.clone();
        upgrade_keyframe(&mut current, TlType::Camera, fmt::CURRENT, &bg);
        assert_eq!(current, before);
    }
}
