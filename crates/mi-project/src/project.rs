//! An open project.

use crate::tree::Tree;
use mi_anim::{update_scene, Playhead, SceneNode, SceneState};
use mi_core::{IdGenerator, SaveId, TlType};
use mi_format::project::{LoadOptions, ProjectFile, Resource, Template, Timeline};
use mi_format::FormatError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Why a project could not be opened or saved.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read \"{path}\": {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("could not write \"{path}\": {source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("could not open \"{path}\": {source}")]
    Format { path: PathBuf, source: FormatError },
    #[error("\"{0}\" is a project from before version 1.1.0, which cannot be opened yet")]
    LegacyProject(PathBuf),
    #[error("the project has not been saved to a file yet")]
    NoPath,
}

/// A project in memory: the data of its file plus what is derived from it.
#[derive(Debug, Clone)]
pub struct Project {
    path: Option<PathBuf>,
    file: ProjectFile,
    tree: Tree,
    timeline_index: HashMap<SaveId, usize>,
    template_index: HashMap<SaveId, usize>,
    resource_index: HashMap<SaveId, usize>,
    ids: IdGenerator,
    changed: bool,
}

/// Settings of the environment a project is opened in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectContext {
    /// Block texture index of the default ground in the loaded Minecraft
    /// assets.
    pub ground_slot: f64,
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self { ground_slot: 0.0 }
    }
}

/// The default particle seed of a project, which the original draws from
/// 1..=32000 whenever a project is created or opened.
fn default_seed(ids: &IdGenerator) -> f64 {
    1.0 + (ids.state() % 32000) as f64
}

impl Project {
    /// An empty project (`project_reset`).
    pub fn new(context: ProjectContext) -> Self {
        let ids = IdGenerator::from_time();
        Self::from_file(ProjectFile::new(context.ground_slot, default_seed(&ids)), ids).0
    }

    /// Wraps loaded data. Returns the project and a list of repairs made to
    /// the timeline tree.
    pub fn from_file(file: ProjectFile, ids: IdGenerator) -> (Self, Vec<String>) {
        let (tree, warnings) = Tree::build(&file.objects.timelines);
        let mut project = Self {
            path: None,
            file,
            tree,
            timeline_index: HashMap::new(),
            template_index: HashMap::new(),
            resource_index: HashMap::new(),
            ids,
            changed: false,
        };
        project.rebuild_indices();
        (project, warnings)
    }

    fn rebuild_indices(&mut self) {
        let objects = &self.file.objects;
        self.timeline_index = objects.timelines.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        self.template_index = objects.templates.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        self.resource_index = objects.resources.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect();
    }

    /// Opens a `.miproject` or backup file (`project_load`). Returns the
    /// project and warnings about anything that was skipped or repaired.
    pub fn open(path: &Path, context: ProjectContext) -> Result<(Self, Vec<String>), ProjectError> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if extension == "mproj" || extension == "mani" {
            return Err(ProjectError::LegacyProject(path.to_owned()));
        }

        let bytes = std::fs::read(path).map_err(|source| ProjectError::Read { path: path.to_owned(), source })?;
        let mut ids = IdGenerator::from_time();
        let seed = default_seed(&ids);
        let options = LoadOptions { ground_slot: context.ground_slot, seed, new_id: &mut || ids.next_id() };
        let loaded = ProjectFile::load(&bytes, options)
            .map_err(|source| ProjectError::Format { path: path.to_owned(), source })?;

        let mut warnings = loaded.warnings;
        let (mut project, tree_warnings) = Self::from_file(loaded.file, ids);
        warnings.extend(tree_warnings);

