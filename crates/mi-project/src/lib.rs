//! The project layer: an open project with its timeline tree, lookups by
//! save id, opening and saving, and evaluation of the scene at a frame.

mod project;
mod tree;

pub use project::{Project, ProjectContext, ProjectError};
pub use tree::Tree;
