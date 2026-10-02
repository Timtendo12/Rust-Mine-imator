//! Edits of timelines and keyframes, as undoable steps (`tl_value_set`,
//! `tl_keyframe_add`, `action_tl_keyframes_move`,
//! `action_tl_keyframes_remove`).

use crate::Project;
use mi_anim::value_rules::{clamp, ClampContext};
use mi_core::{SaveId, Value, ValueId};
use mi_format::project::{Keyframe, Timeline};

/// A keyframe, by its timeline and frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyframeRef {
    pub timeline: SaveId,
    pub position: i64,
}

/// How an edited value relates to the one already there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueChange {
    /// Replace it.
    Set,
    /// Add to it (dragging a number).
    Add,
}

/// `tl_keyframe_add`: inserts at `position`, or the next free frame after
/// it. Returns the index.
fn insert_keyframe(timeline: &mut Timeline, mut keyframe: Keyframe) -> usize {
    let keyframes = &mut timeline.keyframes;
    let mut i = 0;
    while i < keyframes.len() {
        if keyframes[i].position == keyframe.position {
            // Find the next empty slot.
            while i < keyframes.len() && keyframes[i].position == keyframe.position {
                i += 1;
                keyframe.position += 1;
            }
            break;
        }
        if keyframes[i].position > keyframe.position {
            break;
        }
        i += 1;
    }
    keyframes.insert(i, keyframe);
    i
}

impl Project {
    fn clamp_context(&self) -> ClampContext {
        ClampContext { render_distance: self.file.render.distance, ..ClampContext::default() }
    }

    /// Changes values of timelines at frame `marker` (`tl_value_set`): the
    /// keyframe at the marker is edited, or a new one is made there from
    /// the values the timeline has at that frame.
    pub fn set_values(
        &mut self,
        timelines: &[SaveId],
        marker: i64,
        values: &[(ValueId, Value)],
        change: ValueChange,
        merge: Option<&str>,
    ) {
        let marker = marker.max(0);
        let ctx = self.clamp_context();
        self.edit("Change value", merge, |edit| {
            let (state, order) = edit.project().evaluate(marker as f64);
            for id in timelines {
                let Some(index) = edit.project().timeline_index(id) else { continue };
                let current = order.iter().position(|&i| i == index).map(|node| state.nodes[node].values.clone());
                let Some(timeline) = edit.timeline(id) else { continue };
                let k = match timeline.keyframes.iter().position(|k| k.position == marker) {
                    Some(k) => k,
                    None => {
                        let values = current.unwrap_or_else(|| timeline.default_values.clone());
                        insert_keyframe(timeline, Keyframe { position: marker, values })
                    }
                };
                let keyframe = &mut timeline.keyframes[k];
                for (value_id, value) in values {
                    let new = match (change, value, &keyframe.values[*value_id]) {
                        (ValueChange::Add, Value::Number(add), Value::Number(old)) => Value::Number(old + add),
                        _ => value.clone(),
                    };
                    keyframe.values.set(*value_id, clamp(*value_id, new, ctx));
                }
            }
        });
    }

    /// Moves keyframes by `offset` frames from the positions in `from`
    /// (`action_tl_keyframes_move`). Keyframes land on the next free frame
    /// if theirs is taken. Returns where each one ended up.
    pub fn move_keyframes(&mut self, from: &[KeyframeRef], offset: i64, merge: Option<&str>) -> Vec<KeyframeRef> {
        self.edit("Move keyframes", merge, |edit| {
            // Take them all out first, then put them back.
            let mut taken = Vec::new();
            for key in from {
                let Some(timeline) = edit.timeline(&key.timeline) else { continue };
                if let Some(i) = timeline.keyframes.iter().position(|k| k.position == key.position) {
                    let keyframe = timeline.keyframes.remove(i);
                    taken.push((key.timeline.clone(), keyframe));
                }
            }
            let mut moved = Vec::new();
            for (timeline_id, mut keyframe) in taken {
                keyframe.position = (keyframe.position + offset).max(0);
                let Some(timeline) = edit.timeline(&timeline_id) else { continue };
                let index = insert_keyframe(timeline, keyframe);
                moved.push(KeyframeRef { timeline: timeline_id, position: timeline.keyframes[index].position });
            }
            moved
        })
    }

    /// Removes keyframes (`action_tl_keyframes_remove`).
    pub fn remove_keyframes(&mut self, keys: &[KeyframeRef]) {
        self.edit("Remove keyframes", None, |edit| {
            for key in keys {
                if let Some(timeline) = edit.timeline(&key.timeline) {
                    timeline.keyframes.retain(|k| k.position != key.position);
                }
            }
        });
    }

    /// Renames a timeline (`action_tl_name`).
    pub fn rename_timeline(&mut self, id: &SaveId, name: &str) {
        self.edit("Rename timeline", None, |edit| {
            if let Some(timeline) = edit.timeline(id) {
                timeline.name = name.to_owned();
            }
        });
    }

