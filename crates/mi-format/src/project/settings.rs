//! Project-wide settings: the `project`, `render` and `background` objects
//! of a `.miproject` file.
//!
//! Defaults are those of `project_reset`, `project_reset_render`,
//! `project_reset_background` and `camera_work_reset`.

use crate::json::{Atom, Json, JsonObject, JsonWriter};
use crate::record::record;
use mi_core::version::project as fmt;
use mi_core::{Color, ObjRef, SaveId, Value, ValueId};

/// Colours from `macros.gml`.
pub mod colors {
    use mi_core::Color;

    pub const SKY: Color = Color::rgb(120, 167, 255);
    pub const CLOUDS: Color = Color::rgb(255, 255, 255);
    pub const SUNLIGHT: Color = Color::rgb(255, 247, 228);
    pub const AMBIENT: Color = Color::rgb(102, 112, 140);
    pub const NIGHT: Color = Color::rgb(14, 14, 24);
    pub const PLAINS_BIOME_FOLIAGE: Color = Color::rgb(119, 171, 47);
    pub const PLAINS_BIOME_FOLIAGE_2: Color = Color::rgb(98, 168, 87);
    pub const PLAINS_BIOME_GRASS: Color = Color::rgb(145, 189, 89);
    pub const PLAINS_BIOME_WATER: Color = Color::rgb(62, 117, 225);
}

/// Which camera a viewport looks through.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewCamera {
    /// The free editor camera (`-4`).
    Work,
    /// Whichever camera timeline is active at the current frame (`-5`).
    Active,
    /// A specific camera timeline.
    Timeline(SaveId),
}

impl ViewCamera {
    fn save(&self, w: &mut JsonWriter, key: &str) {
        match self {
            ViewCamera::Work => w.var(key, -4.0),
            ViewCamera::Active => w.var(key, -5.0),
            ViewCamera::Timeline(id) => w.var(key, id.as_str()),
        }
    }

    fn load(map: &JsonObject, key: &str) -> Option<Self> {
        match map.get(key)? {
            Json::Number(n) if *n == -5.0 => Some(ViewCamera::Active),
            Json::Number(_) => Some(ViewCamera::Work),
            Json::String(s) if s == "null" || s.is_empty() => Some(ViewCamera::Work),
            Json::String(s) => Some(ViewCamera::Timeline(SaveId::new(s.as_str()))),
            _ => None,
        }
    }
}

record! {
    /// State of the timeline panel (`project.timeline`).
    pub struct TimelineSettings {
        "repeat" => repeat: bool = false,
        "seamless_repeat" => seamless_repeat: bool = false,
        "intervals_show" => intervals_show: bool = false,
        "interval_size" => interval_size: num = 24,
        "interval_offset" => interval_offset: num = 0,
        /// Current frame.
        "marker" => marker: num = 0,
        "list_width" => list_width: num = 320,
        "hor_scroll" => hor_scroll: num = 0,
        "zoom" => zoom: num = 16,
        "region_start" => region_start: nullable = None::<f64>,
        "region_end" => region_end: nullable = None::<f64>,
    }
}

record! {
    /// The editor's free camera (`project.work_camera`).
    pub struct WorkCamera {
        "focus" => focus: point3 = [0.0, 0.0, 16.0],
        "angle_xy" => angle_xy: num = 315,
        "angle_z" => angle_z: num = 5,
        "roll" => roll: num = 0,
        "zoom" => zoom: num = 100,
    }
}

/// The `project` object.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectInfo {
    pub name: String,
    pub author: String,
    pub description: String,
    pub video_width: f64,
    pub video_height: f64,
    pub video_keep_aspect_ratio: bool,
    /// Name of a render preset in `Data/Render`, or empty for custom settings.
    pub render_settings: String,
    /// Frames per second.
    pub tempo: f64,
    pub grid_rows: f64,
    pub grid_columns: f64,
    pub view_main_camera: ViewCamera,
    pub view_second_camera: ViewCamera,
    pub timeline: TimelineSettings,
    /// Colour tags (0..9) hidden in the timeline.
    pub hide_color_tag: Vec<bool>,
    pub work_camera: WorkCamera,
}

