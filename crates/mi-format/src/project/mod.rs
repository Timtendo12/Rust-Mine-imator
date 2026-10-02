//! The JSON project family: `.miproject`, `.miobject`, `.miframes`,
//! `.miparticles` and `.mirender` (formats 24 and up).
//!
//! All five share a header (`format`, `created_in`) and reuse the same
//! objects. This module translates between those files and plain data; it
//! applies the upgrades of older formats that need nothing but the file
//! itself. Upgrades that depend on Minecraft assets or on the legacy lookup
//! tables (block ids, renamed models, item slots) are left to the project
//! layer, which is why [`ProjectFile::loaded_format`] is kept.

mod particles;
mod resource;
mod settings;
mod template;
mod timeline;

pub use particles::{ParticleSource, ParticleSpawner, ParticleType, ParticleTypeSettings, SpawnerSettings};
pub use resource::{Marker, Resource};
pub use settings::{colors, Background, ProjectInfo, RenderSettings, TimelineSettings, ViewCamera, WorkCamera};
pub use template::{ArmorPiece, ItemSettings, Pattern, ShapeSettings, Template, TextSettings, ARMOR_SLOTS, LEATHER_COLOR};
pub use timeline::{Appearance, Inherit, Keyframe, PathSettings, Timeline};

use crate::json::{self, JsonObject, JsonWriter};
use crate::values::ValueSet;
use crate::FormatError;
use mi_core::version::{self, project as fmt};
use mi_core::SaveId;

/// State shared while reading one file.
pub(crate) struct LoadContext<'a> {
    pub format: i32,
    pub warnings: Vec<String>,
    pub loaded_keyframes: usize,
    id_source: &'a mut dyn FnMut() -> SaveId,
}

impl LoadContext<'_> {
    pub fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    pub fn new_id(&mut self) -> SaveId {
        (self.id_source)()
    }
}

/// What the loader needs to know about the program it runs in.
pub struct LoadOptions<'a> {
    /// Block texture index of the default ground (`BG_GROUND_SLOT` default).
    pub ground_slot: f64,
    /// Default particle seed of the project being created.
    pub seed: f64,
    /// Produces save ids for objects that have none in the file.
    pub new_id: &'a mut dyn FnMut() -> SaveId,
}

/// Parses the header shared by all project-family files and checks that the
/// format can be read (`project_load_start`).
fn read_header(bytes: &[u8]) -> Result<(JsonObject, i32, String), FormatError> {
    let root = match json::parse(bytes)? {
        json::Json::Object(root) => root,
        _ => return Err(FormatError::Corrupted("the root is not an object".to_owned())),
    };
    let format = root
        .get("format")
        .and_then(|f| match f {
            json::Json::Number(n) => Some(*n as i32),
            _ => None,
        })
        .ok_or_else(|| FormatError::Corrupted("missing parameter \"format\"".to_owned()))?;
    if format > fmt::CURRENT {
        return Err(FormatError::TooNew { format, supported: fmt::CURRENT });
    }
    if format < fmt::FORMAT_110_PRE_1 {
        return Err(FormatError::Corrupted(format!("invalid format {format} for a JSON project file")));
    }
    let created_in = root.string("created_in").unwrap_or("").to_owned();
    Ok((root, format, created_in))
}

fn write_header(w: &mut JsonWriter, created_in: &str) {
    w.object_start(None);
    w.var("format", fmt::CURRENT);
    w.var("created_in", created_in);
}

/// The version string written to `created_in`.
pub fn created_in() -> String {
    version::MINEIMATOR_VERSION.to_owned()
}

/// Templates, timelines and resources: the body of `.miobject` files and
/// part of every other project-family file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Objects {
    pub templates: Vec<Template>,
    pub timelines: Vec<Timeline>,
    pub resources: Vec<Resource>,
}

impl Objects {
    /// `project_save_objects`
    fn save(&self, w: &mut JsonWriter, project_defaults: &ValueSet) {
        w.array_start(Some("templates"));
        for template in &self.templates {
            template.save(w);
        }
        w.array_done();

        w.array_start(Some("timelines"));
        for timeline in &self.timelines {
            timeline.save(w, project_defaults);
        }
        w.array_done();

        w.array_start(Some("resources"));
        for resource in &self.resources {
            resource.save(w);
        }
        w.array_done();
    }

    /// `project_load_objects`
    fn load(root: &JsonObject, ctx: &mut LoadContext, defaults: &ValueSet, background: &Background) -> Self {
        let entries = |key: &str| root.array(key).unwrap_or_default().iter().filter_map(|e| e.as_object());
        Self {
            templates: entries("templates").filter_map(|m| Template::load(m, ctx)).collect(),
            timelines: entries("timelines").filter_map(|m| Timeline::load(m, ctx, defaults, background)).collect(),
            resources: entries("resources").filter_map(|m| Resource::load(m, ctx)).collect(),
        }
    }
}

