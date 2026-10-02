//! The controls drawn over the selected timeline in the viewport
//! (`view_control_move`, `view_control_rotate`): arrows along the axes its
//! position is given in, and a ring per rotation axis. The backend works
//! out where they are on screen; the frontend draws them and turns drags
//! into value changes.

use mi_anim::math::{self, Mat4};
use mi_anim::NodeState;
use mi_core::{TlType, ValueId, ValueType};
use mi_render::Camera;
use serde::Serialize;

/// Size of the controls relative to the distance from the camera
/// (`view_3d_control_size`).
const CONTROL_SIZE: f64 = 0.2125;

/// Segments of a rotation ring.
const RING_DETAIL: usize = 64;

type Point = [f64; 2];

/// An arrow for moving along one axis.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveAxis {
    /// The value it changes, such as `POS_X`.
    pub value: &'static str,
    pub start: Point,
    pub end: Point,
    /// World units the arrow is long: dragging the mouse along the whole
    /// arrow on screen moves the object this far.
    pub length: f64,
    /// Scale of the parent along this axis: a move of one world unit is
    /// this much less in the value.
    pub scale: f64,
}

/// A ring for turning around one axis.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateAxis {
    /// The value it changes, such as `ROT_Z`.
    pub value: &'static str,
    /// The ring as a closed line.
    pub points: Vec<Point>,
    /// Whether the ring is seen from behind, which reverses the direction
    /// the mouse turns it in.
    pub flip: bool,
}

/// The controls of a timeline, in physical pixels from the top left corner
/// of the viewport.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gizmo {
    pub center: Option<Point>,
    #[serde(rename = "move")]
    pub move_axes: Vec<MoveAxis>,
    pub rotate: Vec<RotateAxis>,
}

