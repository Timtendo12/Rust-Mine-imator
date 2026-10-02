//! The project layer: an open project with its timeline tree, lookups by
//! save id, opening and saving, binding to Minecraft models, and evaluation
//! of the scene at a frame.

mod models;
mod project;
mod tree;

pub use models::{ModelBindings, ModelTextures, PartBinding};
pub use project::{Project, ProjectContext, ProjectError};
pub use tree::Tree;
