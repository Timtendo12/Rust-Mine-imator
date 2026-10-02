//! The project layer: an open project with its timeline tree, lookups by
//! save id, opening and saving, binding to Minecraft models, and evaluation
//! of the scene at a frame.

mod editing;
mod history;
mod models;
mod project;
mod scenery;
mod timeline_ops;
mod tree;

pub use editing::{InfoChange, KeyframeRef, ValueChange};
pub use history::{Edit, History, HISTORY_LIMIT};
pub use models::{ModelBindings, ModelTextures, PartBinding};
pub use project::{Project, ProjectContext, ProjectError, ScenerySize};
pub use scenery::{LoadedScenery, SceneryStore};
pub use timeline_ops::{creatable, CameraPose};
pub use tree::Tree;
