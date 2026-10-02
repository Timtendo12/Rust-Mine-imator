//! Creating, copying and pasting keyframes (`action_tl_keyframes_create`,
//! `tl_keyframes_copy`, `tl_keyframes_paste`, `action_tl_keyframes_cut`).

use crate::editing::insert_keyframe;
use crate::{KeyframeRef, Project};
use mi_core::{ObjRef, SaveId, Value};
use mi_format::project::{Keyframe, Timeline};
use mi_format::ValueSet;

/// A copied keyframe.
#[derive(Debug, Clone, PartialEq)]
struct CopiedKeyframe {
    timeline: SaveId,
    /// The model the timeline is a part of, or the timeline itself.
    owner: SaveId,
    /// Name of the part in its model; empty for timelines that are no part.
    part_name: String,
    /// Frames after the first copied keyframe.
    offset: i64,
    values: ValueSet,
}

/// Keyframes that were copied, to paste any number of times.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyframeClipboard {
    keyframes: Vec<CopiedKeyframe>,
}

impl KeyframeClipboard {
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keyframes.len()
    }
}

/// Where pasted keyframes go.
#[derive(Debug, Clone, PartialEq)]
enum PasteMode {
    /// Into this timeline, whichever one they came from.
    Free(SaveId),
    /// Into the parts with the same names of this model.
    Model(SaveId),
    /// Back into the timelines they came from.
    Fixed,
}

fn owner_of(timeline: &Timeline) -> SaveId {
    timeline.part_of.as_id().cloned().unwrap_or_else(|| timeline.id.clone())
}

impl Project {
    /// The part called `name` of the model timeline `owner` (`tl_part_find`).
    fn part_named(&self, owner: &SaveId, name: &str) -> Option<SaveId> {
        self.timelines().iter().find(|t| t.part_of.as_id() == Some(owner) && t.model_part_name == name).map(|t| t.id.clone())
    }

    /// Adds a keyframe at `marker` to each of `timelines` that has none
    /// there, with the values the timeline has at that frame
    /// (`action_tl_keyframes_create`). Returns the new keyframes.
    pub fn create_keyframes(&mut self, timelines: &[SaveId], marker: i64) -> Vec<KeyframeRef> {
        let marker = marker.max(0);
        self.edit("Create keyframes", None, |edit| {
            let (state, order) = edit.project().evaluate(marker as f64);
            let mut created = Vec::new();
            for id in timelines {
                let Some(index) = edit.project().timeline_index(id) else { continue };
                let current = order.iter().position(|&i| i == index).map(|node| state.nodes[node].values.clone());
                let Some(timeline) = edit.timeline(id) else { continue };
                if timeline.keyframes.iter().any(|k| k.position == marker) {
                    continue;
                }
                let values = current.unwrap_or_else(|| timeline.default_values.clone());
                insert_keyframe(timeline, Keyframe { position: marker, values });
                created.push(KeyframeRef { timeline: id.clone(), position: marker });
            }
            created
        })
    }

    /// Copies keyframes (`tl_keyframes_copy`). Their positions are kept
    /// relative to the first one.
    pub fn copy_keyframes(&self, keys: &[KeyframeRef]) -> KeyframeClipboard {
        let mut keyframes = Vec::new();
        for key in keys {
            let Some(timeline) = self.timeline(&key.timeline) else { continue };
            let Some(keyframe) = timeline.keyframes.iter().find(|k| k.position == key.position) else { continue };
            keyframes.push(CopiedKeyframe {
                timeline: timeline.id.clone(),
                owner: owner_of(timeline),
                part_name: timeline.model_part_name.clone(),
                offset: keyframe.position,
                values: keyframe.values.clone(),
            });
        }
        let first = keyframes.iter().map(|k| k.offset).min().unwrap_or(0);
        for keyframe in &mut keyframes {
            keyframe.offset -= first;
        }
        KeyframeClipboard { keyframes }
    }