impl Default for ProjectInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            author: String::new(),
            description: String::new(),
            video_width: 1280.0,
            video_height: 720.0,
            video_keep_aspect_ratio: true,
            render_settings: "performance".to_owned(),
            tempo: 24.0,
            grid_rows: 3.0,
            grid_columns: 3.0,
            view_main_camera: ViewCamera::Work,
            view_second_camera: ViewCamera::Active,
            timeline: TimelineSettings::default(),
            hide_color_tag: vec![false; 9],
            work_camera: WorkCamera::default(),
        }
    }
}

impl ProjectInfo {
    /// `project_save_project`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(Some("project"));
        w.var("name", &self.name);
        w.var("author", &self.author);
        w.var("description", &self.description);
        w.var("video_width", self.video_width);
        w.var("video_height", self.video_height);
        w.var_bool("video_keep_aspect_ratio", self.video_keep_aspect_ratio);
        w.var("render_settings", &self.render_settings);
        w.var("tempo", self.tempo);
        w.var("grid_rows", self.grid_rows);
        w.var("grid_columns", self.grid_columns);
        self.view_main_camera.save(w, "view_main_camera");
        self.view_second_camera.save(w, "view_second_camera");

        w.object_start(Some("timeline"));
        self.timeline.save_fields(w);
        let tags = self.hide_color_tag.iter().map(|&b| Atom::Number(b as u8 as f64)).collect();
        w.var("hide_color_tag", Atom::List(tags));
        w.object_done();

        w.object_start(Some("work_camera"));
        self.work_camera.save_fields(w);
        w.object_done();

        w.object_done();
    }

    /// `project_load_project`
    pub(crate) fn load(&mut self, map: &JsonObject, format: i32) {
        let string = |key: &str, target: &mut String| {
            if let Some(v) = map.string(key) {
                *target = v.to_owned();
            }
        };
        string("name", &mut self.name);
        string("author", &mut self.author);
        string("description", &mut self.description);
        string("render_settings", &mut self.render_settings);

        let real = |key: &str, target: &mut f64| {
            if let Some(v) = map.real(key) {
                *target = v;
            }
        };
        real("video_width", &mut self.video_width);
        real("video_height", &mut self.video_height);
        real("tempo", &mut self.tempo);
        real("grid_rows", &mut self.grid_rows);
        real("grid_columns", &mut self.grid_columns);
        if let Some(v) = map.flag("video_keep_aspect_ratio") {
            self.video_keep_aspect_ratio = v;
        }

        // A missing or unusable entry falls back to the standard cameras.
        self.view_main_camera = ViewCamera::load(map, "view_main_camera").unwrap_or(ViewCamera::Work);
        self.view_second_camera = ViewCamera::load(map, "view_second_camera").unwrap_or(ViewCamera::Active);

        if let Some(tl) = map.object("timeline") {
            self.timeline.load_fields(tl);
            if format < fmt::FORMAT_200_PRE_5 {
                // Renamed from "show_seconds"; the new key did not exist yet.
                self.timeline.intervals_show = tl.flag("show_seconds").unwrap_or(false);
            }
            if let Some(tags) = tl.array("hide_color_tag") {
                self.hide_color_tag = tags.iter().map(|t| t.as_flag().unwrap_or(false)).collect();
            }
        }

        if let Some(cam) = map.object("work_camera") {
            self.work_camera.load_fields(cam);
        }
    }
}

record! {
    /// The `render` object, also the content of `.mirender` presets.
    pub struct RenderSettings {
        "render_samples" => samples: num = 24,
        "render_distance" => distance: num = 30000,

        "render_ssao" => ssao: bool = true,
        "render_ssao_radius" => ssao_radius: num = 12,
        "render_ssao_power" => ssao_power: num = 1,
        "render_ssao_color" => ssao_color: color = Color::BLACK,

