//! Animation evaluation: what every value of every timeline is at a given
//! frame, and where that puts each object in the world.

pub mod ease;
pub mod evaluate;
pub mod math;
pub mod value_rules;

pub use ease::{BezierHandles, Transition};
pub use evaluate::{evaluate, find_segment, Evaluated, Playhead, Segment};
pub use math::{Mat4, Vec3};