    /// How `tl_keyframes_paste` decides where keyframes go: keyframes of
    /// one timeline go into the single selected timeline; keyframes of one
    /// model go into the model that is selected (or whose part is); anything
    /// else goes back where it came from.
    fn paste_mode(&self, clipboard: &KeyframeClipboard, selected: &[SaveId]) -> PasteMode {
        let Some(first) = clipboard.keyframes.first() else { return PasteMode::Fixed };
        if clipboard.keyframes.iter().all(|k| k.timeline == first.timeline) {
            return match selected {
                [only] if self.timeline(only).is_some() => PasteMode::Free(only.clone()),
                _ => PasteMode::Fixed,
            };
        }
        if clipboard.keyframes.iter().all(|k| k.owner == first.owner) {
            for id in selected {
                let Some(timeline) = self.timeline(id) else { continue };
                if let Some(owner) = timeline.part_of.as_id() {
                    return PasteMode::Model(owner.clone());
                }
                if timeline.parts.is_some() {
                    return PasteMode::Model(id.clone());
                }
            }
        }
        PasteMode::Fixed
    }

    /// Pastes copied keyframes with the first one at `position`
    /// (`action_tl_keyframes_paste`); `selected` are the selected timelines.
    /// Keyframes land on the next free frame if theirs is taken. Returns the
    /// pasted keyframes.
    pub fn paste_keyframes(&mut self, clipboard: &KeyframeClipboard, position: i64, selected: &[SaveId]) -> Vec<KeyframeRef> {
        let position = position.max(0);
        let mode = self.paste_mode(clipboard, selected);
        // Find the targets first: the tree is read while timelines change.
        let targets: Vec<Option<SaveId>> = clipboard
            .keyframes
            .iter()
            .map(|copied| {
                let target = match &mode {
                    PasteMode::Free(id) => return Some(id.clone()),
                    PasteMode::Model(id) => id.clone(),
                    PasteMode::Fixed => copied.owner.clone(),
                };
                let timeline = self.timeline(&target)?;
                if timeline.parts.is_some() && !copied.part_name.is_empty() {
                    return self.part_named(&target, &copied.part_name);
                }
                Some(target)
            })
            .collect();
        // Objects a copied keyframe refers to may be gone by now.
        let exists = |id: &SaveId| self.timeline(id).is_some() || self.template(id).is_some() || self.resource(id).is_some();
        let values: Vec<ValueSet> = clipboard
            .keyframes
            .iter()
            .map(|copied| {
                let mut values = copied.values.clone();
                for &id in mi_core::ValueId::ALL {
                    if let Value::Ref(ObjRef::Id(target)) = &values[id] {
                        if !exists(target) {
                            values.set(id, Value::Ref(ObjRef::Null));
                        }
                    }
                }
                values
            })
            .collect();

        self.edit("Paste keyframes", None, |edit| {
            let mut pasted = Vec::new();
            for ((copied, target), values) in clipboard.keyframes.iter().zip(targets).zip(values) {
                let Some(target) = target else { continue };
                let Some(timeline) = edit.timeline(&target) else { continue };
                let index = insert_keyframe(timeline, Keyframe { position: position + copied.offset, values });
                pasted.push(KeyframeRef { timeline: target, position: timeline.keyframes[index].position });
            }
            pasted
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{IdGenerator, TlType, ValueId};
    use mi_format::project::ProjectFile;

    fn id(name: &str) -> SaveId {
        SaveId::new(name)
    }

    fn key(timeline: &str, position: i64) -> KeyframeRef {
        KeyframeRef { timeline: id(timeline), position }
    }

    /// Two cubes and two models with an arm each.
    fn project() -> Project {
        let mut file = ProjectFile::new(0.0, 1.0);
        let mut add = |name: &str, kind: TlType, frames: &[(i64, f64)], part_of: Option<&str>| {
            let mut tl = Timeline::new(id(name), kind, &file.defaults);
            for &(position, x) in frames {
                let mut values = file.defaults.clone();
                values[ValueId::PosX] = Value::Number(x);
                tl.keyframes.push(Keyframe { position, values });
            }
            match part_of {
                Some(owner) => {
                    tl.part_of = ObjRef::Id(id(owner));
                    tl.parent = id(owner);
                    tl.model_part_name = "arm".into();
                }
                None if kind == TlType::Character => tl.parts = Some(vec![ObjRef::Id(id(&format!("{name}_ARM")))]),
                None => {}
            }
            file.objects.timelines.push(tl);
        };
        add("A", TlType::Cube, &[(0, 1.0), (10, 2.0)], None);
        add("B", TlType::Cube, &[(5, 9.0)], None);
        add("STEVE", TlType::Character, &[(2, 3.0)], None);
        add("STEVE_ARM", TlType::Bodypart, &[(4, 4.0)], Some("STEVE"));
        add("ALEX", TlType::Character, &[], None);
        add("ALEX_ARM", TlType::Bodypart, &[], Some("ALEX"));
        Project::from_file(file, IdGenerator::new(1)).0
    }

    fn frames(project: &Project, timeline: &str) -> Vec<(i64, f64)> {
        project.timeline(&id(timeline)).unwrap().keyframes.iter().map(|k| (k.position, k.values.number(ValueId::PosX))).collect()
    }

    #[test]
    fn created_keyframes_take_the_values_at_the_marker() {
        let mut project = project();
        let created = project.create_keyframes(&[id("A"), id("B")], 5);
        // B already has one there.
        assert_eq!(created, [key("A", 5)]);
        assert_eq!(frames(&project, "A"), [(0, 1.0), (5, 1.5), (10, 2.0)]);
        assert!(project.undo());
        assert_eq!(frames(&project, "A"), [(0, 1.0), (10, 2.0)]);
        // Nothing to create is not a step.
        project.create_keyframes(&[id("B")], 5);
        assert_eq!(project.history().undo_label(), None);
    }

    #[test]
    fn keyframes_of_one_timeline_go_into_the_selected_one() {
        let mut project = project();
        let clipboard = project.copy_keyframes(&[key("A", 10), key("A", 0), key("A", 99)]);
        assert_eq!(clipboard.len(), 2);
        // Positions are relative to the first; a taken frame is skipped.
        let pasted = project.paste_keyframes(&clipboard, 5, &[id("B")]);
        assert_eq!(pasted, [key("B", 15), key("B", 6)]);
        assert_eq!(frames(&project, "B"), [(5, 9.0), (6, 1.0), (15, 2.0)]);
        assert!(project.undo());
        assert_eq!(frames(&project, "B"), [(5, 9.0)]);
        // Without exactly one timeline selected they go back where they came from.
        project.paste_keyframes(&clipboard, 20, &[]);
        assert_eq!(frames(&project, "A"), [(0, 1.0), (10, 2.0), (20, 1.0), (30, 2.0)]);
    }

    #[test]
    fn keyframes_of_a_model_go_into_the_selected_model_by_part_name() {
        let mut project = project();
        let clipboard = project.copy_keyframes(&[key("STEVE", 2), key("STEVE_ARM", 4)]);
        // Selecting a part selects its model for pasting.
        let pasted = project.paste_keyframes(&clipboard, 10, &[id("ALEX_ARM")]);
        assert_eq!(pasted, [key("ALEX", 10), key("ALEX_ARM", 12)]);
        assert_eq!(frames(&project, "ALEX_ARM"), [(12, 4.0)]);
        // With something else selected they return to their own timelines.
        project.paste_keyframes(&clipboard, 20, &[id("A")]);
        assert_eq!(frames(&project, "STEVE"), [(2, 3.0), (20, 3.0)]);
        assert_eq!(frames(&project, "STEVE_ARM"), [(4, 4.0), (22, 4.0)]);
        assert_eq!(frames(&project, "A").len(), 2);
    }

    #[test]
    fn keyframes_of_several_timelines_go_back_and_skip_removed_ones() {
        let mut project = project();
        let clipboard = project.copy_keyframes(&[key("A", 0), key("B", 5)]);
        project.remove_timelines(&[id("B")]);
        let pasted = project.paste_keyframes(&clipboard, 40, &[id("A")]);
        assert_eq!(pasted, [key("A", 40)]);
        assert!(project.copy_keyframes(&[]).is_empty());
        assert!(project.paste_keyframes(&KeyframeClipboard::default(), 0, &[]).is_empty());
    }
}
