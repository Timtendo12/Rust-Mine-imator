//! Edits of timelines and keyframes, as undoable steps (`tl_value_set`,
//! `tl_keyframe_add`, `action_tl_keyframes_move`,
//! `action_tl_keyframes_remove`).

use crate::Project;
use mi_anim::value_rules::{clamp, ClampContext};
use mi_core::{SaveId, Value, ValueId};
use mi_format::json::Json;
use mi_format::project::{Appearance, Background, Inherit, Keyframe, RenderSettings, Timeline};

/// A keyframe, by its timeline and frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyframeRef {
    pub timeline: SaveId,
    pub position: i64,
}

/// A change of the project settings.
#[derive(Debug, Clone, PartialEq)]
pub enum InfoChange {
    Name(String),
    Author(String),
    Description(String),
    /// Frames per second, 1 to 100 as in the original.
    Tempo(f64),
    VideoSize(f64, f64),
}

/// Which group of settings of a timeline a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineSetting {
    /// What the timeline takes over from its parent.
    Inherit,
    /// Rendering options.
    Appearance,
    /// One of [`TIMELINE_FLAGS`].
    Flag,
}

/// Switches of a timeline itself, by their names in project files.
pub const TIMELINE_FLAGS: &[&str] = &["lock", "lock_bend", "scale_resize", "hq_hiding", "lq_hiding", "wind", "wind_terrain"];

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

    /// Sets a background setting by its key in project files (sky, fog,
    /// ground, ...). Returns whether the key exists.
    pub fn set_background_field(&mut self, key: &str, value: Json, merge: Option<&str>) -> bool {
        if !Background::KEYS.contains(&key) {
            return false;
        }
        self.edit("Change environment", merge, |edit| edit.background().set_field(key, value))
    }

    /// Sets a render setting by its key in project files. Choosing settings
    /// by hand makes them custom rather than a preset.
    pub fn set_render_field(&mut self, key: &str, value: Json, merge: Option<&str>) -> bool {
        if !RenderSettings::KEYS.contains(&key) {
            return false;
        }
        self.edit("Change render settings", merge, |edit| {
            edit.info().render_settings.clear();
            edit.render().set_field(key, value)
        })
    }

    /// Changes a project setting (`action_project_*`).
    pub fn set_project_info(&mut self, change: InfoChange) {
        self.edit("Change project settings", None, |edit| {
            let info = edit.info();
            match change {
                InfoChange::Name(name) => info.name = name,
                InfoChange::Author(author) => info.author = author,
                InfoChange::Description(description) => info.description = description,
                InfoChange::Tempo(tempo) => info.tempo = tempo.clamp(1.0, 100.0).round(),
                InfoChange::VideoSize(width, height) => {
                    info.video_width = width.clamp(1.0, 8192.0).round();
                    info.video_height = height.clamp(1.0, 8192.0).round();
                }
            }
        });
    }

    /// Changes a setting of timelines that is not animated
    /// (`action_tl_inherit_*`, `action_tl_backfaces`, `action_tl_lock`, ...):
    /// one of the inherit switches, an appearance option, or a flag of the
    /// timeline itself. Returns whether the setting exists.
    pub fn set_timeline_setting(&mut self, ids: &[SaveId], setting: TimelineSetting, key: &str, value: Json) -> bool {
        let known = match setting {
            TimelineSetting::Inherit => Inherit::KEYS.contains(&key),
            TimelineSetting::Appearance => Appearance::KEYS.contains(&key),
            TimelineSetting::Flag => TIMELINE_FLAGS.contains(&key),
        };
        if !known {
            return false;
        }
        self.edit("Change timeline settings", None, |edit| {
            for id in ids {
                let Some(timeline) = edit.timeline(id) else { continue };
                match setting {
                    TimelineSetting::Inherit => {
                        timeline.inherit.set_field(key, value.clone());
                    }
                    TimelineSetting::Appearance => {
                        timeline.appearance.set_field(key, value.clone());
                    }
                    TimelineSetting::Flag => {
                        let on = value.as_flag().unwrap_or(false);
                        match key {
                            "lock" => timeline.lock = on,
                            "lock_bend" => timeline.lock_bend = on,
                            "scale_resize" => timeline.scale_resize = on,
                            "hq_hiding" => timeline.hq_hiding = on,
                            "lq_hiding" => timeline.lq_hiding = on,
                            "wind" => timeline.wind = on,
                            "wind_terrain" => timeline.wind_terrain = on,
                            _ => {}
                        }
                    }
                }
            }
        });
        true
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
    fn timeline_settings_by_key() {
        let mut project = project();
        let id = [SaveId::new("CUBE")];
        assert!(project.set_timeline_setting(&id, TimelineSetting::Inherit, "alpha", Json::Bool(true)));
        assert!(project.set_timeline_setting(&id, TimelineSetting::Appearance, "backfaces", Json::Bool(true)));
        assert!(project.set_timeline_setting(&id, TimelineSetting::Flag, "lock", Json::Bool(true)));
        assert!(!project.set_timeline_setting(&id, TimelineSetting::Flag, "no_such_flag", Json::Bool(true)));
        let cube = cube(&project);
        assert!(cube.inherit.alpha && cube.appearance.backfaces && cube.lock);
        project.undo();
        project.undo();
        project.undo();
        let cube = self::cube(&project);
        assert!(!cube.inherit.alpha && !cube.appearance.backfaces && !cube.lock);
        assert!(!project.undo());
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
