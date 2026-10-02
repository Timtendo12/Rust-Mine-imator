//! Readers and writers for Mine-imator file formats.
//!
//! The goal is that files written by the original program load here with
//! the same meaning, and that files written here load in the original.
//! See `docs/formats.md` for a description of each format.

pub mod json;
pub mod language;
pub mod project;
pub mod recent;
mod record;
pub mod values;

pub use record::StateValue;
pub use values::ValueSet;

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("could not parse the file: {0}")]
    Json(#[from] json::JsonError),
    #[error("the file is corrupted: {0}")]
    Corrupted(String),
    #[error("the file was saved by a newer version (format {format}, this program reads up to {supported})")]
    TooNew { format: i32, supported: i32 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
