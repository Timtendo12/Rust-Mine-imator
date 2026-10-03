//! Application state shared between commands.

use crate::viewport::{ViewState, ViewportHandle};
use mi_format::language::Language;
use mi_assets::{AssetPack, LegacyBlocks};
use mi_project::{ModelBindings, Project, SceneryStore};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Everything the application keeps between commands. The frontend never
/// holds project data of its own; it asks for what it shows.
#[derive(Default)]
pub struct AppState {
    project: Mutex<Option<Project>>,
    view: Mutex<ViewState>,
    viewport: OnceLock<ViewportHandle>,
    language: OnceLock<Language>,
    pack: OnceLock<AssetPack>,
    /// Models of the open project; replaced together with the project.
    bindings: Mutex<Option<ModelBindings>>,
    /// Numeric block ids of old schematics.
    legacy: OnceLock<LegacyBlocks>,
    /// The Minecraft font text objects are drawn with.
    font: OnceLock<mi_assets::SpriteFont>,
    /// Scenery of the open project; replaced together with the project.
    scenery: Mutex<Option<SceneryStore>>,
    /// Timelines selected in the editor, which the viewport outlines.
    selection: Mutex<Vec<mi_core::SaveId>>,
    /// Copied keyframes; kept when another project is opened, as in the original.
    clipboard: Mutex<mi_project::KeyframeClipboard>,
    /// Decoded sounds of the open project.
    sounds: Mutex<crate::audio::Sounds>,
    /// Plays the sounds along with the animation.
    player: crate::audio::Player,
    /// Set to stop the export that is running.
    cancel_export: std::sync::atomic::AtomicBool,
    startup_path: Mutex<Option<String>>,
}

