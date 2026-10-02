//! The scenery resources of a project, read from the files next to it
//! (`res_load_scenery`).

use crate::Project;
use mi_assets::{AssetPack, LegacyBlocks, Scenery, SceneryOptions};
use mi_core::{ResType, SaveId};
use std::collections::HashMap;
use std::sync::Arc;

/// Above this many blocks that would become timelines, the original never
/// adds them; between 20 and this it asks.
const MAX_SCENERY_TIMELINES: usize = 500;

/// A scenery resource ready to be drawn.
#[derive(Debug, Clone)]
pub struct LoadedScenery {
    pub scenery: Arc<Scenery>,
    /// Whether blocks such as chests and doors are timelines of their own
    /// rather than part of the scenery mesh.
    pub timelines: bool,
    /// Whether blocks with several models pick one at random.
    pub randomize: bool,
}

impl LoadedScenery {
    /// The size in blocks in the timeline's axes (`scenery_size`): the
    /// builder's X and Y are swapped by the turn of block meshes.
    pub fn size(&self) -> [f64; 3] {
        let s = self.scenery.size;
        [s[1] as f64, s[0] as f64, s[2] as f64]
    }
}

/// The scenery of all resources of a project.
#[derive(Debug, Default)]
pub struct SceneryStore {
    scenery: HashMap<SaveId, LoadedScenery>,
    /// Resources that could not be read, with the reason.
    pub errors: Vec<(SaveId, String)>,
}

impl SceneryStore {
    pub fn load(project: &Project, pack: &AssetPack, legacy: &LegacyBlocks) -> Self {
        let mut store = Self::default();
        for resource in project.resources() {
            match resource.kind {
                ResType::Scenery => {}
                ResType::FromWorld => {
                    store.errors.push((resource.id.clone(), "scenery from worlds is not supported yet".into()));
                    continue;
                }
                _ => continue,
            }
            let Some(folder) = project.folder() else { continue };
            let path = folder.join(&resource.filename);
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    store.errors.push((resource.id.clone(), format!("{}: {error}", path.display())));
                    continue;
                }
            };
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let options = SceneryOptions {
                palette: resource.scenery_palette.max(0.0) as usize,
                integrity: resource.scenery_integrity,
                integrity_invert: resource.scenery_integrity_invert,
            };
            match Scenery::read(&bytes, extension, pack.blocks(), legacy, options) {
                Ok(scenery) => {
                    // Projects saved by the original record the choice; for
                    // others it is made as the original makes it, adding
                    // timelines where it would ask.
                    let timelines = resource
                        .scenery_tl_add
                        .unwrap_or_else(|| scenery.timeline_count(pack.blocks()) <= MAX_SCENERY_TIMELINES);
                    let loaded =
                        LoadedScenery { scenery: Arc::new(scenery), timelines, randomize: resource.scenery_randomize };
                    store.scenery.insert(resource.id.clone(), loaded);
                }
                Err(error) => store.errors.push((resource.id.clone(), format!("{}: {error}", path.display()))),
            }
        }
        store
    }

    /// Adds or replaces the scenery of a resource.
    pub fn insert(&mut self, resource: SaveId, scenery: LoadedScenery) {
        self.scenery.insert(resource, scenery);
    }

    pub fn get(&self, resource: &SaveId) -> Option<&LoadedScenery> {
        self.scenery.get(resource)
    }

    pub fn len(&self) -> usize {
        self.scenery.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenery.is_empty()
    }
}
