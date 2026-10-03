//! Sounds on the timeline: the keyframes of audio timelines
//! (`action_tl_add_sound`, `tl_keyframe_length`, `tl_update_length`).
//!
//! The project does not decode sounds itself; whoever does tells it how
//! long each sound resource is, which the lengths of sound keyframes and of
//! the whole animation depend on.

use crate::editing::insert_keyframe;
use crate::Project;
use mi_core::{ObjRef, ResType, SaveId, TlType, Value, ValueId};
use mi_format::project::{Keyframe, Timeline};

/// A sound keyframe that plays.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedSound {
    pub timeline: SaveId,
    /// The sound resource.
    pub resource: SaveId,
    /// Frame it starts at.
    pub position: i64,
    pub volume: f64,
    pub pitch: f64,
    /// Seconds into the sound that it starts from.
    pub start: f64,
    /// Seconds added to its length.
    pub end: f64,
}

/// How long a sound of `seconds` plays (`tl_keyframe_length`, in seconds).
fn play_seconds(seconds: f64, pitch: f64, start: f64, end: f64) -> f64 {
    if pitch == 0.0 {
        0.0
    } else {
        (seconds / pitch + end - start).max(0.0)
    }
}

fn sound_of(keyframe: &Keyframe) -> Option<&SaveId> {
    match &keyframe.values[ValueId::SoundObj] {
        Value::Ref(ObjRef::Id(id)) => Some(id),
        _ => None,
    }
}

impl Project {
    /// Tells the project how long a sound resource is.
    pub fn set_sound_seconds(&mut self, resource: &SaveId, seconds: f64) {
        self.sound_seconds.insert(resource.clone(), seconds);
    }

    /// Length in seconds of a sound resource, once it is known.
    pub fn sound_seconds(&self, resource: &SaveId) -> Option<f64> {
        self.sound_seconds.get(resource).copied()
    }

    /// How many frames a keyframe lasts: for a keyframe of an audio
    /// timeline whose sound is known, how long the sound plays; else 0.
    pub fn keyframe_length(&self, timeline: &Timeline, keyframe: &Keyframe) -> f64 {
        if timeline.kind != TlType::Audio {
            return 0.0;
        }
        let Some(seconds) = sound_of(keyframe).and_then(|id| self.sound_seconds(id)) else { return 0.0 };
        let v = &keyframe.values;
        let played = play_seconds(seconds, v.number(ValueId::SoundPitch), v.number(ValueId::SoundStart), v.number(ValueId::SoundEnd));
        played * self.file.info.tempo
    }

    /// The sounds that play: keyframes with a sound, of audio timelines
    /// that are not hidden (unless `hidden` asks for those too).
    pub fn placed_sounds(&self, hidden: bool) -> Vec<PlacedSound> {
        let mut sounds = Vec::new();
        for timeline in self.timelines() {
            if timeline.kind != TlType::Audio || (timeline.hide && !hidden) {
                continue;
            }
            for keyframe in &timeline.keyframes {
                let Some(resource) = sound_of(keyframe) else { continue };
                let v = &keyframe.values;
                sounds.push(PlacedSound {
                    timeline: timeline.id.clone(),
                    resource: resource.clone(),
                    position: keyframe.position,
                    volume: v.number(ValueId::SoundVolume),
                    pitch: v.number(ValueId::SoundPitch),
                    start: v.number(ValueId::SoundStart),
                    end: v.number(ValueId::SoundEnd),
                });
            }
        }
        sounds
    }

    /// Adds the sound file `source` at frame `marker`
    /// (`action_tl_add_sound`): as a keyframe of the first audio timeline
    /// among `selected`, or of a new audio timeline. Returns that timeline.
    pub fn create_audio(&mut self, source: &std::path::Path, marker: i64, selected: &[SaveId]) -> SaveId {
        let target = selected.iter().find(|id| self.timeline(id).is_some_and(|t| t.kind == TlType::Audio)).cloned();
        self.edit("Add sound", None, |edit| {
            let resource = Self::add_resource(edit, source, ResType::Sound, |_| {});
            let id = match target {
                Some(id) => id,
                None => {
                    let id = edit.new_id();
                    let mut timeline = Timeline::new(id.clone(), TlType::Audio, &edit.project().file.defaults);
                    timeline.parent = SaveId::root();
                    timeline.parent_tree_index = Some(edit.project().tree().root().len() as i64);
                    let end = edit.project().timelines().len();
                    edit.insert_timeline(end, timeline);
                    id
                }
            };
            if let Some(timeline) = edit.timeline(&id) {
                let mut values = timeline.default_values.clone();
                values.set(ValueId::SoundObj, Value::Ref(ObjRef::Id(resource)));
                insert_keyframe(timeline, Keyframe { position: marker.max(0), values });
            }
            id
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectContext;

    #[test]
    fn sounds_become_keyframes_of_audio_timelines() {
        let dir = std::env::temp_dir().join(format!("mi-sounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("boom.ogg"), b"x").unwrap();
        std::fs::write(dir.join("step.wav"), b"x").unwrap();
        let mut project = Project::new(ProjectContext::default());

        // Without an audio timeline selected a new one is made.
        let first = project.create_audio(&dir.join("boom.ogg"), 10, &[]);
        assert_eq!(project.timeline(&first).unwrap().kind, TlType::Audio);
        // With one selected the sound is added to it; a taken frame is skipped.
        let same = project.create_audio(&dir.join("step.wav"), 10, std::slice::from_ref(&first));
        assert_eq!(same, first);
        let positions: Vec<i64> = project.timeline(&first).unwrap().keyframes.iter().map(|k| k.position).collect();
        assert_eq!(positions, [10, 11]);
        assert_eq!(project.resources().iter().map(|r| (r.kind, r.filename.as_str())).collect::<Vec<_>>(), [
            (ResType::Sound, "boom.ogg"),
            (ResType::Sound, "step.wav")
        ]);

        let sounds = project.placed_sounds(false);
        assert_eq!(sounds.len(), 2);
        assert_eq!((sounds[0].position, sounds[0].volume, sounds[0].pitch), (10, 1.0, 1.0));
        assert_eq!(sounds[0].resource, project.resources()[0].id);

        // Lengths are known once the sounds are; the animation lasts as
        // long as its last sound (24 frames per second).
        assert_eq!(project.length(), 11);
        let boom = project.resources()[0].id.clone();
        project.set_sound_seconds(&boom, 2.0);
        let timeline = project.timeline(&first).unwrap();
        assert_eq!(project.keyframe_length(timeline, &timeline.keyframes[0]), 48.0);
        assert_eq!(project.keyframe_length(timeline, &timeline.keyframes[1]), 0.0);
        assert_eq!(project.length(), 58);

        // Hidden audio timelines are silent.
        project.set_hidden(std::slice::from_ref(&first), true);
        assert!(project.placed_sounds(false).is_empty());
        assert_eq!(project.placed_sounds(true).len(), 2);

        project.undo();
        project.undo();
        assert_eq!(project.timeline(&first).unwrap().keyframes.len(), 1);
        project.undo();
        assert!(project.timelines().is_empty() && project.resources().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
