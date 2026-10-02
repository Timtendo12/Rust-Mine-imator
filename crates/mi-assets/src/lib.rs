//! Minecraft assets: the asset pack that ships with the program, models
//! and their meshes, textures.

pub mod blocks;
pub mod builder;
pub mod item;
pub mod legacy;
mod liquid;
pub mod model_file;
pub mod nbt;
pub mod pack;
pub mod scenery;
pub mod shape_mesh;
pub mod text;

pub use builder::{build_grid, Grid, GridBlock, GridSource};
pub use blocks::{block_mesh, pick_weighted, BlockDef, Blocks, Depth, Dir, PlacedBlock, RandomOffset, RenderModel};
pub use item::{item_mesh, ITEM_SIZE};
pub use legacy::{LegacyBlock, LegacyBlocks};
pub use model_file::{Bend, ModelError, ModelFile, ModelPart, ModelShape, ShapeKind};
pub use pack::{decode_square, AssetPack, ModelDef, PackError, ResolvedModel, Rgba};
pub use scenery::{Scenery, SceneryBlock, SceneryError, SceneryOptions};
pub use text::{text_image, text_mesh, Align, SpriteFont, TextImage};
pub use shape_mesh::{shape_mesh, BendStyle};