        "render_shadows" => shadows: bool = true,
        "render_shadows_sun_buffer_size" => shadows_sun_buffer_size: num = 2048,
        "render_shadows_spot_buffer_size" => shadows_spot_buffer_size: num = 512,
        "render_shadows_point_buffer_size" => shadows_point_buffer_size: num = 256,
        "render_shadows_transparent" => shadows_transparent: flag = false,

        "render_subsurface_samples" => subsurface_samples: num = 7,
        "render_subsurface_highlight" => subsurface_highlight: num = 0.5,
        "render_subsurface_highlight_strength" => subsurface_highlight_strength: num = 1,

        "render_indirect" => indirect: bool = true,
        "render_indirect_precision" => indirect_precision: num = 0.3,
        "render_indirect_blur_radius" => indirect_blur_radius: num = 1,
        "render_indirect_strength" => indirect_strength: num = 1,

        "render_reflections" => reflections: bool = true,
        "render_reflections_precision" => reflections_precision: num = 0.3,
        "render_reflections_thickness" => reflections_thickness: num = 1,
        "render_reflections_fade_amount" => reflections_fade_amount: num = 1,

        "render_glow" => glow: bool = true,
        "render_glow_radius" => glow_radius: num = 1,
        "render_glow_intensity" => glow_intensity: num = 1,
        "render_glow_falloff" => glow_falloff: bool = false,
        "render_glow_falloff_radius" => glow_falloff_radius: num = 2,
        "render_glow_falloff_intensity" => glow_falloff_intensity: num = 1,

        "render_aa" => aa: bool = true,
        "render_aa_power" => aa_power: num = 1,

        /// `"blocky"`, `"realistic"` ...
        "bend_style" => bend_style: string = "blocky",
        "opaque_leaves" => opaque_leaves: bool = false,
        "liquid_animation" => liquid_animation: bool = true,
        "water_reflections" => water_reflections: bool = false,

        "block_emissive" => block_emissive: num = 1,
        "block_subsurface" => block_subsurface: num = 8,

        "glint_speed" => glint_speed: num = 1,
        "glint_strength" => glint_strength: num = 1,

        "texture_filtering" => texture_filtering: bool = true,
        "transparent_block_texture_filtering" => transparent_block_texture_filtering: bool = false,
        "texture_filtering_level" => texture_filtering_level: num = 1,

        /// Index into [`mi_core::AlphaMode`].
        "render_alpha_mode" => alpha_mode: num = 0,
        /// Index into [`mi_core::Tonemapper`].
        "tonemapper" => tonemapper: num = 0,
        "exposure" => exposure: num = 1,
        "gamma" => gamma: num = 2.2,
        "material_maps" => material_maps: bool = false,
    }
}

impl RenderSettings {
    /// `project_save_render`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(Some("render"));
        self.save_fields(w);
        w.object_done();
    }
}

record! {
    /// The `background` object: sky, ground, fog and wind.
    pub struct Background {
        "image_show" => image_show: bool = false,
        "image" => image: obj = ObjRef::Null,
        /// `"image"`, `"sphere"` or `"box"`.
        "image_type" => image_type: string = "image",
        "image_stretch" => image_stretch: bool = true,
        "image_box_mapped" => image_box_mapped: bool = false,
        "image_rotation" => image_rotation: num = 0,

        "sky_sun_tex" => sky_sun_tex: obj = ObjRef::default_resource(),
        "sky_sun_angle" => sky_sun_angle: num = 0,
        "sky_sun_scale" => sky_sun_scale: num = 1,
        "sky_moon_tex" => sky_moon_tex: obj = ObjRef::default_resource(),
        "sky_moon_phase" => sky_moon_phase: num = 0,
        "sky_moon_angle" => sky_moon_angle: num = 0,
        "sky_moon_scale" => sky_moon_scale: num = 1,

        "sky_time" => sky_time: num = -45,
        "sky_rotation" => sky_rotation: num = 0,
        "sunlight_strength" => sunlight_strength: num = 1,
        "sunlight_angle" => sunlight_angle: num = 0.526,

