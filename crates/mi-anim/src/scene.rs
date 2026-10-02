//! World transforms of all timelines at one point in time
//! (`tl_update_matrix`, `tl_update_ik`, `app_update_animate`).
//!
//! The input is a flat list of [`SceneNode`]s in tree order (every parent
//! before its children) together with the values evaluated for each. The
//! output is one [`NodeState`] per node. Nothing here knows about the editor
//! or about how models are stored; body parts describe themselves through
//! [`PartInfo`].

use crate::evaluate::{evaluate, Playhead};
use crate::math::{self, Mat4, Vec3, X, Y, Z};
use crate::path::{PathPoint, PathShape, PathTable};
use mi_core::{Color, ObjRef, TlType, Value, ValueId};
use mi_format::project::Timeline;
use mi_format::ValueSet;

/// Which half of a body part bends (`e_part`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendPart {
    Right,
    Left,
    Front,
    Back,
    Upper,
    Lower,
}

/// How a body part bends, from its model definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BendInfo {
    pub part: BendPart,
    /// Axes the part can bend around.
    pub axis: [bool; 3],
    /// Distance of the joint from the part's origin along the bend direction.
    pub offset: f64,
    /// Distance from the joint to the end of the part; above zero for limbs
    /// that support inverse kinematics.
    pub end_offset: f64,
    pub invert: [bool; 3],
    pub direction_min: Vec3,
    pub direction_max: Vec3,
}

impl BendInfo {
    /// `tl_supports_ik`: a limb bending on its lower half around X only.
    pub fn supports_ik(&self) -> bool {
        self.end_offset > 0.0 && self.part == BendPart::Lower && self.axis == [true, false, false]
    }

    /// `model_part_get_bend_matrix` for a timeline: the transform of the
    /// bent half relative to the part.
    pub fn matrix(&self, bend: Vec3) -> Mat4 {
        let mut angles = [0.0; 3];
        for (i, angle) in angles.iter_mut().enumerate() {
            if bend[i] == 0.0 || !self.axis[i] {
                continue;
            }
            *angle = bend[i].clamp(self.direction_min[i], self.direction_max[i]);
            if self.invert[i] {
                *angle = -*angle;
            }
        }
        let mut pos = [0.0; 3];
        match self.part {
            BendPart::Right | BendPart::Left => pos[X] = self.offset,
            BendPart::Front | BendPart::Back => pos[Y] = self.offset,
            BendPart::Upper | BendPart::Lower => pos[Z] = self.offset,
        }
        Mat4::build(pos, angles, [1.0; 3])
    }

    /// Offset from the joint to the end of the limb.
    fn end_vector(&self) -> Vec3 {
        let o = self.end_offset;
        match self.part {
            BendPart::Upper => [0.0, 0.0, o],
            BendPart::Lower => [0.0, 0.0, -o],
            BendPart::Right => [o, 0.0, 0.0],
            BendPart::Left => [-o, 0.0, 0.0],
            BendPart::Front => [0.0, o, 0.0],
            BendPart::Back => [0.0, -o, 0.0],
        }
    }
}

/// Placement of a body part inside its model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartInfo {
    pub position: Vec3,
    pub rotation: Vec3,
    pub bend: Option<BendInfo>,
}

/// One timeline as the transform update sees it.
#[derive(Debug, Clone)]
pub struct SceneNode<'a> {
    pub timeline: &'a Timeline,
    /// Index of the parent node; `None` at the top level.
    pub parent: Option<usize>,
    /// Index of the model this body part belongs to.
    pub part_of: Option<usize>,
    /// Model data when the timeline is a body part with a known model part.
    pub part: Option<PartInfo>,
    /// Rotation point in effect (the template's unless the timeline has its
    /// own; `tl_update_rot_point`).
    pub rot_point: Vec3,
}