/// A `.miproject` file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFile {
    /// Format number the file was read from ([`fmt::CURRENT`] for new
    /// projects). Saving always writes the current format.
    pub loaded_format: i32,
    pub created_in: String,
    pub info: ProjectInfo,
    pub render: RenderSettings,
    pub background: Background,
    pub objects: Objects,
    pub markers: Vec<Marker>,
    /// The project's default values, which timelines are diffed against.
    pub defaults: ValueSet,
}

/// Result of reading a file: the content plus anything that was skipped or
/// repaired on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded<T> {
    pub file: T,
    pub warnings: Vec<String>,
}

impl ProjectFile {
    /// An empty project (`project_reset`).
    pub fn new(ground_slot: f64, seed: f64) -> Self {
        let background = Background::default();
        let defaults = ValueSet::project_defaults(&background, ground_slot, seed);
        Self {
            loaded_format: fmt::CURRENT,
            created_in: created_in(),
            info: ProjectInfo::default(),
            render: RenderSettings::default(),
            background,
            objects: Objects::default(),
            markers: Vec::new(),
            defaults,
        }
    }

    /// Reads a `.miproject` (or `.backupN`) file.
    pub fn load(bytes: &[u8], options: LoadOptions) -> Result<Loaded<Self>, FormatError> {
        let (root, format, created_in) = read_header(bytes)?;
        let mut ctx = LoadContext { format, warnings: Vec::new(), loaded_keyframes: 0, id_source: options.new_id };

        // Timelines start from the defaults of an empty project, not from the
        // loaded background, exactly as the original does.
        let defaults = ValueSet::project_defaults(&Background::default(), options.ground_slot, options.seed);

        let mut info = ProjectInfo::default();
        if let Some(map) = root.object("project") {
            info.load(map, format);
        }
        let mut render = RenderSettings::default();
        if let Some(map) = root.object("render") {
            render.load_fields(map);
        }
        let mut background = Background::default();
        if let Some(map) = root.object("background") {
            background.load(map, format);
        }

        let objects = Objects::load(&root, &mut ctx, &defaults, &background);

        // The original performs this as a side effect of upgrading a
        // keyframe, so projects without keyframes keep the default colours.
        if format < fmt::FORMAT_200_PRE_5 && ctx.loaded_keyframes > 0 {
            background.upgrade_leaf_colors();
        }

        let mut markers: Vec<Marker> = root
            .array("markers")
            .unwrap_or_default()
            .iter()
            .filter_map(|m| m.as_object())
            .map(|m| Marker::load(m, &mut ctx))
            .collect();
        markers.sort_by(|a, b| a.position.total_cmp(&b.position));

        Ok(Loaded {
            file: Self { loaded_format: format, created_in, info, render, background, objects, markers, defaults },
            warnings: ctx.warnings,
        })
    }

    /// Writes the project in the current format (`project_save`).
    pub fn save(&self) -> String {
        let mut w = JsonWriter::new();
        write_header(&mut w, &self.created_in);
        self.info.save(&mut w);
        self.render.save(&mut w);
        self.background.save(&mut w);
        self.objects.save(&mut w, &self.defaults);
        if !self.markers.is_empty() {
            w.array_start(Some("markers"));
            for marker in &self.markers {
                marker.save(&mut w);
            }
            w.array_done();
        }
        w.object_done();
        w.finish()
    }
}

/// A `.miobject` file: a selection of timelines with the templates and
/// resources they need.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFile {
    pub loaded_format: i32,
    pub created_in: String,
    pub objects: Objects,
}

impl ObjectFile {
    /// Reads the file. `defaults` are the default values of the project the
    /// objects are loaded into and `background` its background.
    pub fn load(
        bytes: &[u8],
        defaults: &ValueSet,
        background: &Background,
        new_id: &mut dyn FnMut() -> SaveId,
    ) -> Result<Loaded<Self>, FormatError> {
        let (root, format, created_in) = read_header(bytes)?;
        let mut ctx = LoadContext { format, warnings: Vec::new(), loaded_keyframes: 0, id_source: new_id };
        let objects = Objects::load(&root, &mut ctx, defaults, background);
        Ok(Loaded { file: Self { loaded_format: format, created_in, objects }, warnings: ctx.warnings })
    }

    pub fn save(&self, project_defaults: &ValueSet) -> String {
        let mut w = JsonWriter::new();
        write_header(&mut w, &self.created_in);
        self.objects.save(&mut w, project_defaults);
        w.object_done();
        w.finish()
    }
}

/// A `.miparticles` file: one spawner plus the objects it refers to.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticlesFile {
    pub loaded_format: i32,
    pub created_in: String,
    pub particles: ParticleSpawner,
    pub objects: Objects,
}

impl ParticlesFile {
    pub fn load(
        bytes: &[u8],
        defaults: &ValueSet,
        background: &Background,
        new_id: &mut dyn FnMut() -> SaveId,
    ) -> Result<Loaded<Self>, FormatError> {
        let (root, format, created_in) = read_header(bytes)?;
        let mut ctx = LoadContext { format, warnings: Vec::new(), loaded_keyframes: 0, id_source: new_id };
        let particles = match root.object("particles") {
            Some(map) => ParticleSpawner::load(map, format, &mut || ctx.new_id()),
            None => return Err(FormatError::Corrupted("missing \"particles\" object".to_owned())),
        };
        let objects = Objects::load(&root, &mut ctx, defaults, background);
        Ok(Loaded { file: Self { loaded_format: format, created_in, particles, objects }, warnings: ctx.warnings })
    }

