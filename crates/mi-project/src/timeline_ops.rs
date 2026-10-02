//! Adding, removing, duplicating and moving timelines in the tree
//! (`action_bench_create`, `action_tl_remove`, `tl_remove_clean`,
//! `action_tl_duplicate`, `tl_duplicate`, `action_tl_parent`).

use crate::history::Edit;
use crate::Project;
use mi_assets::{ModelFile, ModelPart};
use mi_core::{ObjRef, SaveId, TempType, TlType, Value, ValueId, ValueKind};
use mi_format::project::{Template, Timeline};
use mi_format::StateValue;
use std::collections::{HashMap, HashSet};

/// Where the work camera is, for new cameras to start from
/// (`tl_value_spawn`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub position: [f64; 3],
    /// Horizontal and vertical look angles and roll, in degrees.
    pub look_xy: f64,
    pub look_z: f64,
    pub roll: f64,
}

/// Kinds of timeline the editor can create on their own.
pub fn creatable(kind: TlType) -> bool {
    kind.is_shape() || matches!(kind, TlType::Folder | TlType::Camera | TlType::PointLight | TlType::SpotLight)
}

/// The children of a timeline (or of the root, for `None`) in tree order.
fn children_of(project: &Project, parent: Option<&SaveId>) -> Vec<SaveId> {
    let tree = project.tree();
    let indices = match parent.and_then(|id| project.timeline_index(id)) {
        Some(index) => tree.children(index),
        None => tree.root(),
    };
    indices.iter().map(|&i| project.timelines()[i].id.clone()).collect()
}

/// A timeline and everything below it, parents first.
fn subtree(project: &Project, id: &SaveId) -> Vec<SaveId> {
    let mut out = vec![id.clone()];
    let mut i = 0;
    while i < out.len() {
        let children = children_of(project, Some(&out[i]));
        out.extend(children);
        i += 1;
    }
    out
}

/// Gives `ordered` the parent `parent` and tree indices in that order.
fn set_children(edit: &mut Edit, parent: Option<&SaveId>, ordered: &[SaveId]) {
    let parent_id = parent.cloned().unwrap_or_else(SaveId::root);
    for (i, id) in ordered.iter().enumerate() {
        if let Some(timeline) = edit.timeline(id) {
            if timeline.parent != parent_id || timeline.parent_tree_index != Some(i as i64) {
                timeline.parent = parent_id.clone();
                timeline.parent_tree_index = Some(i as i64);
            }
        }
    }
}

fn parent_of(project: &Project, id: &SaveId) -> Option<SaveId> {
    let index = project.timeline_index(id)?;
    project.tree().parent(index).map(|p| project.timelines()[p].id.clone())
}

/// A body part timeline for a part of a model (`tl_new_part`,
/// `tl_value_spawn`).
fn part_timeline(id: SaveId, part: &ModelPart, template: &SaveId, owner: &SaveId, edit: &Edit) -> Timeline {
    let mut tl = Timeline::new(id, TlType::Bodypart, &edit.project().file.defaults);
    tl.temp = ObjRef::Id(template.clone());
    tl.part_of = ObjRef::Id(owner.clone());
    tl.model_part_name = part.name.clone();
    tl.inherit.alpha = true;
    tl.inherit.color = true;
    tl.inherit.texture = true;
    tl.inherit.surface = true;
    tl.inherit.subsurface = true;
    tl.inherit.rot_point = true;
    tl.scale_resize = false;
    tl.lock_bend = part.lock_bend;
    tl.appearance.backfaces = part.backfaces;
    tl.depth = part.depth;
    tl.lock = part.locked;
    if let Some(bend) = &part.bend {
        let v = &mut tl.default_values;
        v[ValueId::BendAngleX] = Value::Number(bend.default_angle[0]);
        v[ValueId::BendAngleY] = Value::Number(bend.default_angle[1]);
        v[ValueId::BendAngleZ] = Value::Number(bend.default_angle[2]);
        tl.inherit.bend = bend.inherit;
    }
    tl
}