/// Locks a mutex, using the data as it is if a command panicked while
/// holding the lock: nothing kept here can be left half-updated in a way
/// later commands could not cope with.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppState {
    /// State for an application started with the given command line
    /// arguments (without the program name). The first argument that is not
    /// an option is taken as a project file to open, which is what the
    /// operating system passes when a project file is double-clicked.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let startup_path = args.into_iter().find(|arg| !arg.starts_with('-'));
        Self { startup_path: Mutex::new(startup_path), ..Default::default() }
    }

    /// The project file given on the command line. Returned once, so that a
    /// reload of the frontend does not reopen it over the current project.
    pub fn take_startup_path(&self) -> Option<String> {
        lock(&self.startup_path).take()
    }

    /// The open project, if any.
    pub fn project(&self) -> MutexGuard<'_, Option<Project>> {
        lock(&self.project)
    }

    /// What the viewport currently shows.
    pub fn view(&self) -> ViewState {
        *lock(&self.view)
    }

    /// Changes what the viewport shows and redraws it.
    pub fn update_view(&self, change: impl FnOnce(&mut ViewState)) {
        change(&mut lock(&self.view));
        self.redraw();
    }

    /// The interface texts. Empty until loaded at startup, in which case
    /// names fall back to identifiers.
    pub fn language(&self) -> &Language {
        static EMPTY: OnceLock<Language> = OnceLock::new();
        self.language.get().unwrap_or_else(|| EMPTY.get_or_init(Language::default))
    }

    pub fn set_language(&self, language: Language) {
        let _ = self.language.set(language);
    }

    /// The Minecraft asset pack, once loaded.
    pub fn pack(&self) -> Option<&AssetPack> {
        self.pack.get()
    }

    pub fn set_pack(&self, pack: AssetPack) {
        let _ = self.pack.set(pack);
    }

    pub fn bindings(&self) -> MutexGuard<'_, Option<ModelBindings>> {
        lock(&self.bindings)
    }

    pub fn font(&self) -> Option<&mi_assets::SpriteFont> {
        self.font.get()
    }

    pub fn set_font(&self, font: mi_assets::SpriteFont) {
        let _ = self.font.set(font);
    }

    pub fn set_legacy(&self, legacy: LegacyBlocks) {
        let _ = self.legacy.set(legacy);
    }

    /// Scenery of the open project. Lock after [`AppState::bindings`] when
    /// both are needed.
    pub fn scenery(&self) -> MutexGuard<'_, Option<SceneryStore>> {
        lock(&self.scenery)
    }

    /// Replaces the open project (or closes it), binds its models and
    /// reads its scenery.
    pub fn set_project(&self, project: Option<Project>) {
        let bindings = match (&project, self.pack()) {
            (Some(project), Some(pack)) => Some(ModelBindings::bind(project, pack)),
            _ => None,
        };
        let scenery = match (&project, self.pack()) {
            (Some(project), Some(pack)) => {
                let empty = LegacyBlocks::empty();
                let store = SceneryStore::load(project, pack, self.legacy.get().unwrap_or(&empty));
                for (id, error) in &store.errors {
                    eprintln!("Could not load scenery {id}: {error}");
                }
                Some(store)
            }
            _ => None,
        };
        self.player.stop();
        *lock(&self.project) = project;
        *lock(&self.bindings) = bindings;
        *lock(&self.scenery) = scenery;
        *lock(&self.sounds) = Default::default();
        self.load_new_sounds();
    }

    /// Decodes the sounds of resources that were added to the open project.
    /// Returns what could not be decoded.
    pub fn load_new_sounds(&self) -> Vec<String> {
        let mut guard = lock(&self.project);
        let Some(project) = guard.as_mut() else { return Vec::new() };
        let errors = lock(&self.sounds).load_missing(project);
        for error in &errors {
            eprintln!("Could not load sound {error}");
        }
        errors
    }

    pub fn sounds(&self) -> MutexGuard<'_, crate::audio::Sounds> {
        lock(&self.sounds)
    }

    pub fn player(&self) -> &crate::audio::Player {
        &self.player
    }

    /// Binds models again after timelines were added, removed or
    /// reordered: bindings are by timeline position. Scenery is by
    /// resource and stays.
    pub fn refresh_project_assets(&self) {
        let bindings = match (lock(&self.project).as_ref(), self.pack()) {
            (Some(project), Some(pack)) => Some(ModelBindings::bind(project, pack)),
            _ => None,
        };
        *lock(&self.bindings) = bindings;
    }

    /// Reads the scenery of resources that were added to the open project.
    pub fn load_new_scenery(&self) {
        let empty = LegacyBlocks::empty();
        let guard = lock(&self.project);
        let (Some(project), Some(pack)) = (guard.as_ref(), self.pack()) else { return };
        let mut scenery = lock(&self.scenery);
        let store = scenery.get_or_insert_with(SceneryStore::default);
        store.errors.clear();
        store.load_missing(project, pack, self.legacy.get().unwrap_or(&empty));
        for (id, error) in &store.errors {
            eprintln!("Could not load scenery {id}: {error}");
        }
    }

    /// The numeric block ids of old schematics.
    pub fn legacy(&self) -> Option<&LegacyBlocks> {
        self.legacy.get()
    }

    /// Evaluates the scene of `project` (the open one, which the caller has
    /// locked) with its bound models and loaded scenery, as the viewport
    /// does: bends and the rotation points of scenery depend on them.
    pub fn evaluate(&self, project: &Project, marker: f64) -> (mi_anim::SceneState, Vec<usize>) {
        let bindings = self.bindings();
        let scenery = self.scenery();
        project.evaluate_with(
            marker,
            &|i| bindings.as_ref().and_then(|b| b.part_info(i)),
            &|resource| scenery.as_ref().and_then(|s| s.get(resource)).map(|s| s.size()),
        )
    }

    /// The selected timelines.
    pub fn selection(&self) -> Vec<mi_core::SaveId> {
        lock(&self.selection).clone()
    }

    pub fn set_selection(&self, selection: Vec<mi_core::SaveId>) {
        *lock(&self.selection) = selection;
        self.redraw();
    }

    /// The copied keyframes.
    pub fn clipboard(&self) -> MutexGuard<'_, mi_project::KeyframeClipboard> {
        lock(&self.clipboard)
    }

    /// The flag that stops a running export.
    pub fn cancel_export(&self) -> &std::sync::atomic::AtomicBool {
        &self.cancel_export
    }

    /// The viewport, once it runs.
    pub fn viewport(&self) -> Option<&ViewportHandle> {
        self.viewport.get()
    }

    /// Registers the viewport once it has been created.
    pub fn set_viewport(&self, handle: ViewportHandle) {
        // Set once at startup; a second call would be a programming error
        // and is ignored.
        let _ = self.viewport.set(handle);
    }

    /// Redraws the viewport, if there is one.
    pub fn redraw(&self) {
        if let Some(viewport) = self.viewport.get() {
            viewport.redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_path_is_the_first_non_option_argument_and_is_returned_once() {
        let state = AppState::from_args(["--flag".to_owned(), "C:/a b/scene.miproject".to_owned(), "x".to_owned()]);
        assert_eq!(state.take_startup_path().as_deref(), Some("C:/a b/scene.miproject"));
        assert_eq!(state.take_startup_path(), None);
        assert_eq!(AppState::from_args(Vec::new()).take_startup_path(), None);
    }

    #[test]
    fn view_changes_are_kept_without_a_viewport() {
        let state = AppState::default();
        state.update_view(|view| view.marker = 12.0);
        assert_eq!(state.view().marker, 12.0);
        assert!(state.viewport.get().is_none());
    }
}