        "twilight" => twilight: bool = true,

        "sky_clouds_show" => sky_clouds_show: bool = true,
        /// `"normal"`, `"faded"` or `"flat"`.
        "sky_clouds_mode" => sky_clouds_mode: string = "normal",
        "sky_clouds_tex" => sky_clouds_tex: obj = ObjRef::default_resource(),
        "sky_clouds_speed" => sky_clouds_speed: num = 1,
        "sky_clouds_height" => sky_clouds_height: num = 1024,
        "sky_clouds_size" => sky_clouds_size: num = 1536,
        "sky_clouds_thickness" => sky_clouds_thickness: num = 64,
        "sky_clouds_offset" => sky_clouds_offset: num = 0,

        "ground_show" => ground_show: bool = true,
        /// Block texture name, for example `block/grass_block_top`.
        "ground_name" => ground_name: string = "block/grass_block_top",
        "ground_tex" => ground_tex: obj = ObjRef::default_resource(),
        "ground_tex_material" => ground_tex_material: obj = ObjRef::default_resource(),
        "ground_tex_normal" => ground_tex_normal: obj = ObjRef::default_resource(),

        "biome" => biome: string = "plains",

        "sky_color" => sky_color: color = colors::SKY,
        "sky_clouds_color" => sky_clouds_color: color = colors::CLOUDS,
        "sunlight_color" => sunlight_color: color = colors::SUNLIGHT,
        "ambient_color" => ambient_color: color = colors::AMBIENT,
        "night_color" => night_color: color = colors::NIGHT,

        "foliage_color" => foliage_color: color = colors::PLAINS_BIOME_FOLIAGE,
        "grass_color" => grass_color: color = colors::PLAINS_BIOME_GRASS,
        "water_color" => water_color: color = colors::PLAINS_BIOME_WATER,
        "leaves_oak_color" => leaves_oak_color: color = colors::PLAINS_BIOME_FOLIAGE,
        "leaves_spruce_color" => leaves_spruce_color: color = colors::PLAINS_BIOME_FOLIAGE_2,
        "leaves_birch_color" => leaves_birch_color: color = colors::PLAINS_BIOME_FOLIAGE_2,
        "leaves_jungle_color" => leaves_jungle_color: color = colors::PLAINS_BIOME_FOLIAGE,
        "leaves_acacia_color" => leaves_acacia_color: color = colors::PLAINS_BIOME_FOLIAGE,
        "leaves_dark_oak_color" => leaves_dark_oak_color: color = colors::PLAINS_BIOME_FOLIAGE,
        "leaves_mangrove_color" => leaves_mangrove_color: color = colors::PLAINS_BIOME_FOLIAGE,

        "fog_show" => fog_show: bool = true,
        "fog_sky" => fog_sky: bool = true,
        "fog_color_custom" => fog_color_custom: bool = false,
        "fog_color" => fog_color: color = colors::SKY,
        "fog_object_color_custom" => fog_object_color_custom: bool = false,
        "fog_object_color" => fog_object_color: color = colors::SKY,
        "fog_distance" => fog_distance: num = 10000,
        "fog_size" => fog_size: num = 2000,
        "fog_height" => fog_height: num = 1250,

        "wind" => wind: bool = true,
        "wind_speed" => wind_speed: num = 0.1,
        "wind_strength" => wind_strength: num = 0.5,
        "wind_direction" => wind_direction: num = 45,
        "wind_directional_speed" => wind_directional_speed: num = 0.2,
        "wind_directional_strength" => wind_directional_strength: num = 1.5,

        "texture_animation_speed" => texture_animation_speed: num = 0.25,
    }
}