impl Project {
    /// Adds a character or special block with a timeline for each part of
    /// its model (`temp_animate`, `tl_new_part`, `tl_update_part_list`).
    /// Parts the state hides get no timeline; their children hang directly
    /// below the model, as in the original.
    pub fn create_model(
        &mut self,
        kind: TlType,
        model_name: &str,
        state: Vec<(String, StateValue)>,
        file: &ModelFile,
        hidden_parts: &[String],
    ) -> Option<SaveId> {
        let temp_kind = match kind {
            TlType::Character => TempType::Character,
            TlType::SpecialBlock => TempType::SpecialBlock,
            _ => return None,
        };
        Some(self.edit("Create timeline", None, |edit| {
            let mut template = Template::new(edit.new_id(), temp_kind);
            template.model_name = model_name.to_owned();
            template.model_state = state;
            let template_id = template.id.clone();
            edit.insert_template(template);

            let owner_id = edit.new_id();
            let mut owner = Timeline::new(owner_id.clone(), kind, &edit.project().file.defaults);
            owner.temp = ObjRef::Id(template_id.clone());
            owner.parent = SaveId::root();
            owner.parent_tree_index = Some(children_of(edit.project(), None).len() as i64);

            // Parts in the model's order, each under the nearest part above
            // it that has a timeline.
            let mut parts: Vec<Timeline> = Vec::new();
            let mut children_count: HashMap<SaveId, i64> = HashMap::new();
            let mut stack: Vec<(&ModelPart, SaveId)> = file.parts.iter().rev().map(|p| (p, owner_id.clone())).collect();
            while let Some((part, parent)) = stack.pop() {
                let below = if hidden_parts.contains(&part.name) {
                    parent.clone()
                } else {
                    let mut tl = part_timeline(edit.new_id(), part, &template_id, &owner_id, edit);
                    let count = children_count.entry(parent.clone()).or_insert(0);
                    tl.parent = parent.clone();
                    tl.parent_tree_index = Some(*count);
                    *count += 1;
                    let id = tl.id.clone();
                    parts.push(tl);
                    id
                };
                for child in part.parts.iter().rev() {
                    stack.push((child, below.clone()));
                }
            }
            owner.parts = Some(parts.iter().map(|p| ObjRef::Id(p.id.clone())).collect());

            let mut end = edit.project().timelines().len();
            edit.insert_timeline(end, owner);
            for part in parts {
                end += 1;
                edit.insert_timeline(end, part);
            }
            owner_id
        }))
    }

    /// Adds a block (`temp_animate` for block templates).
    pub fn create_block(&mut self, block_name: &str, state: Vec<(String, StateValue)>) -> SaveId {
        self.edit("Create timeline", None, |edit| {
            let mut template = Template::new(edit.new_id(), TempType::Block);
            template.block_name = block_name.to_owned();
            template.block_state = state;
            let template_id = template.id.clone();
            edit.insert_template(template);

            let id = edit.new_id();
            let mut timeline = Timeline::new(id.clone(), TlType::Block, &edit.project().file.defaults);
            timeline.temp = ObjRef::Id(template_id);
            timeline.appearance.texture_filtering = true;
            timeline.parent = SaveId::root();
            timeline.parent_tree_index = Some(children_of(edit.project(), None).len() as i64);
            let end = edit.project().timelines().len();
            edit.insert_timeline(end, timeline);
            id
        })
    }

    /// Adds a timeline at the end of the root (`action_bench_create` for
    /// folders, cameras, lights and shapes). Shapes get a template of
    /// their own. Returns the new timeline's id.
    pub fn create_timeline(&mut self, kind: TlType, camera: Option<CameraPose>) -> Option<SaveId> {
        if !creatable(kind) {
            return None;
        }
        Some(self.edit("Create timeline", None, |edit| {
            let id = edit.new_id();
            let mut timeline = Timeline::new(id.clone(), kind, &edit.project().file.defaults);
            if let Some(temp_kind) = kind.temp_type() {
                let template = Template::new(edit.new_id(), temp_kind);
                timeline.temp = ObjRef::Id(template.id.clone());
                edit.insert_template(template);
            }
            if let (TlType::Camera, Some(pose)) = (kind, camera) {
                let v = &mut timeline.default_values;
                v[ValueId::PosX] = Value::Number(pose.position[0]);
                v[ValueId::PosY] = Value::Number(pose.position[1]);
                v[ValueId::PosZ] = Value::Number(pose.position[2]);
                v[ValueId::RotX] = Value::Number(-pose.look_z);
                v[ValueId::RotY] = Value::Number(pose.roll);
                v[ValueId::RotZ] = Value::Number(pose.look_xy - 90.0);
            }
            timeline.parent = SaveId::root();
            timeline.parent_tree_index = Some(children_of(edit.project(), None).len() as i64);
            let end = edit.project().timelines().len();
            edit.insert_timeline(end, timeline);
            id
        }))
    }

