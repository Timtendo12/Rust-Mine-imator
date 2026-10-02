//! Shared primitives for the Mine-imator rewrite.
//!
//! Everything here mirrors tables and conventions of the original GML code
//! (`enums.gml`, `macros.gml`, `app_startup_lists.gml`, `tl_value_*.gml`) so
//! that the other crates agree on names, ordering and defaults.

pub mod color;
pub mod ids;
pub mod types;
pub mod value;
pub mod version;

pub use color::Color;
pub use ids::{IdGenerator, ObjRef, SaveId};
pub use types::{AlphaMode, GlintMode, MaterialFormat, ResType, TempType, TlType, Tonemapper, ValueType};
pub use value::{Value, ValueId, ValueKind, VALUE_COUNT};
