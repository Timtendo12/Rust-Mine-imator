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
    /// Scenery of the open project; replaced together with the project.
    scenery: Mutex<Option<SceneryStore>>,
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
        *lock(&self.project) = project;
        *lock(&self.bindings) = bindings;
        *lock(&self.scenery) = scenery;
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