/// Values a timeline ends up with after taking its ancestors into account
/// (`value_inherit`).
#[derive(Debug, Clone, PartialEq)]
pub struct Inherited {
    pub alpha: f64,
    pub rgb_add: Color,
    pub rgb_sub: Color,
    pub rgb_mul: Color,
    pub hsb_add: Color,
    pub hsb_sub: Color,
    pub hsb_mul: Color,
    pub mix_color: Color,
    pub glow_color: Color,
    pub mix_percent: f64,
    pub emissive: f64,
    pub metallic: f64,
    pub roughness: f64,
    pub subsurface: f64,
    pub subsurface_radius: Vec3,
    pub subsurface_color: Color,
    pub wind_influence: f64,
    pub visible: bool,
    pub bend: Vec3,
    pub texture: ObjRef,
    pub texture_material: ObjRef,
    pub texture_normal: ObjRef,
    /// Product of the scale of the ancestors that pass their scale on.
    pub scale: Vec3,
}

/// Result of inverse kinematics for one limb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IkJoints {
    /// Joint positions in the limb's own space: origin, knee/elbow, end.
    pub positions: [Vec3; 3],
    /// Orientation of each segment.
    pub matrices: [Mat4; 3],
    pub bone_matrices: [Mat4; 3],
    /// Angle at the middle joint in degrees.
    pub bend_angle: f64,
}

/// Everything known about a timeline at the evaluated frame.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeState {
    /// Animated values at the playhead.
    pub values: ValueSet,
    pub inherited: Inherited,
    /// World transform.
    pub matrix: Mat4,
    /// World transform including the rotation point; what meshes are drawn
    /// with.
    pub matrix_render: Mat4,
    /// Transform the timeline is relative to.
    pub matrix_parent: Mat4,
    /// The timeline's own position, rotation and scale.
    pub matrix_local: Mat4,
    pub world_pos: Vec3,
    /// For cameras orbiting a point: the point.
    pub orbit_center: Option<Vec3>,
    pub ik: Option<IkJoints>,
    /// World transform before inverse kinematics turned the limb; the space
    /// the joint positions in `ik` are expressed in.
    pub matrix_pre_ik: Mat4,
}

/// The state of a scene at one point in time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneState {
    pub nodes: Vec<NodeState>,
    /// Sampled path of every path timeline, by node index.
    pub paths: Vec<Option<PathTable>>,
}

fn vec3(values: &ValueSet, ids: [ValueId; 3]) -> Vec3 {
    [values.number(ids[0]), values.number(ids[1]), values.number(ids[2])]
}

const POS: [ValueId; 3] = [ValueId::PosX, ValueId::PosY, ValueId::PosZ];
const ROT: [ValueId; 3] = [ValueId::RotX, ValueId::RotY, ValueId::RotZ];
const SCA: [ValueId; 3] = [ValueId::ScaX, ValueId::ScaY, ValueId::ScaZ];
const BEND: [ValueId; 3] = [ValueId::BendAngleX, ValueId::BendAngleY, ValueId::BendAngleZ];

fn color(values: &ValueSet, id: ValueId) -> Color {
    values[id].as_color().unwrap_or(Color::BLACK)
}

fn reference(values: &ValueSet, id: ValueId) -> ObjRef {
    values[id].as_ref().cloned().unwrap_or(ObjRef::Null)
}

/// `color_add`
fn color_add(a: Color, b: Color) -> Color {
    Color::rgb(a.r.saturating_add(b.r), a.g.saturating_add(b.g), a.b.saturating_add(b.b))
}

/// `color_multiply`
fn color_multiply(a: Color, b: Color) -> Color {
    let channel = |x: u8, y: u8| ((x as f64 / 255.0) * (y as f64 / 255.0) * 255.0) as u8;
    Color::rgb(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}

struct Pass<'a> {
    nodes: &'a [SceneNode<'a>],
    use_paths: bool,
    /// Second pass for models that copy the pose of their parent model.
    pose: bool,
}