impl Background {
    /// `project_save_background`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(Some("background"));
        self.save_fields(w);
        w.object_done();
    }

    /// `project_load_background`, including the upgrades of older formats
    /// that do not depend on Minecraft assets.
    pub(crate) fn load(&mut self, map: &JsonObject, format: i32) {
        self.load_fields(map);

        if format < fmt::FORMAT_200_PRE_5 {
            // Sunlight strength used to be an addition on top of 1.
            self.sunlight_strength += 1.0;

            // Cloud style was two flags, and "height" meant thickness.
            self.sky_clouds_mode = Background::default().sky_clouds_mode;
            if map.flag("sky_clouds_story_mode").unwrap_or(false) {
                self.sky_clouds_mode = "faded".to_owned();
            } else if map.flag("sky_clouds_flat").unwrap_or(false) {
                self.sky_clouds_mode = "flat".to_owned();
            }
            let defaults = Background::default();
            self.sky_clouds_height = map.real("sky_clouds_z").unwrap_or(defaults.sky_clouds_height);
            self.sky_clouds_thickness = map.real("sky_clouds_height").unwrap_or(defaults.sky_clouds_thickness);

            // The built-in cloud texture became eight times larger.
            if self.sky_clouds_tex == ObjRef::default_resource() {
                self.sky_clouds_size *= 8.0;
            }
        }

        if format < fmt::FORMAT_120_PRE_1 {
            self.ground_name = self.ground_name.replacen("blocks/", "block/", 1);
        }

        // Empty biome name bug in some versions: fall back to plains.
        if self.biome.is_empty() {
            self.biome = Background::default().biome;
        }

        // Separate leaf colours were introduced in 2.0.0; older projects
        // derive them from the foliage colour.
        if format < fmt::FORMAT_200_PRE_5 {
            self.leaves_oak_color = self.foliage_color;
            self.leaves_spruce_color = colors::PLAINS_BIOME_FOLIAGE_2;
            self.leaves_birch_color = colors::PLAINS_BIOME_FOLIAGE_2;
            self.leaves_jungle_color = self.foliage_color;
            self.leaves_acacia_color = self.foliage_color;
            self.leaves_dark_oak_color = self.foliage_color;
            self.leaves_mangrove_color = self.foliage_color;
        }
    }

    /// The background setting behind a `BG_*` value, in the representation
    /// the value uses. `None` for values that are not background settings and
    /// for `BG_GROUND_SLOT`, which is an index into the Minecraft assets.
    pub fn value(&self, id: ValueId) -> Option<Value> {
        use ValueId::*;
        let num = Value::Number;
        let flag_as_number = |b: bool| Value::Number(b as u8 as f64);
        Some(match id {
            BgImageShow => Value::Bool(self.image_show),
            BgImageRotation => num(self.image_rotation),
            BgSkyMoonPhase => num(self.sky_moon_phase),
            BgSkyTime => num(self.sky_time),
            BgSkyRotation => num(self.sky_rotation),
            BgSunlightStrength => num(self.sunlight_strength),
            BgSunlightAngle => num(self.sunlight_angle),
            BgSkySunAngle => num(self.sky_sun_angle),
            BgSkySunScale => num(self.sky_sun_scale),
            BgSkyMoonAngle => num(self.sky_moon_angle),
            BgSkyMoonScale => num(self.sky_moon_scale),
            BgTwilight => flag_as_number(self.twilight),
            BgSkyCloudsShow => Value::Bool(self.sky_clouds_show),
            BgSkyCloudsSpeed => num(self.sky_clouds_speed),
            BgSkyCloudsHeight => num(self.sky_clouds_height),
            BgSkyCloudsOffset => num(self.sky_clouds_offset),
            BgGroundShow => Value::Bool(self.ground_show),
            BgBiome => Value::Str(self.biome.clone()),
            BgSkyColor => Value::Color(self.sky_color),
            BgSkyCloudsColor => Value::Color(self.sky_clouds_color),
            BgSunlightColor => Value::Color(self.sunlight_color),
            BgAmbientColor => Value::Color(self.ambient_color),
            BgNightColor => Value::Color(self.night_color),
            BgGrassColor => Value::Color(self.grass_color),
            BgFoliageColor => Value::Color(self.foliage_color),
            BgWaterColor => Value::Color(self.water_color),
            BgLeavesOakColor => Value::Color(self.leaves_oak_color),
            BgLeavesSpruceColor => Value::Color(self.leaves_spruce_color),
            BgLeavesBirchColor => Value::Color(self.leaves_birch_color),
            BgLeavesJungleColor => Value::Color(self.leaves_jungle_color),
            BgLeavesAcaciaColor => Value::Color(self.leaves_acacia_color),
            BgLeavesDarkOakColor => Value::Color(self.leaves_dark_oak_color),
            BgLeavesMangroveColor => Value::Color(self.leaves_mangrove_color),
            BgFogShow => Value::Bool(self.fog_show),
            BgFogSky => flag_as_number(self.fog_sky),
            BgFogCustomColor => flag_as_number(self.fog_color_custom),
            BgFogColor => Value::Color(self.fog_color),
            BgFogCustomObjectColor => flag_as_number(self.fog_object_color_custom),
            BgFogObjectColor => Value::Color(self.fog_object_color),
            BgFogDistance => num(self.fog_distance),
            BgFogSize => num(self.fog_size),
            BgFogHeight => num(self.fog_height),
            BgWind => Value::Bool(self.wind),
            BgWindSpeed => num(self.wind_speed),
            BgWindStrength => num(self.wind_strength),
            BgWindDirection => num(self.wind_direction),
            BgWindDirectionalSpeed => num(self.wind_directional_speed),
            BgWindDirectionalStrength => num(self.wind_directional_strength),
            BgTextureAniSpeed => num(self.texture_animation_speed),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    #[test]
    fn every_background_value_except_ground_slot_is_mapped() {
        let bg = Background::default();
        for &id in ValueId::ALL {
            let mapped = bg.value(id).is_some();
            assert_eq!(mapped, id.is_background() && id != ValueId::BgGroundSlot, "{id:?}");
        }
    }

    #[test]
    fn project_info_round_trip() {
        let mut info = ProjectInfo::default();
        info.name = "Tëst".into();
        info.tempo = 30.0;
        info.view_main_camera = ViewCamera::Timeline(SaveId::new("CAM0000000000000"));
        info.timeline.region_start = Some(10.0);
        info.timeline.region_end = Some(48.0);
        info.hide_color_tag[3] = true;
        info.work_camera.focus = [1.0, 2.0, 3.0];

        let mut w = JsonWriter::new();
        w.object_start(None);
        info.save(&mut w);
        w.object_done();
        let text = w.finish();
        assert!(text.contains("\"focus\": [ 1, 3, 2 ]"), "{text}");
        assert!(text.contains("\"hide_color_tag\": [ 0, 0, 0, 1, 0, 0, 0, 0, 0 ]"), "{text}");
        assert!(text.contains("\"view_second_camera\": -5"), "{text}");

        let doc = parse(text.as_bytes()).unwrap();
        let mut loaded = ProjectInfo::default();
        loaded.load(doc.as_object().unwrap().object("project").unwrap(), fmt::CURRENT);
        assert_eq!(loaded, info);
    }

    #[test]
    fn old_background_is_upgraded() {
        let doc = parse(
            br#"{"sunlight_strength": 0.5, "sky_clouds_flat": true, "sky_clouds_z": 300,
                 "sky_clouds_height": 12, "sky_clouds_size": 100, "sky_clouds_tex": "default",
                 "ground_name": "blocks/stone", "biome": ""}"#,
        )
        .unwrap();
        let mut bg = Background::default();
        bg.load(doc.as_object().unwrap(), fmt::FORMAT_113);
        assert_eq!(bg.sunlight_strength, 1.5);
        assert_eq!(bg.sky_clouds_mode, "flat");
        assert_eq!(bg.sky_clouds_height, 300.0);
        assert_eq!(bg.sky_clouds_thickness, 12.0);
        assert_eq!(bg.sky_clouds_size, 800.0);
        assert_eq!(bg.ground_name, "block/stone");
        assert_eq!(bg.biome, "plains");
    }
}
