//! Particle spawner settings (`particles` object of a template, also the
//! body of `.miparticles` files).
//!
//! Defaults come from `temp_particles_init` and `ptype_event_create`.

use crate::json::{JsonObject, JsonWriter};
use crate::record::{load_obj, record, save_obj};
use mi_core::version::project as fmt;
use mi_core::{Color, ObjRef, SaveId};

record! {
    /// Spawner-wide settings (the `pc_*` variables of a template).
    pub struct SpawnerSettings {
        "spawn_constant" => spawn_constant: bool = true,
        "spawn_amount" => spawn_amount: num = 100,

        "spawn_region_use" => spawn_region_use: bool = false,
        /// `"sphere"`, `"cube"`, `"box"` or `"path"`.
        "spawn_region_type" => spawn_region_type: string = "sphere",
        /// Path timeline particles spawn along.
        "spawn_region_path" => spawn_region_path: opt_obj = ObjRef::Null,
        "spawn_region_sphere_radius" => spawn_region_sphere_radius: num = 100,
        "spawn_region_cube_size" => spawn_region_cube_size: num = 200,
        "spawn_region_box_size" => spawn_region_box_size: point3 = [200.0, 200.0, 200.0],
        "spawn_region_path_radius" => spawn_region_path_radius: num = 8,

        /// `"none"`, `"ground"` or `"custom"`.
        "bounding_box_type" => bounding_box_type: string = "none",
        "bounding_box_ground_z" => bounding_box_ground_z: num = 0,
        "bounding_box_custom_start" => bounding_box_custom_start: point3 = [-100.0, -100.0, -100.0],
        "bounding_box_custom_end" => bounding_box_custom_end: point3 = [100.0, 100.0, 100.0],
        "bounding_box_relative" => bounding_box_relative: bool = false,

        "destroy_at_animation_finish" => destroy_at_animation_finish: bool = true,
        "destroy_at_amount" => destroy_at_amount: bool = true,
        "destroy_at_amount_val" => destroy_at_amount_val: num = 200,
        "destroy_at_time" => destroy_at_time: bool = false,
        "destroy_at_time_seconds" => destroy_at_time_seconds: num = 5,
        "destroy_at_time_israndom" => destroy_at_time_israndom: bool = false,
        "destroy_at_time_random_min" => destroy_at_time_random_min: num = 5,
        "destroy_at_time_random_max" => destroy_at_time_random_max: num = 10,
        "destroy_at_bounding_box" => destroy_at_bounding_box: flag = false,
    }
}

/// What a particle type draws.
#[derive(Debug, Clone, PartialEq)]
pub enum ParticleSource {
    /// A sprite sheet (`sprite_tex`).
    Sheet,
    /// One of the built-in Minecraft particle templates (`sprite_template`).
    Template,
    /// Instances of a library template.
    Object(ObjRef),
}

record! {
    /// The motion and look of one particle type, without its identity.
    pub struct ParticleTypeSettings {
        "text" => text: string = "",
        "spawn_rate" => spawn_rate: num = 0,

        "sprite_tex" => sprite_tex: obj = ObjRef::Null,
        "sprite_tex_image" => sprite_tex_image: num = 0,
        "sprite_template_tex" => sprite_template_tex: obj = ObjRef::default_resource(),
        "sprite_template" => sprite_template: string = "generic",
        "sprite_template_still_frame" => sprite_template_still_frame: bool = false,
        "sprite_template_random_frame" => sprite_template_random_frame: bool = false,
        "sprite_template_reverse" => sprite_template_reverse: bool = true,
        "sprite_frame_width" => sprite_frame_width: num = 8,
        "sprite_frame_height" => sprite_frame_height: num = 8,
        "sprite_frame_start" => sprite_frame_start: num = 7,
        "sprite_frame_end" => sprite_frame_end: num = 0,
        "sprite_animation_speed" => sprite_animation_speed: num = 5,
        "sprite_animation_speed_israndom" => sprite_animation_speed_israndom: bool = false,
        "sprite_animation_speed_random_min" => sprite_animation_speed_random_min: num = 5,
        "sprite_animation_speed_random_max" => sprite_animation_speed_random_max: num = 10,
        /// 0 = stop, 1 = loop, 2 = reverse.
        "sprite_animation_onend" => sprite_animation_onend: num = 0,