        // Backups open as the project they belong to but are not saved over.
        project.path = (extension == "miproject").then(|| path.to_owned());
        if project.file.info.name.is_empty() {
            project.file.info.name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_owned();
        }
        Ok((project, warnings))
    }

    /// Writes the project to its file (`project_save`).
    pub fn save(&mut self) -> Result<(), ProjectError> {
        let path = self.path.clone().ok_or(ProjectError::NoPath)?;
        self.save_as(&path)
    }

    /// Writes the project to `path` and makes that its file.
    pub fn save_as(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.sync_tree_indices();
        let text = self.file.save();
        std::fs::write(path, text).map_err(|source| ProjectError::Write { path: path.to_owned(), source })?;
        self.path = Some(path.to_owned());
        self.changed = false;
        Ok(())
    }

    /// Updates `parent_tree_index` of every timeline from the tree.
    fn sync_tree_indices(&mut self) {
        for i in 0..self.file.objects.timelines.len() {
            let index = self.tree.index_in_parent(i) as i64;
            let parent = match self.tree.parent(i) {
                Some(p) => self.file.objects.timelines[p].id.clone(),
                None => SaveId::root(),
            };
            let tl = &mut self.file.objects.timelines[i];
            tl.parent_tree_index = Some(index);
            tl.parent = parent;
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Folder that resource file names are relative to.
    pub fn folder(&self) -> Option<&Path> {
        self.path.as_deref().and_then(Path::parent)
    }

    pub fn file(&self) -> &ProjectFile {
        &self.file
    }

    /// Whether there are changes that have not been saved.
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn timelines(&self) -> &[Timeline] {
        &self.file.objects.timelines
    }

    pub fn templates(&self) -> &[Template] {
        &self.file.objects.templates
    }

    pub fn resources(&self) -> &[Resource] {
        &self.file.objects.resources
    }

    pub fn timeline_index(&self, id: &SaveId) -> Option<usize> {
        self.timeline_index.get(id).copied()
    }

    pub fn timeline(&self, id: &SaveId) -> Option<&Timeline> {
        self.timeline_index(id).map(|i| &self.file.objects.timelines[i])
    }

    pub fn template(&self, id: &SaveId) -> Option<&Template> {
        self.template_index.get(id).map(|&i| &self.file.objects.templates[i])
    }

    pub fn resource(&self, id: &SaveId) -> Option<&Resource> {
        self.resource_index.get(id).map(|&i| &self.file.objects.resources[i])
    }

    /// A save id that no object of the project uses.
    pub fn new_id(&mut self) -> SaveId {
        loop {
            let id = self.ids.next_id();
            let used = self.timeline_index.contains_key(&id)
                || self.template_index.contains_key(&id)
                || self.resource_index.contains_key(&id);
            if !used {
                return id;
            }
        }
    }

    /// Frame of the last keyframe (`tl_update_length`). Audio clips will
    /// extend this once sounds can be loaded.
    pub fn length(&self) -> i64 {
        self.timelines().iter().filter_map(|tl| tl.keyframes.last()).map(|k| k.position).max().unwrap_or(0)
    }

    /// A playhead at `marker` with the project's loop settings.
    pub fn playhead(&self, marker: f64) -> Playhead {
        let timeline = &self.file.info.timeline;
        Playhead {
            marker,
            seamless_repeat: timeline.seamless_repeat,
            region: timeline.region_start.zip(timeline.region_end),
            length: self.length() as f64,
        }
    }

    /// The timelines in tree order, as the transform update wants them.
    /// The second list maps each node back to its timeline index.
    pub fn scene_nodes(&self) -> (Vec<SceneNode<'_>>, Vec<usize>) {
        let order = self.tree.order().to_vec();
        let mut node_of = vec![usize::MAX; self.timelines().len()];
        for (node, &timeline) in order.iter().enumerate() {
            node_of[timeline] = node;
        }

        let nodes = order
            .iter()
            .map(|&i| {
                let timeline = &self.timelines()[i];
                SceneNode {
                    timeline,
                    parent: self.tree.parent(i).map(|p| node_of[p]),
                    part_of: timeline.part_of.as_id().and_then(|id| self.timeline_index(id)).map(|p| node_of[p]),
                    // Model data comes from the Minecraft assets, which are
                    // not loaded yet.
                    part: None,
                    rot_point: if timeline.rot_point_custom { timeline.rot_point } else { [0.0; 3] },
                }
            })
            .collect();
        (nodes, order)
    }

    /// Evaluates the whole scene at frame `marker`. `SceneState::nodes` is
    /// in tree order; the returned list gives the timeline index of each.
    pub fn evaluate(&self, marker: f64) -> (SceneState, Vec<usize>) {
        let (nodes, order) = self.scene_nodes();
        (update_scene(&nodes, &self.playhead(marker)), order)
    }

    /// The camera timeline that is active at the evaluated frame: the first
    /// visible camera in tree order (`app_update_animate`).
    pub fn active_camera(&self, state: &SceneState, order: &[usize]) -> Option<usize> {
        order.iter().enumerate().find_map(|(node, &timeline)| {
            let tl = &self.timelines()[timeline];
            (tl.kind == TlType::Camera && !tl.hide && state.nodes[node].inherited.visible).then_some(timeline)
        })
    }
}
