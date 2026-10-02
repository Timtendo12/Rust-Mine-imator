//! Application state shared between commands.

use mi_project::Project;
use std::sync::{Mutex, MutexGuard};

/// Everything the application keeps between commands. The frontend never
/// holds project data of its own; it asks for what it shows.
#[derive(Default)]
pub struct AppState {
    project: Mutex<Option<Project>>,
    startup_path: Mutex<Option<String>>,
}

impl AppState {
    /// State for an application started with the given command line
    /// arguments (without the program name). The first argument that is not
    /// an option is taken as a project file to open, which is what the
    /// operating system passes when a project file is double-clicked.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let startup_path = args.into_iter().find(|arg| !arg.starts_with('-'));
        Self { project: Mutex::default(), startup_path: Mutex::new(startup_path) }
    }

    /// The project file given on the command line. Returned once, so that a
    /// reload of the frontend does not reopen it over the current project.
    pub fn take_startup_path(&self) -> Option<String> {
        self.startup_path.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take()
    }

    /// The open project, if any.
    pub fn project(&self) -> MutexGuard<'_, Option<Project>> {
        // A command that panicked while holding the lock cannot leave the
        // project half-updated in a way later commands could not cope with,
        // so the data is used as it is.
        self.project.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
}