        "angle_extend" => angle_extend: bool = false,
        "angle" => angle: point3 = [0.0; 3],
        "angle_israndom" => angle_israndom: flags3 = [true; 3],
        "angle_random_min" => angle_random_min: point3 = [0.0; 3],
        "angle_random_max" => angle_random_max: point3 = [360.0; 3],
        "angle_speed" => angle_speed: num = 20,
        "angle_speed_israndom" => angle_speed_israndom: flag = true,
        "angle_speed_random_min" => angle_speed_random_min: num = 0,
        "angle_speed_random_max" => angle_speed_random_max: num = 20,
        "angle_speed_add" => angle_speed_add: num = 0,
        "angle_speed_add_israndom" => angle_speed_add_israndom: flag = false,
        "angle_speed_add_random_min" => angle_speed_add_random_min: num = -1,
        "angle_speed_add_random_max" => angle_speed_add_random_max: num = 1,
        "angle_speed_mul" => angle_speed_mul: num = 1,
        "angle_speed_mul_israndom" => angle_speed_mul_israndom: flag = false,
        "angle_speed_mul_random_min" => angle_speed_mul_random_min: num = 0.75,
        "angle_speed_mul_random_max" => angle_speed_mul_random_max: num = 0.9,

        "spd_extend" => spd_extend: bool = false,
        "spd" => spd: point3 = [0.0; 3],
        "spd_israndom" => spd_israndom: flags3 = [false; 3],
        "spd_random_min" => spd_random_min: point3 = [-20.0; 3],
        "spd_random_max" => spd_random_max: point3 = [20.0; 3],
        "spd_add" => spd_add: point3 = [0.0; 3],
        "spd_add_israndom" => spd_add_israndom: flags3 = [false; 3],
        "spd_add_random_min" => spd_add_random_min: point3 = [-1.0; 3],
        "spd_add_random_max" => spd_add_random_max: point3 = [1.0; 3],
        "spd_mul" => spd_mul: point3 = [1.0; 3],
        "spd_mul_israndom" => spd_mul_israndom: flags3 = [false; 3],
        "spd_mul_random_min" => spd_mul_random_min: point3 = [0.75; 3],
        "spd_mul_random_max" => spd_mul_random_max: point3 = [0.9; 3],

        "rot_extend" => rot_extend: bool = false,
        "rot" => rot: point3 = [0.0; 3],
        "rot_israndom" => rot_israndom: flags3 = [true; 3],
        "rot_random_min" => rot_random_min: point3 = [0.0; 3],
        "rot_random_max" => rot_random_max: point3 = [360.0; 3],

        "rot_spd_extend" => rot_spd_extend: bool = false,
        "rot_spd" => rot_spd: point3 = [0.0; 3],
        "rot_spd_israndom" => rot_spd_israndom: flags3 = [true; 3],
        "rot_spd_random_min" => rot_spd_random_min: point3 = [-180.0; 3],
        "rot_spd_random_max" => rot_spd_random_max: point3 = [180.0; 3],
        "rot_spd_add" => rot_spd_add: point3 = [0.0; 3],
        "rot_spd_add_israndom" => rot_spd_add_israndom: flags3 = [false; 3],
        "rot_spd_add_random_min" => rot_spd_add_random_min: point3 = [-10.0; 3],
        "rot_spd_add_random_max" => rot_spd_add_random_max: point3 = [10.0; 3],
        "rot_spd_mul" => rot_spd_mul: point3 = [1.0; 3],
        "rot_spd_mul_israndom" => rot_spd_mul_israndom: flags3 = [false; 3],
        "rot_spd_mul_random_min" => rot_spd_mul_random_min: point3 = [0.75; 3],
        "rot_spd_mul_random_max" => rot_spd_mul_random_max: point3 = [0.9; 3],

        "sprite_angle" => sprite_angle: num = 0,
        "sprite_angle_israndom" => sprite_angle_israndom: bool = false,
        "sprite_angle_random_min" => sprite_angle_random_min: num = 0,
        "sprite_angle_random_max" => sprite_angle_random_max: num = 360,
        "sprite_angle_add" => sprite_angle_add: num = 0,
        "sprite_angle_add_israndom" => sprite_angle_add_israndom: bool = false,
        "sprite_angle_add_random_min" => sprite_angle_add_random_min: num = -90,
        "sprite_angle_add_random_max" => sprite_angle_add_random_max: num = 90,

        "scale" => scale: num = 1,
        "scale_israndom" => scale_israndom: bool = false,
        "scale_random_min" => scale_random_min: num = 0.5,
        "scale_random_max" => scale_random_max: num = 2,
        "scale_add" => scale_add: num = 0,
        "scale_add_israndom" => scale_add_israndom: bool = false,
        "scale_add_random_min" => scale_add_random_min: num = -0.2,
        "scale_add_random_max" => scale_add_random_max: num = -0.1,