    /// Removes timelines with everything below them (`action_tl_remove`).
    /// Parts of models and scenery go with their owner only. References
    /// to removed timelines are cleared (`tl_remove_clean`).
    pub fn remove_timelines(&mut self, ids: &[SaveId]) {
        let mut removed: Vec<SaveId> = Vec::new();
        for id in ids {
            let Some(timeline) = self.timeline(id) else { continue };
            if !timeline.part_of.is_null() || removed.contains(id) {
                continue;
            }
            for below in subtree(self, id) {
                if !removed.contains(&below) {
                    removed.push(below);
                }
            }
        }
        if removed.is_empty() {
            return;
        }
        let gone: HashSet<SaveId> = removed.iter().cloned().collect();
        let parents: Vec<Option<SaveId>> = removed.iter().map(|id| parent_of(self, id)).collect();

        self.edit("Remove timelines", None, |edit| {
            for id in &removed {
                edit.remove_timeline(id);
            }
            // Close the gaps in the remaining parents' children.
            edit.refresh_tree();
            let mut seen = HashSet::new();
            for parent in parents.iter().filter(|p| p.as_ref().is_none_or(|p| !gone.contains(p))) {
                if seen.insert(parent.clone()) {
                    let ordered = children_of(edit.project(), parent.as_ref());
                    set_children(edit, parent.as_ref(), &ordered);
                }
            }

            let points_at_removed = |value: &Value| matches!(value, Value::Ref(ObjRef::Id(id)) if gone.contains(id));
            let reference_values: Vec<ValueId> = ValueId::ALL
                .iter()
                .copied()
                .filter(|v| matches!(v.kind(), ValueKind::Object | ValueKind::Texture))
                .collect();
            let holders: Vec<SaveId> = edit
                .project()
                .timelines()
                .iter()
                .filter(|t| {
                    reference_values.iter().any(|&v| {
                        points_at_removed(&t.default_values[v]) || t.keyframes.iter().any(|k| points_at_removed(&k.values[v]))
                    })
                })
                .map(|t| t.id.clone())
                .collect();
            for id in holders {
                let Some(timeline) = edit.timeline(&id) else { continue };
                for &v in &reference_values {
                    if points_at_removed(&timeline.default_values[v]) {
                        timeline.default_values[v] = Value::Ref(ObjRef::Null);
                    }
                    for keyframe in &mut timeline.keyframes {
                        if points_at_removed(&keyframe.values[v]) {
                            keyframe.values[v] = Value::Ref(ObjRef::Null);
                        }
                    }
                }
            }

            let refers = |r: &ObjRef| matches!(r, ObjRef::Id(id) if gone.contains(id));
            let templates: Vec<SaveId> = edit
                .project()
                .templates()
                .iter()
                .filter(|t| {
                    refers(&t.shape.tex)
                        || refers(&t.shape.tex_material)
                        || refers(&t.shape.tex_normal)
                        || t.particles.as_ref().is_some_and(|p| refers(&p.settings.spawn_region_path))
                })
                .map(|t| t.id.clone())
                .collect();
            for id in templates {
                let Some(template) = edit.template(&id) else { continue };
                for slot in [&mut template.shape.tex, &mut template.shape.tex_material, &mut template.shape.tex_normal] {
                    if refers(slot) {
                        *slot = ObjRef::Null;
                    }
                }
                if let Some(particles) = &mut template.particles {
                    if refers(&particles.settings.spawn_region_path) {
                        particles.settings.spawn_region_path = ObjRef::Null;
                    }
                }
            }
        });
    }

