//! An open project.

use crate::tree::Tree;
use mi_anim::{update_scene, PartInfo, Playhead, SceneNode, SceneState};
use mi_core::{IdGenerator, SaveId, TlType};
use mi_format::language::Language;
use mi_format::project::{LoadOptions, ProjectFile, Resource, Template, Timeline};
use mi_core::{ObjRef, TempType};
use mi_format::FormatError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Vec3 = [f64; 3];

/// The size in blocks of a loaded scenery resource, in the timeline's
/// axes; `None` while it is not loaded.
pub type ScenerySize<'a> = dyn Fn(&SaveId) -> Option<[f64; 3]> + 'a;

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
    /// Where the project's resources are; also set for an opened backup,
    /// which has no file of its own to save to.
    folder: Option<PathBuf>,
    /// Changed since the last save or backup.
    pub(crate) unsaved_backup: bool,
    pub(crate) file: ProjectFile,
    tree: Tree,
    timeline_index: HashMap<SaveId, usize>,
    template_index: HashMap<SaveId, usize>,
    resource_index: HashMap<SaveId, usize>,
    ids: IdGenerator,
    /// Files of resources that were added since the project was saved:
    /// they are read from where they were picked and copied next to the
    /// project when it is saved (`load_folder` in the original).
    pub(crate) resource_sources: HashMap<SaveId, PathBuf>,
    pub(crate) changed: bool,
    pub(crate) history: crate::history::History,
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