/// Projects a world position to the viewport; `None` behind the camera.
fn project(view_proj: &glam::Mat4, width: f64, height: f64, p: [f64; 3]) -> Option<Point> {
    let clip = *view_proj * glam::Vec4::new(p[0] as f32, p[1] as f32, p[2] as f32, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let (x, y) = ((clip.x / clip.w) as f64, (clip.y / clip.w) as f64);
    Some([(x * 0.5 + 0.5) * width, (1.0 - (y * 0.5 + 0.5)) * height])
}

/// The controls of a timeline of type `kind` in state `node`, seen through
/// `camera` in a viewport of `width` × `height` pixels.
pub fn gizmo(kind: TlType, node: &NodeState, camera: &Camera, width: f64, height: f64) -> Gizmo {
    if width < 1.0 || height < 1.0 {
        return Gizmo::default();
    }
    let types = kind.value_types(false);
    let view_proj = camera.view_projection((width / height) as f32);
    let screen = |p: [f64; 3]| project(&view_proj, width, height, p);
    let cam_from = [camera.from.x as f64, camera.from.y as f64, camera.from.z as f64];

    // The parent's axes at the timeline's position, without scaling.
    let parent_scale = node.matrix_parent.axis_scale();
    let mut frame: Mat4 = node.matrix_parent;
    frame.set_position(node.matrix.position());
    frame.remove_scale();
    let origin = frame.position();
    let reach = math::distance(cam_from, node.world_pos) * CONTROL_SIZE;

    let mut out = Gizmo { center: screen(node.world_pos), ..Default::default() };
    if out.center.is_none() {
        return out;
    }

    if types.has(ValueType::TransformPos) {
        let (start, end) = (reach / 7.0, reach);
        for (axis, value) in [ValueId::PosX, ValueId::PosY, ValueId::PosZ].into_iter().enumerate() {
            let along = |d: f64| {
                let mut p = [0.0; 3];
                p[axis] = d;
                frame.transform_point(p)
            };
            if let (Some(a), Some(b)) = (screen(along(start)), screen(along(end))) {
                let scale = if parent_scale[axis].abs() > 1e-9 { parent_scale[axis] } else { 1.0 };
                out.move_axes.push(MoveAxis { value: value.name(), start: a, end: b, length: end, scale });
            }
        }
    }

    if types.has(ValueType::TransformRot) {
        let radius = reach * 0.6;
        let rot = |id: ValueId| node.values.number(id);
        let z_ring = frame;
        let x_ring = Mat4::build([0.0; 3], [0.0, -90.0, rot(ValueId::RotZ)], [1.0; 3]).then(&frame);
        let y_ring = Mat4::build([0.0; 3], [rot(ValueId::RotX) + 90.0, 0.0, rot(ValueId::RotZ)], [1.0; 3]).then(&frame);
        for (value, ring) in [(ValueId::RotX, x_ring), (ValueId::RotY, y_ring), (ValueId::RotZ, z_ring)] {
            let points: Option<Vec<Point>> = (0..=RING_DETAIL)
                .map(|i| {
                    let angle = std::f64::consts::TAU * i as f64 / RING_DETAIL as f64;
                    screen(ring.transform_point([angle.cos() * radius, angle.sin() * radius, 0.0]))
                })
                .collect();
            let Some(points) = points else { continue };
            let face = math::normalize(ring.transform_vector([0.0, 0.0, 1.0]));
            let to_camera = math::normalize(math::sub(cam_from, origin));
            out.rotate.push(RotateAxis { value: value.name(), points, flip: math::dot(face, to_camera) < 0.0 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_anim::update_scene;
    use mi_core::SaveId;
    use mi_format::project::{ProjectFile, Timeline};

    fn node_at(kind: TlType, position: [f64; 3]) -> NodeState {
        let file = ProjectFile::new(0.0, 1.0);
        let mut timeline = Timeline::new(SaveId::new("TL"), kind, &file.defaults);
        timeline.default_values[ValueId::PosX] = mi_core::Value::Number(position[0]);
        timeline.default_values[ValueId::PosY] = mi_core::Value::Number(position[1]);
        timeline.default_values[ValueId::PosZ] = mi_core::Value::Number(position[2]);
        let nodes = [mi_anim::SceneNode { timeline: &timeline, parent: None, part_of: None, part: None, rot_point: [0.0; 3] }];
        let playhead = mi_anim::Playhead { marker: 0.0, seamless_repeat: false, region: None, length: 0.0 };
        update_scene(&nodes, &playhead).nodes.remove(0)
    }

    /// Looking along +Y at the origin from 100 units away.
    fn camera() -> Camera {
        Camera {
            from: glam::Vec3::new(0.0, -100.0, 0.0),
            to: glam::Vec3::ZERO,
            up: glam::Vec3::Z,
            fov: 45.0,
            near: 1.0,
            far: 1000.0,
        }
    }

    #[test]
    fn arrows_point_along_the_axes_on_screen() {
        let g = gizmo(TlType::Cube, &node_at(TlType::Cube, [0.0; 3]), &camera(), 400.0, 400.0);
        let center = g.center.unwrap();
        assert!((center[0] - 200.0).abs() < 1e-3 && (center[1] - 200.0).abs() < 1e-3);
        assert_eq!(g.move_axes.iter().map(|a| a.value).collect::<Vec<_>>(), ["POS_X", "POS_Y", "POS_Z"]);
        let x = &g.move_axes[0];
        let z = &g.move_axes[2];
        // The world is left-handed: +X is to the left. +Z is up, which is
        // towards smaller y on screen.
        assert!(x.end[0] < x.start[0] && x.start[0] < 200.0, "{x:?}");
        assert!((x.end[1] - 200.0).abs() < 1e-3);
        assert!(z.end[1] < z.start[1] && z.start[1] < 200.0, "{z:?}");
        // The arrow is as long as the control is big at this distance.
        assert!((x.length - 100.0 * CONTROL_SIZE).abs() < 1e-9 && x.scale == 1.0);
        // Far away the control keeps its size on screen.
        let far = gizmo(TlType::Cube, &node_at(TlType::Cube, [0.0, 400.0, 0.0]), &camera(), 400.0, 400.0);
        let screen_length = |a: &MoveAxis| ((a.end[0] - a.start[0]).powi(2) + (a.end[1] - a.start[1]).powi(2)).sqrt();
        assert!((screen_length(&far.move_axes[0]) - screen_length(x)).abs() < 1e-3);
    }

    #[test]
    fn rings_and_types() {
        let g = gizmo(TlType::Cube, &node_at(TlType::Cube, [0.0; 3]), &camera(), 400.0, 400.0);
        assert_eq!(g.rotate.iter().map(|r| r.value).collect::<Vec<_>>(), ["ROT_X", "ROT_Y", "ROT_Z"]);
        assert!(g.rotate.iter().all(|r| r.points.len() == RING_DETAIL + 1));
        // Seen from the front the Z ring is edge-on: a horizontal line.
        let z = &g.rotate[2];
        assert!(z.points.iter().all(|p| (p[1] - 200.0).abs() < 1e-3));
        // Point lights do not rotate; folders have no controls of their own
        // beyond moving.
        assert!(gizmo(TlType::PointLight, &node_at(TlType::PointLight, [0.0; 3]), &camera(), 400.0, 400.0).rotate.is_empty());
        // Behind the camera nothing is shown.
        let behind = gizmo(TlType::Cube, &node_at(TlType::Cube, [0.0, -500.0, 0.0]), &camera(), 400.0, 400.0);
        assert!(behind.center.is_none() && behind.move_axes.is_empty());
    }
}
