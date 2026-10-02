//! Minecraft assets: the asset pack that ships with the program, models
//! and their meshes, textures.

pub mod model_file;
pub mod pack;
pub mod shape_mesh;

pub use model_file::{Bend, ModelError, ModelFile, ModelPart, ModelShape, ShapeKind};
pub use pack::{decode_square, AssetPack, ModelDef, PackError, ResolvedModel, Rgba};
pub use shape_mesh::{shape_mesh, BendStyle};
