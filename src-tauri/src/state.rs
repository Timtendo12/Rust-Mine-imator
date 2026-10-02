//! Application state shared between commands.

use crate::viewport::{ViewState, ViewportHandle};
use mi_project::Project;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Everything the application keeps between commands. The frontend never
/// holds project data of its own; it asks for what it shows.
#[derive(Default)]
pub struct AppState {
    project: Mutex<Option<Project>>,
    view: Mutex<ViewState>,
    viewport: OnceLock<ViewportHandle>,
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