    pub fn save(&self, project_defaults: &ValueSet) -> String {
        let mut w = JsonWriter::new();
        write_header(&mut w, &self.created_in);
        self.particles.save(&mut w);
        self.objects.save(&mut w, project_defaults);
        w.object_done();
        w.finish()
    }
}

/// A `.mirender` file: a render settings preset.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFile {
    pub loaded_format: i32,
    pub created_in: String,
    pub render: RenderSettings,
}

impl RenderFile {
    /// Reads the preset on top of `current`, as importing does: settings
    /// missing from the file keep their current value.
    pub fn load(bytes: &[u8], current: &RenderSettings) -> Result<Self, FormatError> {
        let (root, format, created_in) = read_header(bytes)?;
        let mut render = current.clone();
        if let Some(map) = root.object("render") {
            render.load_fields(map);
        }
        Ok(Self { loaded_format: format, created_in, render })
    }

    pub fn save(&self) -> String {
        let mut w = JsonWriter::new();
        write_header(&mut w, &self.created_in);
        self.render.save(&mut w);
        w.object_done();
        w.finish()
    }
}

/// One keyframe of a `.miframes` file. The values are kept as written
/// because they are relative to the defaults of whichever timeline the
/// keyframe ends up on.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeEntry {
    /// Frame offset from the first keyframe of the file.
    pub position: f64,
    /// Model part the keyframe belongs to, in model animations.
    pub part_name: Option<String>,
    values: JsonObject,
}

impl KeyframeEntry {
    /// Captures a keyframe for saving: the values of `values` that differ
    /// from the timeline's `defaults`.
    pub fn capture(position: f64, part_name: Option<String>, values: &ValueSet, defaults: &ValueSet) -> Self {
        let mut w = JsonWriter::new();
        w.object_start(None);
        values.save_diff(&mut w, "values", defaults);
        w.object_done();
        let doc = json::parse(w.finish().as_bytes()).expect("the writer produces valid JSON");
        let values = doc.as_object().and_then(|o| o.object("values")).cloned().unwrap_or_default();
        Self { position, part_name, values }
    }

    /// The complete value set of this keyframe on a timeline with the given
    /// defaults, read according to the file's `format`.
    pub fn resolve(&self, defaults: &ValueSet, format: i32) -> ValueSet {
        let mut values = defaults.clone();
        values.load_from(&self.values, format);
        values
    }
}

/// A `.miframes` file: keyframes of one timeline, or of the parts of one
/// model (`keyframes_save`).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframesFile {
    pub loaded_format: i32,
    pub created_in: String,
    /// Whether keyframes are spread over the parts of a model.
    pub is_model: bool,
    /// Frames per second the positions are expressed in.
    pub tempo: f64,
    /// Distance between the first and last keyframe.
    pub length: f64,
    pub keyframes: Vec<KeyframeEntry>,
    pub objects: Objects,
}

impl KeyframesFile {
    /// Reads the file. `project_tempo` is used when the file has no tempo.
    pub fn load(
        bytes: &[u8],
        project_tempo: f64,
        defaults: &ValueSet,
        background: &Background,
        new_id: &mut dyn FnMut() -> SaveId,
    ) -> Result<Loaded<Self>, FormatError> {
        let (root, format, created_in) = read_header(bytes)?;
        let mut ctx = LoadContext { format, warnings: Vec::new(), loaded_keyframes: 0, id_source: new_id };
        let keyframes = root
            .array("keyframes")
            .unwrap_or_default()
            .iter()
            .filter_map(|k| k.as_object())
            .map(|k| KeyframeEntry {
                position: k.real("position").unwrap_or(0.0),
                part_name: k.string("part_name").map(str::to_owned),
                values: k.object("values").cloned().unwrap_or_default(),
            })
            .collect();
        let objects = Objects::load(&root, &mut ctx, defaults, background);
        Ok(Loaded {
            file: Self {
                loaded_format: format,
                created_in,
                is_model: root.flag("is_model").unwrap_or(false),
                tempo: root.real("tempo").unwrap_or(project_tempo),
                length: root.real("length").unwrap_or(0.0),
                keyframes,
                objects,
            },
            warnings: ctx.warnings,
        })
    }

    pub fn save(&self, project_defaults: &ValueSet) -> String {
        let mut w = JsonWriter::new();
        write_header(&mut w, &self.created_in);
        w.var_bool("is_model", self.is_model);
        w.var("tempo", self.tempo);
        w.var("length", self.length);
        w.array_start(Some("keyframes"));
        for keyframe in &self.keyframes {
            w.object_start(None);
            w.var("position", keyframe.position);
            if let Some(part_name) = &keyframe.part_name {
                w.var("part_name", part_name);
            }
            json::write_object(&mut w, "values", &keyframe.values);
            w.object_done();
        }
        w.array_done();
        self.objects.save(&mut w, project_defaults);
        w.object_done();
        w.finish()
    }
}
