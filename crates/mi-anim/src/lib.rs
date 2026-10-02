//! Animation evaluation: what every value of every timeline is at a given
//! frame, and where that puts each object in the world.

pub mod ease;
pub mod evaluate;
pub mod math;
pub mod noise;
pub mod path;
pub mod scene;
pub mod value_rules;

pub use ease::{BezierHandles, Transition};
pub use evaluate::{evaluate, find_segment, Evaluated, Playhead, Segment};
pub use math::{Mat4, Vec3};
pub use noise::camera_shake;
pub use scene::{update_scene, BendInfo, BendPart, NodeState, PartInfo, SceneNode, SceneState};
