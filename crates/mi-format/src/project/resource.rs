//! Resources (`obj_resource`): files a project refers to, and timeline
//! markers.

use super::LoadContext;
use crate::json::{JsonObject, JsonWriter};
use mi_core::version::project as fmt;
use mi_core::{MaterialFormat, ResType, SaveId};

/// An external file used by the project. `filename` is relative to the
/// project folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub id: SaveId,
    pub kind: ResType,
    pub filename: String,
    /// Skins: whether the image uses the player skin layout.
    pub player_skin: bool,
    /// Item sheets: number of columns and rows.
    pub item_sheet_size: [f64; 2],
    /// Scenery: create timelines for block entities (chests, banners, ...).
    pub scenery_tl_add: Option<bool>,
    pub scenery_download_skins: bool,
    /// Structure files: which palette to use.
    pub scenery_palette: f64,
    /// Structure files: fraction of blocks kept.
    pub scenery_integrity: f64,
    pub scenery_integrity_invert: bool,
    pub scenery_randomize: bool,
    /// World imports: region folder and selected box.
    pub world_regions_dir: String,
    pub world_box_start: Option<[f64; 3]>,
    pub world_box_end: Option<[f64; 3]>,
    pub world_filter_mode: f64,
    pub world_filter_array: Vec<String>,
    /// Index into [`MaterialFormat`].
    pub material_format: f64,
}

impl Resource {
    /// A resource with the settings of `res_event_create`.
    pub fn new(id: SaveId, kind: ResType) -> Self {
        Self {
            id,
            kind,
            filename: String::new(),
            player_skin: false,
            item_sheet_size: [32.0, 32.0],
            scenery_tl_add: None,
            scenery_download_skins: true,
            scenery_palette: 0.0,
            scenery_integrity: 1.0,
            scenery_integrity_invert: false,
            scenery_randomize: true,
            world_regions_dir: String::new(),
            world_box_start: None,
            world_box_end: None,
            world_filter_mode: 0.0,
            world_filter_array: Vec::new(),
            material_format: MaterialFormat::LabPbr.index() as f64,
        }
    }

    /// `project_save_resource`
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(None);
        w.var("id", self.id.as_str());
        w.var("type", self.kind.name());
        w.var("filename", &self.filename);

        if matches!(self.kind, ResType::Skin | ResType::DownloadedSkin) {
            w.var_bool("player_skin", self.player_skin);
        }
        if self.kind == ResType::ItemSheet {
            w.var_point2("item_sheet_size", self.item_sheet_size);
        }
        if matches!(self.kind, ResType::Scenery | ResType::FromWorld) {
            w.var_bool("scenery_tl_add", self.scenery_tl_add.unwrap_or(false));
            w.var_bool("scenery_download_skins", self.scenery_download_skins);
        }
        if self.kind == ResType::Scenery {
            w.var("scenery_palette", self.scenery_palette);
            w.var("scenery_integrity", self.scenery_integrity);
            w.var_bool("scenery_integrity_invert", self.scenery_integrity_invert);
            w.var_bool("randomize", self.scenery_randomize);
        }
        if self.kind == ResType::FromWorld {
            w.var("world_regions_dir", &self.world_regions_dir);
            w.var_point3_nullable("world_box_start", self.world_box_start);
            w.var_point3_nullable("world_box_end", self.world_box_end);
            w.var("world_filter_mode", self.world_filter_mode);
            w.array_start(Some("world_filter_array"));
            for entry in &self.world_filter_array {
                w.array_value(entry);
            }
            w.array_done();
        }
        w.var("material_format", self.material_format);
        w.object_done();
    }

    /// `project_load_resource`
    pub(crate) fn load(map: &JsonObject, ctx: &mut LoadContext) -> Option<Self> {
        let mut type_name = map.string("type").unwrap_or("");
        if ctx.format < fmt::FORMAT_200_PRE_5 && type_name == "schematic" {
            type_name = "scenery";
        }
        let Some(kind) = ResType::from_name(type_name) else {
            ctx.warn(format!("Skipped a resource of unknown type \"{type_name}\""));
            return None;
        };
        let id = match map.string("id") {
            Some(id) => SaveId::new(id),
            None => ctx.new_id(),
        };
        let mut res = Resource::new(id, kind);
        if let Some(v) = map.string("filename") {
            res.filename = v.to_owned();
        }

        if matches!(kind, ResType::Skin | ResType::DownloadedSkin) {
            if let Some(v) = map.flag("player_skin") {
                res.player_skin = v;
            }
        }
        if kind == ResType::ItemSheet {
            if let Some(v) = map.point2("item_sheet_size") {
                res.item_sheet_size = v;
            }
        }
        if matches!(kind, ResType::Scenery | ResType::FromWorld) {
            res.scenery_tl_add = Some(map.flag("scenery_tl_add").unwrap_or(true));
            res.scenery_download_skins = map.flag("scenery_download_skins").unwrap_or(false);
        }
        if kind == ResType::Scenery {
            if let Some(v) = map.real("scenery_palette") {
                res.scenery_palette = v;
            }
            if let Some(v) = map.real("scenery_integrity") {
                res.scenery_integrity = v;
            }
            if let Some(v) = map.flag("scenery_integrity_invert") {
                res.scenery_integrity_invert = v;
            }
            if let Some(v) = map.flag("randomize") {
                res.scenery_randomize = v;
            }
        }
        if kind == ResType::FromWorld {
            res.world_regions_dir = map.string("world_regions_dir").unwrap_or("").to_owned();
            res.world_box_start = map.point3("world_box_start");
            res.world_box_end = map.point3("world_box_end");
            res.world_filter_mode = map.real("world_filter_mode").unwrap_or(0.0);
            res.world_filter_array = map
                .array("world_filter_array")
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
        if let Some(v) = map.real("material_format") {
            res.material_format = v;
        }

        Some(res)
    }
}

/// A named position on the timeline (`obj_marker`).
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub id: SaveId,
    /// Frame number.
    pub position: f64,
    pub name: String,
    /// Colour tag index.
    pub color: f64,
}

impl Marker {
    pub(crate) fn save(&self, w: &mut JsonWriter) {
        w.object_start(None);
        w.var("id", self.id.as_str());
        w.var("position", self.position);
        w.var("name", &self.name);
        w.var("color", self.color);
        w.object_done();
    }

    pub(crate) fn load(map: &JsonObject, ctx: &mut LoadContext) -> Self {
        Self {
            id: match map.string("id") {
                Some(id) => SaveId::new(id),
                None => ctx.new_id(),
            },
            position: map.real("position").unwrap_or(0.0),
            name: map.string("name").unwrap_or("").to_owned(),
            color: map.real("color").unwrap_or(0.0),
        }
    }
}