    /// Copies timelines with everything below them, next to the originals
    /// (`action_tl_duplicate`). Returns the ids of the copies.
    pub fn duplicate_timelines(&mut self, ids: &[SaveId]) -> Vec<SaveId> {
        // Timelines whose ancestor is copied too come along with it.
        let selected: HashSet<&SaveId> = ids.iter().collect();
        let roots: Vec<SaveId> = ids
            .iter()
            .filter(|id| self.timeline(id).is_some_and(|t| t.part_of.is_null()))
            .filter(|id| {
                let mut parent = parent_of(self, id);
                while let Some(p) = parent {
                    if selected.contains(&p) {
                        return false;
                    }
                    parent = parent_of(self, &p);
                }
                true
            })
            .cloned()
            .collect();

        self.edit("Duplicate timelines", None, |edit| {
            let mut copies = Vec::new();
            for root in &roots {
                let originals = subtree(edit.project(), root);
                let map: HashMap<SaveId, SaveId> = originals.iter().map(|id| (id.clone(), edit.new_id())).collect();
                let remap = |r: &ObjRef| match r {
                    ObjRef::Id(id) => map.get(id).map_or_else(|| r.clone(), |new| ObjRef::Id(new.clone())),
                    other => other.clone(),
                };
                let parent = parent_of(edit.project(), root);
                let siblings = children_of(edit.project(), parent.as_ref()).len();
                for original in &originals {
                    let Some(source) = edit.project().timeline(original) else { continue };
                    let mut copy = source.clone();
                    copy.id = map[original].clone();
                    copy.temp = remap(&copy.temp);
                    copy.part_of = remap(&copy.part_of);
                    copy.part_root = remap(&copy.part_root);
                    if let Some(parts) = &mut copy.parts {
                        *parts = parts.iter().map(&remap).collect();
                    }
                    if original == root {
                        copy.parent = parent.clone().unwrap_or_else(SaveId::root);
                        copy.parent_tree_index = Some(siblings as i64);
                    } else if let Some(new_parent) = map.get(&copy.parent) {
                        copy.parent = new_parent.clone();
                    }
                    let end = edit.project().timelines().len();
                    edit.insert_timeline(end, copy);
                }
                edit.refresh_tree();
                copies.push(map[root].clone());
            }
            copies
        })
    }