        "alpha" => alpha: num = 1,
        "alpha_israndom" => alpha_israndom: bool = false,
        "alpha_random_min" => alpha_random_min: num = 0,
        "alpha_random_max" => alpha_random_max: num = 1,
        "alpha_add" => alpha_add: num = 0,
        "alpha_add_israndom" => alpha_add_israndom: bool = false,
        "alpha_add_random_min" => alpha_add_random_min: num = -0.1,
        "alpha_add_random_max" => alpha_add_random_max: num = -0.05,

        "color" => color: color = Color::WHITE,
        "color_israndom" => color_israndom: bool = false,
        "color_random_start" => color_random_start: color = Color::GRAY,
        "color_random_end" => color_random_end: color = Color::WHITE,
        "color_mix_enabled" => color_mix_enabled: bool = false,
        "color_mix" => color_mix: color = Color::BLACK,
        "color_mix_israndom" => color_mix_israndom: bool = false,
        "color_mix_random_start" => color_mix_random_start: color = Color::GRAY,
        "color_mix_random_end" => color_mix_random_end: color = Color::WHITE,
        "color_mix_time" => color_mix_time: num = 3,
        "color_mix_time_israndom" => color_mix_time_israndom: bool = false,
        "color_mix_time_random_min" => color_mix_time_random_min: num = 1,
        "color_mix_time_random_max" => color_mix_time_random_max: num = 5,

        "spawn_region" => spawn_region: bool = true,
        "bounding_box" => bounding_box: bool = true,
        "bounce" => bounce: bool = true,
        "bounce_factor" => bounce_factor: num = 0.5,
        "orbit" => orbit: bool = false,
    }
}

/// One particle type of a spawner (`obj_particle_type`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleType {
    pub id: SaveId,
    pub name: String,
    pub source: ParticleSource,
    pub settings: ParticleTypeSettings,
}

impl ParticleType {
    pub fn new(id: SaveId) -> Self {
        Self {
            id,
            name: String::new(),
            source: ParticleSource::Template,
            settings: ParticleTypeSettings::default(),
        }
    }

    fn save(&self, w: &mut JsonWriter) {
        w.object_start(None);
        w.var("id", self.id.as_str());
        w.var("name", &self.name);
        match &self.source {
            ParticleSource::Sheet => w.var("temp_type", "sheet"),
            ParticleSource::Template => w.var("temp_type", "template"),
            ParticleSource::Object(_) => w.var("temp_type", "templateobj"),
        }
        if let ParticleSource::Object(temp) = &self.source {
            save_obj(w, "temp", temp);
        }
        self.settings.save_fields(w);
        w.object_done();
    }

    fn load(map: &JsonObject, format: i32, fallback_id: SaveId) -> Self {
        let mut ptype = ParticleType::new(map.string("id").map(SaveId::new).unwrap_or(fallback_id));
        if let Some(name) = map.string("name") {
            ptype.name = name.to_owned();
        }

        ptype.source = if format < fmt::FORMAT_123_PRE_2 {
            // Sheets only, or a library template.
            match load_obj(map, "temp") {
                Some(ObjRef::Id(id)) => ParticleSource::Object(ObjRef::Id(id)),
                _ => ParticleSource::Sheet,
            }
        } else {
            match map.string("temp_type").unwrap_or("sheet") {
                "sheet" => ParticleSource::Sheet,
                "template" => ParticleSource::Template,
                _ => match load_obj(map, "temp") {
                    Some(reference) => ParticleSource::Object(reference),
                    None => ParticleSource::Sheet,
                },
            }
        };

        ptype.settings.load_fields(map);

        // Launch angle did not exist; make sure it has no effect.
        if format < fmt::FORMAT_123_PRE_2 {
            ptype.settings.angle_speed = 0.0;
            ptype.settings.angle_speed_israndom = false;
        }

        ptype
    }
}

/// A particle spawner: its settings and its particle types.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleSpawner {
    pub settings: SpawnerSettings,
    pub types: Vec<ParticleType>,
}

impl ParticleSpawner {
    /// `project_save_particles`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(Some("particles"));
        self.settings.save_fields(w);
        w.array_start(Some("types"));
        for ptype in &self.types {
            ptype.save(w);
        }
        w.array_done();
        w.object_done();
    }

    /// `project_load_particles`. `new_id` supplies ids for types that have
    /// none in the file.
    pub(crate) fn load(map: &JsonObject, format: i32, new_id: &mut dyn FnMut() -> SaveId) -> Self {
        let mut spawner = ParticleSpawner::default();
        spawner.settings.load_fields(map);
        for entry in map.array("types").unwrap_or_default() {
            if let Some(type_map) = entry.as_object() {
                let fallback = if type_map.string("id").is_some() { SaveId::new("") } else { new_id() };
                spawner.types.push(ParticleType::load(type_map, format, fallback));
            }
        }
        spawner
    }
}
