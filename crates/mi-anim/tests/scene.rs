//! Scene transform tests: hierarchy, inheritance, paths and inverse
//! kinematics on hand-built scenes.

use mi_anim::math::{self, Vec3};
use mi_anim::{update_scene, BendInfo, BendPart, PartInfo, Playhead, SceneNode, SceneState};
use mi_core::{Color, ObjRef, SaveId, TlType, Value, ValueId};
use mi_format::project::{Background, Timeline};
use mi_format::ValueSet;

fn defaults() -> ValueSet {
    ValueSet::project_defaults(&Background::default(), 0.0, 1.0)
}

fn timeline(id: &str, kind: TlType) -> Timeline {
    Timeline::new(SaveId::new(id), kind, &defaults())
}

fn set(tl: &mut Timeline, id: ValueId, value: f64) {
    tl.default_values[id] = Value::Number(value);
}

fn place(tl: &mut Timeline, pos: Vec3) {
    set(tl, ValueId::PosX, pos[0]);
    set(tl, ValueId::PosY, pos[1]);
    set(tl, ValueId::PosZ, pos[2]);
}

fn node(timeline: &Timeline, parent: Option<usize>) -> SceneNode<'_> {
    SceneNode { timeline, parent, part_of: None, part: None, rot_point: [0.0; 3] }
}

fn run(nodes: &[SceneNode]) -> SceneState {
    update_scene(nodes, &Playhead::at(0.0))
}

fn close(a: Vec3, b: Vec3) -> bool {
    math::distance(a, b) < 1e-6
}

#[test]
fn child_follows_parent() {
    let mut parent = timeline("P", TlType::Cube);
    place(&mut parent, [10.0, 0.0, 0.0]);
    set(&mut parent, ValueId::RotZ, 90.0);
    let mut child = timeline("C", TlType::Cube);
    place(&mut child, [5.0, 0.0, 0.0]);

    let state = run(&[node(&parent, None), node(&child, Some(0))]);
    assert!(close(state.nodes[0].world_pos, [10.0, 0.0, 0.0]));
    // The parent's +X points to world -Y after a 90 degree Z rotation.
    assert!(close(state.nodes[1].world_pos, [10.0, -5.0, 0.0]), "{:?}", state.nodes[1].world_pos);
    // The child's own X axis is turned with the parent.
    assert!(close(state.nodes[1].matrix.transform_vector([1.0, 0.0, 0.0]), [0.0, -1.0, 0.0]));
}