/// `path` with `suffix` added to its file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// File name without folder and extension.
fn file_stem(filename: &str) -> String {
    let name = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => name.to_owned(),
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
            folder: None,
            unsaved_backup: false,
            file,
            tree,
            timeline_index: HashMap::new(),
            template_index: HashMap::new(),
            resource_index: HashMap::new(),
            ids,
            resource_sources: HashMap::new(),
            changed: false,
            history: Default::default(),
        };
        project.rebuild_indices();
        (project, warnings)
    }

    /// Rebuilds the timeline tree after timelines or parents changed.
    pub(crate) fn rebuild_tree(&mut self) {
        // Repairs were reported when the project was opened.
        self.tree = Tree::build(&self.file.objects.timelines).0;
    }

    pub(crate) fn rebuild_indices(&mut self) {
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
        project.folder = path.parent().map(Path::to_owned);
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

    /// Writes the project to `path` and makes that its file. The files of
    /// its resources are copied next to it (`res_save`), from where they
    /// were added or from the folder the project was in.
    pub fn save_as(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.sync_tree_indices();
        let text = self.file.save();
        std::fs::write(path, text).map_err(|source| ProjectError::Write { path: path.to_owned(), source })?;

        if let Some(folder) = path.parent() {
            for resource in &self.file.objects.resources {
                // Scenery from worlds has no file of its own.
                if resource.kind == mi_core::ResType::FromWorld || resource.filename.is_empty() {
                    continue;
                }
                let Some(from) = self.resource_path(resource) else { continue };
                let to = folder.join(&resource.filename);
                let same = from == to || matches!((from.canonicalize(), to.canonicalize()), (Ok(a), Ok(b)) if a == b);
                if same || !from.is_file() {
                    continue;
                }
                if let Some(parent) = to.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::copy(&from, &to).map_err(|source| ProjectError::Write { path: to.clone(), source })?;
            }
        }
        // From now on the copies next to the project are the resources.
        self.resource_sources.clear();
        self.path = Some(path.to_owned());
        self.folder = path.parent().map(Path::to_owned);
        self.changed = false;
        self.unsaved_backup = false;
        Ok(())
    }

    /// The newest backup of the project (`action_toolbar_open_last_backup`),
    /// if there is one. Backups are named after the project's folder.
    pub fn last_backup(&self) -> Option<PathBuf> {
        let base = self.backup_base()?;
        [".backup1", ".backup"].iter().map(|suffix| with_suffix(&base, suffix)).find(|path| path.is_file())
    }

    /// `<folder>/<folder name>`, which `.backupN` is appended to.
    fn backup_base(&self) -> Option<PathBuf> {
        // A project that was never saved has nowhere to keep backups.
        self.path.as_ref()?;
        let folder = self.folder.as_ref()?;
        Some(folder.join(folder.file_name()?))
    }

    /// Writes a backup of the project next to it (`project_backup`), keeping
    /// `amount` of them: `.backup1` is the newest, older ones move up a
    /// number and the oldest is dropped. Returns the file written, or `None`
    /// when there was nothing to back up: the project has no file yet, or
    /// nothing changed since the last save or backup.
    pub fn backup(&mut self, amount: usize) -> Result<Option<PathBuf>, ProjectError> {
        if !self.unsaved_backup {
            return Ok(None);
        }
        let Some(base) = self.backup_base() else { return Ok(None) };
        for number in (1..amount).rev() {
            let from = with_suffix(&base, &format!(".backup{number}"));
            if from.is_file() {
                let to = with_suffix(&base, &format!(".backup{}", number + 1));
                // Renaming fails on some systems when the target exists.
                let _ = std::fs::remove_file(&to);
                std::fs::rename(&from, &to).map_err(|source| ProjectError::Write { path: to.clone(), source })?;
            }
        }
        let path = with_suffix(&base, if amount > 1 { ".backup1" } else { ".backup" });
        self.sync_tree_indices();
        std::fs::write(&path, self.file.save()).map_err(|source| ProjectError::Write { path: path.clone(), source })?;
        self.unsaved_backup = false;
        Ok(Some(path))
    }

    /// Where the file of a resource is: where it was added from, or else
    /// next to the project. `None` for an unsaved project's own resources.
    pub fn resource_path(&self, resource: &Resource) -> Option<PathBuf> {
        match self.resource_sources.get(&resource.id) {
            Some(source) => Some(source.clone()),
            None => Some(self.folder()?.join(&resource.filename)),
        }
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
        self.folder.as_deref()
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

    /// Name a template is shown with (`temp_update_display_name`): its own
    /// name, or one derived from what it is.
    pub fn template_display_name(&self, template: &Template, language: &Language) -> String {
        if !template.name.is_empty() {
            return template.name.clone();
        }
        match template.kind {
            TempType::Character | TempType::SpecialBlock => language.asset_name("model", &template.model_name),
            TempType::Block => language.asset_name("block", &template.block_name),
            TempType::Bodypart => language.text(
                "librarybodypartof",
                &[
                    &language.asset_name("modelpart", &template.model_part_name),
                    &language.asset_name("model", &template.model_name),
                ],
            ),
            TempType::Scenery | TempType::Model => {
                let resource = if template.kind == TempType::Scenery { &template.scenery } else { &template.model };
                match resource.as_id().and_then(|id| self.resource(id)) {
                    Some(resource) => file_stem(&resource.filename),
                    None => language.text(&format!("type{}", template.kind.name()), &[]),
                }
            }
            kind => language.text(&format!("type{}", kind.name()), &[]),
        }
    }

    /// Name a timeline is shown with (`tl_update_display_name`): its own
    /// name, or one derived from its type, model part, block or template.
    pub fn timeline_display_name(&self, timeline: &Timeline, language: &Language) -> String {
        if !timeline.name.is_empty() {
            return timeline.name.clone();
        }
        let type_name = || language.text(&format!("type{}", timeline.kind.name()), &[]);
        if !timeline.part_of.is_null() {
            return match timeline.kind {
                TlType::Bodypart if !timeline.model_part_name.is_empty() => {
                    language.asset_name("modelpart", &timeline.model_part_name)
                }
                TlType::Bodypart => language.text("timelineunusedbodypart", &[]),
                TlType::SpecialBlock => match &timeline.part_model {
                    Some((model, _)) if !model.is_empty() => language.asset_name("model", model),
                    _ => type_name(),
                },
                TlType::Block => match &timeline.part_block {
                    Some((block, _)) if !block.is_empty() => language.asset_name("block", block),
                    _ => type_name(),
                },
                _ => type_name(),
            };
        }
        match &timeline.temp {
            ObjRef::Id(id) => match self.template(id) {
                Some(template) => self.template_display_name(template, language),
                None => type_name(),
            },
            _ => type_name(),
        }
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

    /// The rotation point a template gives its timelines
    /// (`temp_update_rot_point`): the middle of the floor of blocks and
    /// scenery, the bottom of shapes. `scenery_size` is the size in blocks
    /// of a loaded scenery resource, in the timeline's axes.
    pub fn template_rot_point(&self, template: &Template, scenery_size: &ScenerySize) -> Vec3 {
        const BLOCK: f64 = 16.0;
        const ITEM_SIZE: f64 = 16.0;
        let repeat = if template.block_repeat_enable { template.block_repeat } else { [1.0; 3] };
        let mut point = [0.0; 3];
        match template.kind {
            TempType::Scenery => {
                if let Some(size) = template.scenery.as_id().and_then(scenery_size) {
                    point[0] = repeat[0] * size[0] * BLOCK / 2.0;
                    point[1] = repeat[1] * size[1] * BLOCK / 2.0;
                }
            }
            TempType::Block => {
                point[0] = repeat[0] * BLOCK / 2.0;
                point[1] = repeat[1] * BLOCK / 2.0;
            }
            TempType::Item => {
                point[0] = ITEM_SIZE / 2.0;
                point[1] = if template.item.is_3d { 0.5 } else { 0.0 };
            }
            TempType::Text => {
                point[1] = if template.text.is_3d { 0.5 } else { 0.0 };
            }
            // Block-format models get the middle of a block too, once
            // models can be loaded.
            _ => {}
        }
        if template.kind.is_shape() {
            point[2] = -8.0;
        }
        point
    }

    /// The rotation point a timeline turns around (`tl_update_rot_point`):
    /// its own when custom or part of a model or scenery, else its
    /// template's.
    pub fn rot_point(&self, timeline: &Timeline, scenery_size: &ScenerySize) -> Vec3 {
        if timeline.rot_point_custom || !timeline.part_of.is_null() {
            return timeline.rot_point;
        }
        match timeline.temp.as_id().and_then(|id| self.template(id)) {
            Some(template) => self.template_rot_point(template, scenery_size),
            None => timeline.rot_point,
        }
    }

    /// The timelines in tree order, as the transform update wants them.
    /// The second list maps each node back to its timeline index.
    pub fn scene_nodes(&self) -> (Vec<SceneNode<'_>>, Vec<usize>) {
        self.scene_nodes_with(&|_| None, &|_| None)
    }

    /// Like [`Project::scene_nodes`], with the model part of each body part
    /// timeline given by `part_of_timeline` (timeline index to part) and
    /// the sizes of loaded scenery.
    pub fn scene_nodes_with(
        &self,
        part_of_timeline: &dyn Fn(usize) -> Option<PartInfo>,
        scenery_size: &ScenerySize,
    ) -> (Vec<SceneNode<'_>>, Vec<usize>) {
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
                    part: part_of_timeline(i),
                    rot_point: self.rot_point(timeline, scenery_size),
                }
            })
            .collect();
        (nodes, order)
    }

    /// Evaluates the whole scene at frame `marker`. `SceneState::nodes` is
    /// in tree order; the returned list gives the timeline index of each.
    pub fn evaluate(&self, marker: f64) -> (SceneState, Vec<usize>) {
        self.evaluate_with(marker, &|_| None, &|_| None)
    }

    /// Like [`Project::evaluate`], with model parts (see
    /// [`Project::scene_nodes_with`]).
    pub fn evaluate_with(
        &self,
        marker: f64,
        part_of_timeline: &dyn Fn(usize) -> Option<PartInfo>,
        scenery_size: &ScenerySize,
    ) -> (SceneState, Vec<usize>) {
        let (nodes, order) = self.scene_nodes_with(part_of_timeline, scenery_size);
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
