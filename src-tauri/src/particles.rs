//! The particles of the open project: one simulation per spawner timeline,
//! stepped along with the animation (`particle_spawner_update`,
//! `tl_update_values`).

use mi_anim::NodeState;
use mi_core::{ObjRef, SaveId, Value, ValueId};
use mi_format::project::{ParticleSpawner, Timeline};
use mi_particles::{Spawner, SpawnerState, STEPS_PER_SECOND};
use std::collections::HashMap;

/// The simulation of one spawner timeline.
#[derive(Default)]
struct Run {
    spawner: Spawner,
    /// Where the animation was when it was last stepped.
    marker: Option<f64>,
    /// The keyframe the animation was at then.
    keyframe: Option<i64>,
}

/// The simulations of all spawner timelines.
#[derive(Default)]
pub struct ParticleStore {
    runs: HashMap<SaveId, Run>,
}

/// Where the animation is when a spawner is stepped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// Frame of the animation.
    pub marker: f64,
    /// The same in seconds.
    pub seconds: f64,
    /// Where the timeline the spawner's `ATTRACTOR` value names is.
    pub attractor: Option<[f64; 3]>,
}

/// The keyframe at or before `marker`.
fn keyframe_at(timeline: &Timeline, marker: f64) -> Option<i64> {
    timeline.keyframes.iter().map(|k| k.position).take_while(|&position| position as f64 <= marker).last()
}

impl ParticleStore {
    /// Brings the spawner of `timeline` to a moment of the animation and
    /// gives its particles.
    pub fn advance(
        &mut self,
        timeline: &Timeline,
        node: &NodeState,
        settings: &ParticleSpawner,
        moment: Moment,
        template_frames: &dyn Fn(&str) -> i64,
    ) -> &[mi_particles::Particle] {
        let Moment { marker, seconds, attractor } = moment;
        let run = self.runs.entry(timeline.id.clone()).or_default();
        let values = &node.values;
        let keyframe = keyframe_at(timeline, marker);

        // Reaching a new keyframe while going forwards fires a burst or
        // clears the particles, if the keyframe says so.
        if run.marker.is_some_and(|before| marker > before) && keyframe != run.keyframe {
            if !settings.settings.spawn_constant && values.flag(ValueId::Spawn) && !values.flag(ValueId::Freeze) {
                run.spawner.fire();
            }
            if values.flag(ValueId::Clear) {
                run.spawner.clear();
            }
        }
        run.marker = Some(marker);
        run.keyframe = keyframe;

        let m = node.matrix_render.0;
        let state = SpawnerState {
            world_pos: node.world_pos,
            rotation: [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]],
            spawn: values.flag(ValueId::Spawn),
            freeze: values.flag(ValueId::Freeze),
            seed: values.flag(ValueId::CustomSeed).then(|| values.number(ValueId::Seed)),
            attractor,
            force: values.number(ValueId::Force),
        };
        let step = (seconds * STEPS_PER_SECOND).floor() as i64;
        run.spawner.advance(step, settings, &state, template_frames);
        &run.spawner.particles
    }

    /// Forgets spawners that are no longer in the project.
    pub fn retain(&mut self, exists: impl Fn(&SaveId) -> bool) {
        self.runs.retain(|id, _| exists(id));
    }
}

/// The timeline a spawner's particles are pulled to.
pub fn attractor_of(node: &NodeState) -> Option<&SaveId> {
    match &node.values[ValueId::Attractor] {
        Value::Ref(ObjRef::Id(id)) => Some(id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::TlType;
    use mi_format::project::{Keyframe, ParticleSource, ParticleType, ProjectFile};

    fn burst_spawner() -> ParticleSpawner {
        let mut spawner = ParticleSpawner::default();
        spawner.settings.spawn_constant = false;
        spawner.settings.spawn_amount = 5.0;
        let mut kind = ParticleType::new(SaveId::new("T"));
        kind.source = ParticleSource::Template;
        kind.settings.spawn_rate = 1.0;
        kind.settings.sprite_template_still_frame = true;
        spawner.types.push(kind);
        spawner
    }

    #[test]
    fn bursts_fire_when_the_animation_reaches_a_keyframe() {
        let file = ProjectFile::new(0.0, 1.0);
        let mut timeline = Timeline::new(SaveId::new("SPAWNER"), TlType::ParticleSpawner, &file.defaults);
        timeline.keyframes.push(Keyframe { position: 10, values: file.defaults.clone() });
        let node = {
            let nodes = [mi_anim::SceneNode { timeline: &timeline, parent: None, part_of: None, part: None, rot_point: [0.0; 3] }];
            let playhead = mi_anim::Playhead { marker: 10.0, seamless_repeat: false, region: None, length: 20.0 };
            mi_anim::update_scene(&nodes, &playhead).nodes.remove(0)
        };
        let settings = burst_spawner();
        let mut store = ParticleStore::default();
        let frames = |_: &str| 1;
        let at = |store: &mut ParticleStore, frame: f64| {
            let moment = Moment { marker: frame, seconds: frame / 24.0, attractor: None };
            store.advance(&timeline, &node, &settings, moment, &frames).len()
        };
        // Before the keyframe nothing happens.
        assert_eq!(at(&mut store, 0.0), 0);
        assert_eq!(at(&mut store, 9.0), 0);
        // Reaching it fires, once.
        assert_eq!(at(&mut store, 10.0), 5);
        assert_eq!(at(&mut store, 11.0), 5);
        assert_eq!(at(&mut store, 15.0), 5);
        // Going back clears; passing the keyframe again fires again.
        assert_eq!(at(&mut store, 5.0), 0);
        assert_eq!(at(&mut store, 12.0), 5);
        // Removed timelines are forgotten.
        store.retain(|_| false);
        assert!(store.runs.is_empty());
    }
}