    /// Moves timelines under `parent` (the root for `None`) at `index`
    /// among its children, or at the end (`action_tl_parent`). Parts of
    /// models and moves into a timeline's own descendants are skipped.
    pub fn reparent_timelines(&mut self, ids: &[SaveId], parent: Option<&SaveId>, index: Option<usize>) {
        let movable: Vec<SaveId> = ids
            .iter()
            .filter(|id| self.timeline(id).is_some_and(|t| t.part_of.is_null()))
            .filter(|id| parent.is_none_or(|p| !subtree(self, id).contains(p)))
            .cloned()
            .collect();
        if movable.is_empty() || parent.is_some_and(|p| self.timeline(p).is_none()) {
            return;
        }
        let old_parents: Vec<Option<SaveId>> = movable.iter().map(|id| parent_of(self, id)).collect();

        self.edit("Move timelines", None, |edit| {
            let mut children: Vec<SaveId> =
                children_of(edit.project(), parent).into_iter().filter(|c| !movable.contains(c)).collect();
            let at = index.unwrap_or(children.len()).min(children.len());
            for (i, id) in movable.iter().enumerate() {
                children.insert(at + i, id.clone());
            }
            set_children(edit, parent, &children);
            edit.refresh_tree();
            let mut seen = HashSet::new();
            for old in &old_parents {
                if old.as_ref() != parent && seen.insert(old.clone()) {
                    let ordered = children_of(edit.project(), old.as_ref());
                    set_children(edit, old.as_ref(), &ordered);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::IdGenerator;
    use mi_format::project::{Keyframe, ProjectFile};

    /// FOLDER > (CUBE, LAMP); CAM at the root, aiming at CUBE.
    fn project() -> Project {
        let mut file = ProjectFile::new(0.0, 1.0);
        let mut add = |id: &str, kind: TlType, parent: &str, index: i64| {
            let mut tl = Timeline::new(SaveId::new(id), kind, &file.defaults);
            tl.parent = if parent.is_empty() { SaveId::root() } else { SaveId::new(parent) };
            tl.parent_tree_index = Some(index);
            file.objects.timelines.push(tl);
        };
        add("FOLDER", TlType::Folder, "", 0);
        add("CUBE", TlType::Cube, "FOLDER", 0);
        add("LAMP", TlType::PointLight, "FOLDER", 1);
        add("CAM", TlType::Camera, "", 1);
        let cam = file.objects.timelines.last_mut().unwrap();
        cam.default_values[ValueId::PathObj] = Value::Ref(ObjRef::id("CUBE"));
        let mut values = cam.default_values.clone();
        values[ValueId::IkTarget] = Value::Ref(ObjRef::id("CUBE"));
        cam.keyframes.push(Keyframe { position: 0, values });
        Project::from_file(file, IdGenerator::new(5)).0
    }

    fn names(project: &Project) -> Vec<(String, usize)> {
        project
            .tree()
            .order()
            .iter()
            .map(|&i| (project.timelines()[i].id.to_string(), project.tree().depth(i)))
            .collect()
    }

    fn s(list: &[(&str, usize)]) -> Vec<(String, usize)> {
        list.iter().map(|(n, d)| (n.to_string(), *d)).collect()
    }

    #[test]
    fn created_timelines_go_to_the_end_of_the_root() {
        let mut project = project();
        let pose = CameraPose { position: [1.0, 2.0, 3.0], look_xy: 120.0, look_z: 10.0, roll: 5.0 };
        let cube = project.create_timeline(TlType::Sphere, None).unwrap();
        let cam = project.create_timeline(TlType::Camera, Some(pose)).unwrap();
        assert!(project.create_timeline(TlType::Bodypart, None).is_none());
        let order = names(&project);
        assert_eq!(order[order.len() - 2], (cube.to_string(), 0));
        assert_eq!(order[order.len() - 1], (cam.to_string(), 0));
        // Shapes get a template of their own.
        let sphere = project.timeline(&cube).unwrap();
        let template = project.template(sphere.temp.as_id().unwrap()).unwrap();
        assert_eq!(template.kind, mi_core::TempType::Sphere);
        let camera = &project.timeline(&cam).unwrap().default_values;
        assert_eq!(
            [camera.number(ValueId::PosX), camera.number(ValueId::RotX), camera.number(ValueId::RotY), camera.number(ValueId::RotZ)],
            [1.0, -10.0, 5.0, 30.0]
        );
        project.undo();
        project.undo();
        assert_eq!(project.timelines().len(), 4);
        assert!(project.templates().is_empty());
    }

    #[test]
    fn removing_takes_children_and_clears_references() {
        let mut project = project();
        project.remove_timelines(&[SaveId::new("FOLDER")]);
        assert_eq!(names(&project), s(&[("CAM", 0)]));
        let cam = project.timeline(&SaveId::new("CAM")).unwrap();
        assert_eq!(cam.default_values[ValueId::PathObj], Value::Ref(ObjRef::Null));
        assert_eq!(cam.keyframes[0].values[ValueId::IkTarget], Value::Ref(ObjRef::Null));
        // The camera moved up to the first place.
        assert_eq!(cam.parent_tree_index, Some(0));

        project.undo();
        assert_eq!(names(&project), s(&[("FOLDER", 0), ("CUBE", 1), ("LAMP", 1), ("CAM", 0)]));
        let cam = project.timeline(&SaveId::new("CAM")).unwrap();
        assert_eq!(cam.default_values[ValueId::PathObj], Value::Ref(ObjRef::id("CUBE")));
    }

    #[test]
    fn duplicates_copy_the_subtree_next_to_the_original() {
        let mut project = project();
        // The cube is inside the folder, so it is not copied twice.
        let copies = project.duplicate_timelines(&[SaveId::new("FOLDER"), SaveId::new("CUBE")]);
        assert_eq!(copies.len(), 1);
        let order = names(&project);
        assert_eq!(order.len(), 7);
        assert_eq!(order[4], (copies[0].to_string(), 0));
        assert_eq!((order[5].1, order[6].1), (1, 1));
        let copied_cube = project.timeline(&SaveId::new(&order[5].0)).unwrap();
        assert_eq!(copied_cube.kind, TlType::Cube);
        assert_ne!(copied_cube.id, SaveId::new("CUBE"));
        project.undo();
        assert_eq!(project.timelines().len(), 4);
    }

    #[test]
    fn reparenting_reorders_and_refuses_loops() {
        let mut project = project();
        project.reparent_timelines(&[SaveId::new("CAM")], Some(&SaveId::new("FOLDER")), Some(1));
        assert_eq!(names(&project), s(&[("FOLDER", 0), ("CUBE", 1), ("CAM", 1), ("LAMP", 1)]));
        // A folder cannot go into its own child.
        project.reparent_timelines(&[SaveId::new("FOLDER")], Some(&SaveId::new("CUBE")), None);
        assert_eq!(project.history().undo_label(), Some("Move timelines"));
        assert_eq!(names(&project)[0], ("FOLDER".to_string(), 0));
        // Out to the root, at the front.
        project.reparent_timelines(&[SaveId::new("LAMP")], None, Some(0));
        assert_eq!(names(&project), s(&[("LAMP", 0), ("FOLDER", 0), ("CUBE", 1), ("CAM", 1)]));
        project.undo();
        project.undo();
        assert_eq!(names(&project), s(&[("FOLDER", 0), ("CUBE", 1), ("LAMP", 1), ("CAM", 0)]));
    }
}
