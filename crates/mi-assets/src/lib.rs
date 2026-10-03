//! Minecraft assets: the asset pack that ships with the program, models
//! and their meshes, textures.

pub mod animation;
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
pub mod skin;
pub mod sky;
pub mod text;

pub use animation::{sheet_frame, TextureAnimation, SHEET_FRAMES};
pub use builder::{build_grid, Grid, GridBlock, GridSource};
pub use blocks::{block_mesh, pick_weighted, BlockDef, Blocks, Depth, Dir, PlacedBlock, RandomOffset, RenderModel};
pub use item::{item_mesh, ITEM_SIZE};
pub use legacy::{LegacyBlock, LegacyBlocks};
pub use model_file::{Bend, ModelError, ModelFile, ModelPart, ModelShape, ShapeKind};
pub use pack::{decode_square, AssetPack, ModelDef, PackError, ResolvedModel, Rgba};
pub use scenery::{Scenery, SceneryBlock, SceneryError, SceneryOptions};
pub use text::{text_image, text_mesh, Align, SpriteFont, TextImage};
pub use shape_mesh::{shape_mesh, BendStyle};
pub use skin::player_skin;
pub use sky::{cloud_positions, clouds_mesh, moon_phase, CloudMode};