    /// Hides or shows timelines in the viewport (`action_tl_hide`).
    pub fn set_hidden(&mut self, ids: &[SaveId], hidden: bool) {
        self.edit(if hidden { "Hide timelines" } else { "Show timelines" }, None, |edit| {
            for id in ids {
                if let Some(timeline) = edit.timeline(id) {
                    timeline.hide = hidden;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{IdGenerator, TlType};
    use mi_format::project::ProjectFile;

    fn project() -> Project {
        let mut file = ProjectFile::new(0.0, 1.0);
        let mut tl = Timeline::new(SaveId::new("CUBE"), TlType::Cube, &file.defaults);
        let mut values = file.defaults.clone();
        values[ValueId::PosX] = Value::Number(10.0);
        tl.keyframes.push(Keyframe { position: 0, values: values.clone() });
        values[ValueId::PosX] = Value::Number(30.0);
        tl.keyframes.push(Keyframe { position: 20, values });
        file.objects.timelines.push(tl);
        Project::from_file(file, IdGenerator::new(1)).0
    }

    fn cube(project: &Project) -> &Timeline {
        project.timeline(&SaveId::new("CUBE")).unwrap()
    }

    fn positions(project: &Project) -> Vec<i64> {
        cube(project).keyframes.iter().map(|k| k.position).collect()
    }

    #[test]
    fn editing_between_keyframes_adds_one_from_the_current_values() {
        let mut project = project();
        let id = [SaveId::new("CUBE")];
        project.set_values(&id, 10, &[(ValueId::PosY, Value::Number(5.0))], ValueChange::Set, None);
        assert_eq!(positions(&project), [0, 10, 20]);
        let added = &cube(&project).keyframes[1].values;
        // Interpolated halfway, plus the edit.
        assert_eq!((added.number(ValueId::PosX), added.number(ValueId::PosY)), (20.0, 5.0));
        assert!(project.is_changed());

        // On a keyframe, that one is edited.
        project.set_values(&id, 20, &[(ValueId::PosX, Value::Number(1.0))], ValueChange::Add, None);
        assert_eq!(cube(&project).keyframes[2].values.number(ValueId::PosX), 31.0);
        // Values are clamped.
        project.set_values(&id, 20, &[(ValueId::Alpha, Value::Number(-2.0))], ValueChange::Set, None);
        assert_eq!(cube(&project).keyframes[2].values.number(ValueId::Alpha), 0.0);

        assert!(project.undo() && project.undo());
        assert_eq!(cube(&project).keyframes[2].values.number(ValueId::PosX), 30.0);
        assert!(project.undo());
        assert_eq!(positions(&project), [0, 20]);
        assert!(!project.undo());
        assert!(project.redo());
        assert_eq!(positions(&project), [0, 10, 20]);
    }

    #[test]
    fn drags_merge_into_one_step_from_where_they_started() {
        let mut project = project();
        let id = [SaveId::new("CUBE")];
        for step in 1..=5 {
            project.set_values(&id, 0, &[(ValueId::PosX, Value::Number(step as f64))], ValueChange::Add, Some("drag"));
        }
        // Every update applies the offset to the value before the drag.
        assert_eq!(cube(&project).keyframes[0].values.number(ValueId::PosX), 15.0);
        project.finish_edit();
        project.set_values(&id, 0, &[(ValueId::PosX, Value::Number(1.0))], ValueChange::Add, Some("drag"));
        assert_eq!(cube(&project).keyframes[0].values.number(ValueId::PosX), 16.0);
        assert!(project.undo());
        assert_eq!(cube(&project).keyframes[0].values.number(ValueId::PosX), 15.0);
        assert!(project.undo());
        assert_eq!(cube(&project).keyframes[0].values.number(ValueId::PosX), 10.0);
        assert!(!project.undo());
    }

    #[test]
    fn moved_keyframes_skip_taken_frames_and_stay_above_zero() {
        let mut project = project();
        let key = |position| KeyframeRef { timeline: SaveId::new("CUBE"), position };
        let moved = project.move_keyframes(&[key(0)], 20, Some("move"));
        assert_eq!(moved, [key(21)]);
        assert_eq!(positions(&project), [20, 21]);
        // A drag recomputes from the start.
        let moved = project.move_keyframes(&[key(0)], 5, Some("move"));
        assert_eq!(moved, [key(5)]);
        assert_eq!(positions(&project), [5, 20]);
        project.finish_edit();
        project.move_keyframes(&[key(5), key(20)], -10, None);
        assert_eq!(positions(&project), [0, 10]);
        project.remove_keyframes(&[key(0)]);
        assert_eq!(positions(&project), [10]);
        project.undo();
        project.undo();
        project.undo();
        assert_eq!(positions(&project), [0, 20]);
    }

    #[test]
    fn rename_and_hide() {
        let mut project = project();
        let id = SaveId::new("CUBE");
        project.rename_timeline(&id, "Box");
        project.set_hidden(std::slice::from_ref(&id), true);
        assert!(cube(&project).hide && cube(&project).name == "Box");
        assert_eq!(project.history().undo_label(), Some("Hide timelines"));
        project.undo();
        project.undo();
        assert!(!cube(&project).hide && cube(&project).name.is_empty());
        // An edit that changes nothing is not a step.
        project.set_hidden(std::slice::from_ref(&id), false);
        assert_eq!(project.history().undo_label(), None);
    }
}