impl Pass<'_> {
    /// Index of the node whose timeline has the given id.
    fn find(&self, target: &ObjRef) -> Option<usize> {
        let id = target.as_id()?;
        self.nodes.iter().position(|n| &n.timeline.id == id)
    }

    /// Child of model `model` that is the body part called `name`
    /// (`tl_part_find`).
    fn find_part(&self, model: usize, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.part_of == Some(model) && n.timeline.model_part_name == name)
    }

    fn compute(&self, index: usize, states: &[NodeState], paths: &[Option<PathTable>]) -> NodeState {
        let node = &self.nodes[index];
        let tl = node.timeline;
        let previous = &states[index];
        let values = &previous.values;
        let parent = node.parent.map(|p| (&self.nodes[p], &states[p]));

        // Parent matrix.
        let mut matrix_parent = match parent {
            Some((parent_node, parent_state)) => {
                let mut m = if tl.inherit.rot_point { parent_state.matrix_render } else { parent_state.matrix };
                // Follow the bent half of the parent body part.
                if parent_node.timeline.kind == TlType::Bodypart && tl.lock_bend {
                    if let Some(bend) = parent_node.part.and_then(|p| p.bend) {
                        m = bend.matrix(parent_state.inherited.bend).then(&m);
                    }
                }
                m
            }
            None => Mat4::IDENTITY,
        };

        // Follow a path.
        if self.use_paths {
            if let Some(path_index) = self.find(&reference(values, ValueId::PathObj)) {
                if let Some(table) = paths[path_index].as_ref().filter(|t| !t.is_empty()) {
                    let on_path = table.transform_at(values.number(ValueId::PathOffset));
                    matrix_parent = matrix_parent.then(&on_path).then(&states[path_index].matrix);
                }
            }
        }

        // Placement of the body part in its model.
        if tl.kind == TlType::Bodypart {
            if let Some(part) = node.part {
                let position = if node.part_of.is_some() { part.position } else { [0.0; 3] };
                matrix_parent = Mat4::build(position, part.rotation, [1.0; 3]).then(&matrix_parent);
            }
        }

        let pos = vec3(values, POS);
        let rot = vec3(values, ROT);
        let own_scale = vec3(values, SCA);
        let matrix_local = Mat4::build(pos, rot, own_scale);
        let mut matrix = matrix_local.then(&matrix_parent);

        let ik = previous.ik.filter(|_| !reference(values, ValueId::IkTarget).is_null());

        if !tl.inherit.rotation {
            matrix.remove_rotation();
            matrix = Mat4::build([0.0; 3], rot, [1.0; 3]).then(&matrix);
        }

        let matrix_pre_ik = matrix;
        if let Some(ik) = &ik {
            matrix = ik.matrices[0].then(&matrix);
        }

        // Copy the pose of the matching part of the parent model.
        let mut pose_bend = [0.0; 3];
        if self.pose {
            let pose_source = node
                .part_of
                .filter(|&model| self.nodes[model].timeline.inherit.pose)
                .and_then(|model| self.nodes[model].parent)
                .and_then(|outer| self.find_part(outer, &tl.model_part_name));
            if let Some(source) = pose_source {
                let source_state = &states[source];
                matrix = source_state.matrix_local.then(&matrix);
                pose_bend = source_state.inherited.bend;
                if let Some(source_ik) = &source_state.ik {
                    matrix = source_ik.matrices[0].then(&matrix);
                }
            }
        }

        // "Resize" scaling: scale applies along the object's own axes and
        // does not shear children.
        if tl.scale_resize || !tl.inherit.scale || tl.kind == TlType::ParticleSpawner {
            let mut sca = own_scale;
            let mut current = index;
            while self.nodes[current].timeline.inherit.scale {
                let Some(p) = self.nodes[current].parent else { break };
                sca = math::mul(sca, vec3(&states[p].values, SCA));
                current = p;
            }

            matrix_parent.remove_scale();
            let mut parent_no_scale = matrix_parent;
            if !tl.inherit.rotation {
                parent_no_scale.remove_rotation();
            }

            let mut no_scale = Mat4::build(pos, rot, [1.0; 3]).then(&parent_no_scale);
            if let Some(ik) = &ik {
                no_scale = ik.matrices[0].then(&no_scale);
            }
            // Take the orientation, keep the translation.
            matrix.0[..11].copy_from_slice(&no_scale.0[..11]);

            let applied = if tl.inherit.scale { sca } else { own_scale };
            matrix = Mat4::scaling(applied).then(&matrix);
        }

        if !tl.inherit.position {
            matrix_parent.remove_rotation();
            matrix.set_position(pos);
        }

        // Cameras orbiting a point sit at a distance from it.
        let mut orbit_center = None;
        if tl.kind == TlType::Camera && values.flag(ValueId::CamRotate) {
            let center = matrix.position();
            orbit_center = Some(center);
            let distance = values.number(ValueId::CamRotateDistance);
            let angle_xy = values.number(ValueId::CamRotateAngleXy) + 90.0;
            let angle_z = values.number(ValueId::CamRotateAngleZ);
            let flat = math::lengthdir_x(1.0, angle_z);
            matrix.set_position([
                center[X] + math::lengthdir_x(distance, angle_xy) * flat,
                center[Y] + math::lengthdir_y(distance, angle_xy) * flat,
                center[Z] + math::lengthdir_z(distance, angle_z),
            ]);
        }

        let world_pos = matrix.position();
        let matrix_render = Mat4::translation(math::scale(node.rot_point, -1.0)).then(&matrix);

        let inherited = self.inherit(index, states, values, &ik, pose_bend);

        NodeState {
            values: values.clone(),
            inherited,
            matrix,
            matrix_render,
            matrix_parent,
            matrix_local,
            world_pos,
            orbit_center,
            ik,
            matrix_pre_ik,
        }
    }

    /// Accumulates material values, visibility and bend up the parent chain.
    fn inherit(
        &self,
        index: usize,
        states: &[NodeState],
        values: &ValueSet,
        ik: &Option<IkJoints>,
        pose_bend: Vec3,
    ) -> Inherited {
        use ValueId::*;

        // Scale shown by the position controls: all ancestors up to the
        // first one that does not inherit scale.
        let mut scale = [1.0; 3];
        let mut current = index;
        while let Some(p) = self.nodes[current].parent {
            scale = math::mul(scale, vec3(&states[p].values, SCA));
            if !self.nodes[p].timeline.inherit.scale {
                break;
            }
            current = p;
        }

        let ik_blend = if ik.is_some() { values.number(IkBlend) } else { 0.0 };
        let mut out = Inherited {
            alpha: values.number(Alpha),
            rgb_add: color(values, RgbAdd),
            rgb_sub: color(values, RgbSub),
            rgb_mul: color(values, RgbMul),
            hsb_add: color(values, HsbAdd),
            hsb_sub: color(values, HsbSub),
            hsb_mul: color(values, HsbMul),
            mix_color: color(values, MixColor),
            glow_color: color(values, GlowColor),
            mix_percent: values.number(MixPercent),
            emissive: values.number(Emissive),
            metallic: values.number(Metallic),
            roughness: values.number(Roughness),
            subsurface: values.number(Subsurface),
            subsurface_radius: vec3(values, [SubsurfaceRadiusRed, SubsurfaceRadiusGreen, SubsurfaceRadiusBlue]),
            subsurface_color: color(values, SubsurfaceColor),
            wind_influence: values.number(WindInfluence),
            visible: values.flag(Visible),
            bend: math::add(math::scale(vec3(values, BEND), 1.0 - ik_blend), pose_bend),
            texture: reference(values, TextureObj),
            texture_material: reference(values, TextureMaterialObj),
            texture_normal: reference(values, TextureNormalObj),
            scale,
        };

        // Each kind of inheritance stops at the first timeline in the chain
        // that turns it off.
        let (mut alpha, mut colors, mut glow, mut visibility) = (true, true, true, true);
        let (mut bend, mut texture, mut surface, mut subsurface) = (true, true, true, true);
        let has_own = |v: &ValueSet, id: ValueId| matches!(v[id], Value::Ref(ObjRef::Id(_)));

        let mut current = index;
        while let Some(p) = self.nodes[current].parent {
            let inherit = &self.nodes[current].timeline.inherit;
            let own = &states[current].values;
            let par = &states[p].values;

            alpha &= inherit.alpha;
            colors &= inherit.color;
            glow &= inherit.glow_color;
            visibility &= inherit.visibility;
            bend &= inherit.bend;
            texture &= inherit.texture && !has_own(own, TextureObj);
            surface &= inherit.surface && !has_own(own, TextureMaterialObj);
            subsurface &= inherit.subsurface;

            if alpha {
                out.alpha *= par.number(Alpha);
            }
            if colors {
                out.rgb_add = color_add(out.rgb_add, color(par, RgbAdd));
                out.rgb_sub = color_add(out.rgb_sub, color(par, RgbSub));
                out.rgb_mul = color_multiply(out.rgb_mul, color(par, RgbMul));
                out.hsb_add = color_add(out.hsb_add, color(par, HsbAdd));
                out.hsb_sub = color_add(out.hsb_sub, color(par, HsbSub));
                out.hsb_mul = color_multiply(out.hsb_mul, color(par, HsbMul));
                out.mix_color = color_add(out.mix_color, color(par, MixColor));
                out.mix_percent = (out.mix_percent + par.number(MixPercent)).clamp(0.0, 1.0);
            }
            if surface {
                out.texture_material = reference(par, TextureMaterialObj);
                out.texture_normal = reference(par, TextureNormalObj);
                out.metallic = (out.metallic + par.number(Metallic)).clamp(0.0, 1.0);
                out.roughness = (out.roughness * par.number(Roughness)).clamp(0.0, 1.0);
                out.emissive += par.number(Emissive);
            }
            if subsurface {
                out.subsurface += par.number(Subsurface);
                let radius = vec3(par, [SubsurfaceRadiusRed, SubsurfaceRadiusGreen, SubsurfaceRadiusBlue]);
                out.subsurface_radius = math::mul(out.subsurface_radius, radius).map(|v| v.clamp(0.0, 1.0));
                out.subsurface_color = color_multiply(out.subsurface_color, color(par, SubsurfaceColor));
            }
            if glow {
                out.glow_color = color_multiply(out.glow_color, color(par, GlowColor));
            }
            if visibility {
                out.visible &= par.flag(Visible);
            }
            if bend {
                out.bend = math::add(out.bend, vec3(par, BEND));
            }
            if texture {
                out.texture = reference(par, TextureObj);
            }
            out.wind_influence *= par.number(WindInfluence);

            current = p;
        }

        if let Some(ik) = ik {
            out.bend[X] += ik.bend_angle * ik_blend;
        }
        out
    }

    fn run(&self, states: &mut [NodeState], paths: &[Option<PathTable>], only: impl Fn(usize) -> bool) {
        for index in 0..self.nodes.len() {
            if !only(index) {
                continue;
            }
            // Paths and their points do not follow paths themselves.
            if self.use_paths && matches!(self.nodes[index].timeline.kind, TlType::Path | TlType::PathPoint) {
                continue;
            }
            states[index] = self.compute(index, states, paths);
        }
    }
}