#[test]
fn position_and_rotation_inheritance_can_be_turned_off() {
    let mut parent = timeline("P", TlType::Cube);
    place(&mut parent, [10.0, 0.0, 0.0]);
    set(&mut parent, ValueId::RotZ, 90.0);

    let mut free = timeline("A", TlType::Cube);
    place(&mut free, [5.0, 0.0, 0.0]);
    free.inherit.position = false;

    let mut upright = timeline("B", TlType::Cube);
    place(&mut upright, [5.0, 0.0, 0.0]);
    upright.inherit.rotation = false;

    let state = run(&[node(&parent, None), node(&free, Some(0)), node(&upright, Some(0))]);
    assert!(close(state.nodes[1].world_pos, [5.0, 0.0, 0.0]));
    // Still positioned through the parent, but not turned.
    assert!(close(state.nodes[2].world_pos, [10.0, -5.0, 0.0]));
    assert!(close(state.nodes[2].matrix.transform_vector([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]));
}

#[test]
fn scale_is_passed_on_without_shearing() {
    let mut parent = timeline("P", TlType::Cube);
    set(&mut parent, ValueId::ScaX, 2.0);
    let mut child = timeline("C", TlType::Cube);
    place(&mut child, [5.0, 0.0, 0.0]);
    set(&mut child, ValueId::RotZ, 90.0);
    set(&mut child, ValueId::ScaY, 3.0);

    let state = run(&[node(&parent, None), node(&child, Some(0))]);
    let child_state = &state.nodes[1];
    // The offset from the parent is stretched by the parent's scale.
    assert!(close(child_state.world_pos, [10.0, 0.0, 0.0]), "{:?}", child_state.world_pos);
    // The child's axes are scaled by parent × own scale and stay perpendicular.
    let x = child_state.matrix.transform_vector([1.0, 0.0, 0.0]);
    let y = child_state.matrix.transform_vector([0.0, 1.0, 0.0]);
    assert!((math::length(x) - 2.0).abs() < 1e-9);
    assert!((math::length(y) - 3.0).abs() < 1e-9);
    assert!(math::dot(x, y).abs() < 1e-9);
    assert_eq!(child_state.inherited.scale, [2.0, 1.0, 1.0]);

    // Without scale inheritance only the own scale remains.
    let mut alone = child.clone();
    alone.inherit.scale = false;
    let state = run(&[node(&parent, None), node(&alone, Some(0))]);
    let x = state.nodes[1].matrix.transform_vector([1.0, 0.0, 0.0]);
    assert!((math::length(x) - 1.0).abs() < 1e-9);
}

#[test]
fn rotation_point_offsets_the_render_matrix() {
    let mut tl = timeline("T", TlType::Cube);
    place(&mut tl, [0.0, 0.0, 8.0]);
    let mut n = node(&tl, None);
    n.rot_point = [8.0, 8.0, 0.0];
    let state = run(&[n]);
    assert!(close(state.nodes[0].matrix.transform_point([0.0; 3]), [0.0, 0.0, 8.0]));
    assert!(close(state.nodes[0].matrix_render.transform_point([8.0, 8.0, 0.0]), [0.0, 0.0, 8.0]));
}

#[test]
fn material_values_and_visibility_are_inherited() {
    let mut parent = timeline("P", TlType::Folder);
    set(&mut parent, ValueId::Alpha, 0.5);
    parent.default_values[ValueId::Visible] = Value::Bool(false);
    parent.default_values[ValueId::RgbAdd] = Value::Color(Color::rgb(10, 20, 30));
    set(&mut parent, ValueId::Emissive, 0.25);
    set(&mut parent, ValueId::Roughness, 0.5);

    let mut child = timeline("C", TlType::Cube);
    set(&mut child, ValueId::Alpha, 0.5);
    child.default_values[ValueId::RgbAdd] = Value::Color(Color::rgb(250, 1, 1));
    set(&mut child, ValueId::Emissive, 0.5);

    // Defaults: visibility and surface are inherited, alpha and colour not.
    let state = run(&[node(&parent, None), node(&child, Some(0))]);
    let inherited = &state.nodes[1].inherited;
    assert_eq!(inherited.alpha, 0.5);
    assert_eq!(inherited.rgb_add, Color::rgb(250, 1, 1));
    assert!(!inherited.visible);
    assert_eq!(inherited.emissive, 0.75);
    assert_eq!(inherited.roughness, 0.5);

    let mut inheriting = child.clone();
    inheriting.inherit.alpha = true;
    inheriting.inherit.color = true;
    inheriting.inherit.visibility = false;
    inheriting.inherit.surface = false;
    let state = run(&[node(&parent, None), node(&inheriting, Some(0))]);
    let inherited = &state.nodes[1].inherited;
    assert_eq!(inherited.alpha, 0.25);
    assert_eq!(inherited.rgb_add, Color::rgb(255, 21, 31));
    assert!(inherited.visible);
    assert_eq!(inherited.emissive, 0.5);
}

#[test]
fn inheritance_stops_at_the_first_timeline_that_opts_out() {
    let mut top = timeline("A", TlType::Folder);
    set(&mut top, ValueId::Alpha, 0.5);
    let mut middle = timeline("B", TlType::Folder);
    set(&mut middle, ValueId::Alpha, 0.5);
    // `middle` does not inherit alpha, so `top` does not reach the leaf.
    let mut leaf = timeline("C", TlType::Cube);
    leaf.inherit.alpha = true;

    let state = run(&[node(&top, None), node(&middle, Some(0)), node(&leaf, Some(1))]);
    assert_eq!(state.nodes[2].inherited.alpha, 0.5);

    middle.inherit.alpha = true;
    let state = run(&[node(&top, None), node(&middle, Some(0)), node(&leaf, Some(1))]);
    assert_eq!(state.nodes[2].inherited.alpha, 0.25);
}

#[test]
fn textures_are_inherited_unless_overridden() {
    let mut parent = timeline("P", TlType::Folder);
    parent.default_values[ValueId::TextureObj] = Value::Ref(ObjRef::id("TEX_PARENT"));
    let mut child = timeline("C", TlType::Cube);
    child.inherit.texture = true;

    let state = run(&[node(&parent, None), node(&child, Some(0))]);
    assert_eq!(state.nodes[1].inherited.texture, ObjRef::id("TEX_PARENT"));

    child.default_values[ValueId::TextureObj] = Value::Ref(ObjRef::id("TEX_OWN"));
    let state = run(&[node(&parent, None), node(&child, Some(0))]);
    assert_eq!(state.nodes[1].inherited.texture, ObjRef::id("TEX_OWN"));
}

#[test]
fn camera_orbits_its_position() {
    let mut camera = timeline("CAM", TlType::Camera);
    place(&mut camera, [100.0, 0.0, 0.0]);
    camera.default_values[ValueId::CamRotate] = Value::Bool(true);
    set(&mut camera, ValueId::CamRotateDistance, 50.0);
    set(&mut camera, ValueId::CamRotateAngleXy, 0.0);
    set(&mut camera, ValueId::CamRotateAngleZ, 0.0);

    let state = run(&[node(&camera, None)]);
    let cam = &state.nodes[0];
    assert_eq!(cam.orbit_center, Some([100.0, 0.0, 0.0]));
    assert!((math::distance(cam.world_pos, [100.0, 0.0, 0.0]) - 50.0).abs() < 1e-9);
    assert!(cam.world_pos[2].abs() < 1e-9);

    // Looking down from above.
    set(&mut camera, ValueId::CamRotateAngleZ, 89.9);
    let state = run(&[node(&camera, None)]);
    assert!(state.nodes[0].world_pos[2] > 49.9);
}

#[test]
fn objects_follow_paths() {
    let mut path = timeline("PATH", TlType::Path);
    place(&mut path, [0.0, 0.0, 100.0]);
    let mut a = timeline("A", TlType::PathPoint);
    place(&mut a, [0.0, 0.0, 0.0]);
    let mut b = timeline("B", TlType::PathPoint);
    place(&mut b, [40.0, 0.0, 0.0]);

    let mut rider = timeline("R", TlType::Cube);
    rider.default_values[ValueId::PathObj] = Value::Ref(ObjRef::id("PATH"));

    let nodes = |rider: &Timeline| -> SceneState {
        run(&[node(&path, None), node(&a, Some(0)), node(&b, Some(0)), node(rider, None)])
    };

    let state = nodes(&rider);
    let table = state.paths[0].as_ref().unwrap();
    assert!(table.length > 37.0 && table.length <= 40.0, "{}", table.length);
    assert!(close(state.nodes[3].world_pos, [0.0, 0.0, 100.0]));

    let half = table.length / 2.0;
    set(&mut rider, ValueId::PathOffset, half);
    let state = nodes(&rider);
    let pos = state.nodes[3].world_pos;
    // Roughly halfway: the table is only approximately evenly spaced.
    assert!(math::distance(pos, [20.0, 0.0, 100.0]) < 3.0, "{pos:?}");
    assert!(pos[1].abs() < 1e-9 && (pos[2] - 100.0).abs() < 1e-9);
    // The rider's +Y points along the path.
    assert!(close(state.nodes[3].matrix.transform_vector([0.0, 1.0, 0.0]), [1.0, 0.0, 0.0]));

    // Past the end it stays at the last point.
    set(&mut rider, ValueId::PathOffset, 500.0);
    let pos = nodes(&rider).nodes[3].world_pos;
    assert!(math::distance(pos, [40.0, 0.0, 100.0]) < 0.2, "{pos:?}");
}

fn leg() -> PartInfo {
    PartInfo {
        position: [0.0, 0.0, 12.0],
        rotation: [0.0; 3],
        bend: Some(BendInfo {
            part: BendPart::Lower,
            axis: [true, false, false],
            offset: -6.0,
            end_offset: 6.0,
            invert: [false; 3],
            direction_min: [0.0; 3],
            direction_max: [180.0, 0.0, 0.0],
        }),
    }
}

#[test]
fn bent_parent_carries_locked_children() {
    let model = timeline("M", TlType::Character);
    let mut limb = timeline("L", TlType::Bodypart);
    set(&mut limb, ValueId::BendAngleX, 90.0);
    let item = timeline("I", TlType::Item);
    let mut loose = timeline("J", TlType::Item);
    loose.lock_bend = false;

    let mut limb_node = node(&limb, Some(0));
    limb_node.part_of = Some(0);
    limb_node.part = Some(leg());
    let state = run(&[node(&model, None), limb_node, node(&item, Some(1)), node(&loose, Some(1))]);

    assert!(close(state.nodes[1].world_pos, [0.0, 0.0, 12.0]));
    assert_eq!(state.nodes[1].inherited.bend, [90.0, 0.0, 0.0]);
    // The locked child sits at the joint and is turned with the lower half.
    assert!(close(state.nodes[2].world_pos, [0.0, 0.0, 6.0]), "{:?}", state.nodes[2].world_pos);
    let down = state.nodes[2].matrix.transform_vector([0.0, 0.0, -1.0]);
    assert!(down[2].abs() < 1e-9 && (math::length(down) - 1.0).abs() < 1e-9, "{down:?}");
    // The unlocked child stays with the upper half.
    assert!(close(state.nodes[3].world_pos, [0.0, 0.0, 12.0]));
    assert!(close(state.nodes[3].matrix.transform_vector([0.0, 0.0, -1.0]), [0.0, 0.0, -1.0]));
}

#[test]
fn inverse_kinematics_reaches_the_target() {
    let model = timeline("M", TlType::Character);
    let mut limb = timeline("L", TlType::Bodypart);
    limb.default_values[ValueId::IkTarget] = Value::Ref(ObjRef::id("TARGET"));
    let mut target = timeline("TARGET", TlType::Folder);
    place(&mut target, [0.0, 5.0, 4.0]);

    let scene = |limb: &Timeline, target: &Timeline| -> SceneState {
        let mut limb_node = node(limb, Some(0));
        limb_node.part_of = Some(0);
        limb_node.part = Some(leg());
        run(&[node(&model, None), limb_node, node(target, None)])
    };

    let state = scene(&limb, &target);
    let limb_state = &state.nodes[1];
    let ik = limb_state.ik.expect("the limb has a target");
    let world = |p: Vec3| limb_state.matrix_pre_ik.transform_point(p);

    // Hip stays put, bone lengths are kept, the foot is on the target.
    assert!(close(world(ik.positions[0]), [0.0, 0.0, 12.0]));
    assert!((math::distance(ik.positions[0], ik.positions[1]) - 6.0).abs() < 1e-6);
    assert!((math::distance(ik.positions[1], ik.positions[2]) - 6.0).abs() < 1e-6);
    assert!(math::distance(world(ik.positions[2]), [0.0, 5.0, 4.0]) < 1e-4, "{:?}", world(ik.positions[2]));
    assert!(ik.bend_angle > 1.0 && ik.bend_angle < 179.0, "{}", ik.bend_angle);
    // The knee angle replaces the keyframed bend.
    assert!((limb_state.inherited.bend[0] - ik.bend_angle).abs() < 1e-9);

    // A target out of reach stretches the limb straight towards it.
    place(&mut target, [0.0, 100.0, 12.0]);
    let state = scene(&limb, &target);
    let limb_state = &state.nodes[1];
    let ik = limb_state.ik.unwrap();
    let foot = limb_state.matrix_pre_ik.transform_point(ik.positions[2]);
    assert!(close(foot, [0.0, 12.0, 12.0]), "{foot:?}");
    assert!(ik.bend_angle.abs() < 1e-4);

    // Without a target there is no IK.
    limb.default_values[ValueId::IkTarget] = Value::Ref(ObjRef::Null);
    assert!(scene(&limb, &target).nodes[1].ik.is_none());
}

#[test]
fn ik_blend_mixes_with_the_resting_pose() {
    let model = timeline("M", TlType::Character);
    let mut limb = timeline("L", TlType::Bodypart);
    limb.default_values[ValueId::IkTarget] = Value::Ref(ObjRef::id("TARGET"));
    set(&mut limb, ValueId::IkBlend, 0.0);
    let mut target = timeline("TARGET", TlType::Folder);
    place(&mut target, [0.0, 5.0, 4.0]);

    let mut limb_node = node(&limb, Some(0));
    limb_node.part_of = Some(0);
    limb_node.part = Some(leg());
    let state = run(&[node(&model, None), limb_node, node(&target, None)]);
    let limb_state = &state.nodes[1];
    let ik = limb_state.ik.unwrap();
    // With no blend the foot stays where the straight leg puts it.
    let foot = limb_state.matrix_pre_ik.transform_point(ik.positions[2]);
    assert!(math::distance(foot, [0.0, 0.0, 0.0]) < 1e-3, "{foot:?}");
}
