//! The project layer: an open project with its timeline tree, lookups by
//! save id, opening and saving, binding to Minecraft models, and evaluation
//! of the scene at a frame.

mod editing;
mod history;
mod keyframes;
mod markers;
mod models;
mod project;
mod scenery;
mod sounds;
mod timeline_ops;
mod tree;

pub use editing::{InfoChange, KeyframeRef, TimelineSetting, ValueChange, TIMELINE_FLAGS};
pub use keyframes::KeyframeClipboard;
pub use markers::{MarkerChange, Repeat, MARKER_COLORS};
pub use history::{Edit, History, HISTORY_LIMIT};
pub use models::{ModelBindings, ModelTextures, PartBinding};
pub use project::{Project, ProjectContext, ProjectError, ScenerySize};
pub use sounds::PlacedSound;
pub use scenery::{LoadedScenery, SceneryStore};
pub use timeline_ops::{creatable, CameraPose};
pub use tree::Tree;