/// `do_ik`: solves a two-bone limb (FABRIK) so that its end reaches the
/// target, optionally turned towards a pole target.
fn solve_ik(state: &NodeState, bend: &BendInfo, target: Vec3, pole: Option<Vec3>) -> IkJoints {
    let values = &state.values;
    let mat = state.matrix_pre_ik;
    let blend = values.number(ValueId::IkBlend);
    let angle_offset = values.number(ValueId::IkAngleOffset);

    // Nudge the pole target off the limb's axes to keep the angle maths
    // well defined.
    let mut pole = pole;
    if let Some(p) = pole.as_mut() {
        for i in X..=Z {
            if p[i] == state.world_pos[i] {
                p[i] += 0.0001;
            }
            if p[i] == target[i] {
                p[i] += 0.0001;
            }
        }
    }

    // Joints of the straight limb.
    let joint_matrix = bend.matrix([0.0; 3]).then(&mat);
    let mut joints = [
        mat.position(),
        joint_matrix.position(),
        Mat4::translation(bend.end_vector()).then(&joint_matrix).position(),
    ];

    let mut end = [0.0; 3];
    for i in X..=Z {
        end[i] = joints[2][i] + (target[i] - joints[2][i]) * blend;
    }

    let lengths = [math::distance(joints[0], joints[1]), math::distance(joints[1], joints[2])];

    if !bend.invert[X] {
        joints[1] = math::add(joints[0], math::sub(joints[0], joints[1]));
        joints[2] = math::add(joints[0], math::sub(joints[0], joints[2]));
    }

    let mut straight = true;
    if math::distance(joints[0], end) > lengths[0] + lengths[1] {
        // Out of reach: stretch towards the target.
        let dir = math::direction(joints[0], end);
        joints[1] = math::add(joints[0], math::scale(dir, lengths[0]));
        joints[2] = math::add(joints[1], math::scale(dir, lengths[1]));
    } else {
        // A target exactly in line with the limb gives no bend direction.
        let to_end = math::direction(joints[0], end);
        let to_joint = math::direction(joints[0], joints[1]);
        if (0..3).all(|i| to_end[i].abs() == to_joint[i].abs()) {
            let sideways = math::normalize(math::cross(to_end, [-1.0, 0.0, 0.0]));
            end = math::add(end, math::scale(sideways, 0.0001));
        }

        // Up to 30 rounds, stopping once the end of the limb is on the
        // target. (The original tests the distance from the limb's origin,
        // which never changes, so it always runs all rounds.)
        for _ in 0..30 {
            if math::distance(joints[2], end) < 1e-9 {
                break;
            }
            // Backwards: put the end on the target and pull the joint along.
            joints[2] = end;
            joints[1] = math::add(joints[2], math::scale(math::direction(joints[2], joints[1]), lengths[1]));
            // Forwards: restore the bone lengths from the fixed origin.
            joints[1] = math::add(joints[0], math::scale(math::direction(joints[0], joints[1]), lengths[0]));
            joints[2] = math::add(joints[1], math::scale(math::direction(joints[1], joints[2]), lengths[1]));
        }
        straight = false;
    }

    // Work in the limb's own space from here.
    let inverse = mat.inverse();
    let p0 = inverse.transform_point(joints[0]);
    let mut p1 = inverse.transform_point(joints[1]);
    let p2 = inverse.transform_point(joints[2]);
    let pole = pole.map(|p| inverse.transform_point(p));

    let mut ab = math::direction(p0, p1);
    let mut bc = math::direction(p1, p2);
    let ac = math::direction(p0, p2);

    // Swing the middle joint towards the pole target.
    if let (Some(pole), false) = (pole, straight) {
        let pole_proj = math::project_plane(pole, p0, ac);
        let p1_proj = math::project_plane(p1, p0, ac);
        let angle = math::angle_signed(math::sub(p1_proj, p0), math::sub(pole_proj, p0), ac);
        p1 = math::add(p0, math::rotate_axis_angle(math::sub(p1, p0), ac, (angle + angle_offset).to_radians()));
        ab = math::direction(p0, p1);
        bc = math::direction(p1, p2);
    }

    // Front-facing direction of the first bone.
    let mut n = match (pole, straight) {
        (None, true) => {
            if ab[X] == 0.0 && ab[Y] == 0.0 {
                math::normal(ab, -90.0)
            } else {
                math::normal(ab, 0.0)
            }
        }
        (pole, straight) => {
            let reference = match (pole, straight) {
                (Some(pole), true) => pole,
                _ => p2,
            };
            let mut n = math::direction(math::project_plane(reference, p0, ab), p0);
            let pole_and_straight = pole.is_some() && straight;
            if pole_and_straight {
                n = math::rotate_axis_angle(n, ac, angle_offset.to_radians());
            }
            if bend.invert[X] {
                n = math::scale(n, -1.0);
            }
            if pole_and_straight {
                n = math::scale(n, -1.0);
            }
            n
        }
    };

    let first = (Mat4::rotate_to(n, math::scale(ab, -1.0)), Mat4::rotate_to(n, ab));

    if !straight {
        n = math::direction(p1, math::project_plane(p0, p1, bc));
    }
    let second = (Mat4::rotate_to(n, math::scale(bc, -1.0)), Mat4::rotate_to(n, bc));

    IkJoints {
        positions: [p0, p1, p2],
        matrices: [first.0, second.0, second.0],
        bone_matrices: [first.1, second.1, second.1],
        bend_angle: math::dot(ab, bc).clamp(-1.0, 1.0).acos().to_degrees(),
    }
}

/// Builds the sampled path of every path timeline from its path point
/// children.
fn build_paths(nodes: &[SceneNode], states: &[NodeState]) -> Vec<Option<PathTable>> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            if node.timeline.kind != TlType::Path {
                return None;
            }
            let points: Vec<PathPoint> = nodes
                .iter()
                .enumerate()
                .filter(|(_, child)| child.parent == Some(index) && child.timeline.kind == TlType::PathPoint)
                .map(|(child, _)| {
                    let v = &states[child].values;
                    PathPoint::new(vec3(v, POS), v.number(ValueId::PathPointAngle), v.number(ValueId::PathPointScale))
                })
                .collect();
            let settings = &node.timeline.path;
            let shape = PathShape {
                closed: settings.closed,
                smooth: settings.smooth,
                detail: settings.detail.max(0.0) as usize,
            };
            Some(PathTable::build(&points, shape))
        })
        .collect()
}

/// Evaluates every timeline at the playhead and computes its transform.
///
/// `nodes` must list every parent before its children.
pub fn update_scene(nodes: &[SceneNode], playhead: &Playhead) -> SceneState {
    let identity = |values: ValueSet| NodeState {
        inherited: Inherited {
            alpha: 1.0,
            rgb_add: Color::BLACK,
            rgb_sub: Color::BLACK,
            rgb_mul: Color::WHITE,
            hsb_add: Color::BLACK,
            hsb_sub: Color::BLACK,
            hsb_mul: Color::WHITE,
            mix_color: Color::BLACK,
            glow_color: Color::WHITE,
            mix_percent: 0.0,
            emissive: 0.0,
            metallic: 0.0,
            roughness: 1.0,
            subsurface: 0.0,
            subsurface_radius: [1.0; 3],
            subsurface_color: Color::WHITE,
            wind_influence: 1.0,
            visible: true,
            bend: [0.0; 3],
            texture: ObjRef::Null,
            texture_material: ObjRef::Null,
            texture_normal: ObjRef::Null,
            scale: [1.0; 3],
        },
        values,
        matrix: Mat4::IDENTITY,
        matrix_render: Mat4::IDENTITY,
        matrix_parent: Mat4::IDENTITY,
        matrix_local: Mat4::IDENTITY,
        world_pos: [0.0; 3],
        orbit_center: None,
        ik: None,
        matrix_pre_ik: Mat4::IDENTITY,
    };

    let mut states: Vec<NodeState> = nodes
        .iter()
        .map(|node| {
            let has_bend = node.part.is_some_and(|p| p.bend.is_some());
            identity(evaluate(node.timeline, playhead, has_bend).values)
        })
        .collect();

    let no_paths = vec![None; nodes.len()];
    let all = |_: usize| true;

    // 1. Plain hierarchy, which also places paths and their points.
    Pass { nodes, use_paths: false, pose: false }.run(&mut states, &no_paths, all);

    // 2. Paths, then everything again with path following.
    let paths = build_paths(nodes, &states);
    let follows_path = states.iter().any(|s| !reference(&s.values, ValueId::PathObj).is_null());
    let with_paths = Pass { nodes, use_paths: follows_path, pose: false };
    if follows_path {
        with_paths.run(&mut states, &paths, all);
    }

    // 3. Inverse kinematics: solve, re-run the hierarchy, and once more so
    //    that limbs whose targets moved with another limb settle.
    let limbs: Vec<(usize, BendInfo)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.timeline.kind == TlType::Bodypart)
        .filter_map(|(i, n)| n.part.and_then(|p| p.bend).filter(BendInfo::supports_ik).map(|b| (i, b)))
        .collect();
    if !limbs.is_empty() {
        for _ in 0..2 {
            let mut changed = false;
            for (index, bend) in &limbs {
                let state = &states[*index];
                let target = with_paths.find(&reference(&state.values, ValueId::IkTarget));
                let solved = target.map(|target| {
                    let pole = with_paths
                        .find(&reference(&state.values, ValueId::IkTargetAngle))
                        .map(|pole| states[pole].world_pos);
                    solve_ik(state, bend, states[target].world_pos, pole)
                });
                if solved != state.ik {
                    changed = true;
                    states[*index].ik = solved;
                }
            }
            if !changed {
                break;
            }
            with_paths.run(&mut states, &paths, all);
        }
    }

    // 4. Models that copy the pose of the model they are attached to.
    let poses: Vec<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].timeline.inherit.pose && nodes[i].parent.is_some())
        .collect();
    if !poses.is_empty() {
        let posed = |i: usize| {
            // The model itself and everything below it.
            let mut current = Some(i);
            while let Some(c) = current {
                if poses.contains(&c) {
                    return true;
                }
                current = nodes[c].parent;
            }
            false
        };
        Pass { nodes, use_paths: follows_path, pose: true }.run(&mut states, &paths, posed);
    }

    SceneState { nodes: states, paths }
}
