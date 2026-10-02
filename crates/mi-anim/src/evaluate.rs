//! Evaluation of a timeline's values at a point in time
//! (`tl_update_values`, `tl_update_values_progress`, `tl_update_values_ease`).

use crate::ease::{BezierHandles, Transition};
use crate::value_rules::interpolate;
use mi_core::types::{ValueType, ValueTypes};
use mi_core::{TlType, ValueId};
use mi_format::project::{Keyframe, Timeline};
use mi_format::ValueSet;

/// Where on the time axis to evaluate, and how the animation loops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playhead {
    /// Position in frames (`timeline_marker`); may be fractional during
    /// playback.
    pub marker: f64,
    /// Blend the end of the loop into its start (`timeline_seamless_repeat`).
    pub seamless_repeat: bool,
    /// The repeated region, if one is set.
    pub region: Option<(f64, f64)>,
    /// Length of the whole animation in frames (`timeline_length`).
    pub length: f64,
}

impl Playhead {
    pub fn at(marker: f64) -> Self {
        Self { marker, seamless_repeat: false, region: None, length: 0.0 }
    }

    fn loop_range(&self) -> (f64, f64) {
        self.region.unwrap_or((0.0, self.length))
    }
}

/// The pair of keyframes a marker lies between.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Index of the keyframe at or before the marker.
    pub current: Option<usize>,
    /// Index of the keyframe after the marker; equals `current` after the
    /// last keyframe.
    pub next: Option<usize>,
    /// Linear progress from `current` to `next`, 0..1.
    pub progress: f64,
}

impl Segment {
    /// Whether the marker is between two different keyframes.
    pub fn animates(&self) -> bool {
        matches!((self.current, self.next), (Some(c), Some(n)) if c != n)
    }
}

/// Finds the keyframes around the playhead. `keyframes` must be sorted by
/// position.
pub fn find_segment(keyframes: &[Keyframe], playhead: &Playhead) -> Segment {
    let marker = playhead.marker;

    // The first keyframe after the marker is "next"; the one before it is
    // "current". Past the last keyframe both are the last keyframe.
    let after = keyframes.iter().position(|k| k.position as f64 > marker);
    let mut current = match after {
        Some(0) => None,
        Some(i) => Some(i - 1),
        None => keyframes.len().checked_sub(1),
    };
    let mut next = after.or(current);

    if !playhead.seamless_repeat {
        let progress = match (current, next) {
            (Some(c), Some(n)) if c != n => {
                let from = keyframes[c].position as f64;
                let to = keyframes[n].position as f64;
                (marker - from) / (to - from)
            }
            _ => 0.0,
        };
        return Segment { current, next, progress };
    }

    // Seamless looping: past the last keyframe of the loop, head for the
    // first one; before the first one, come from the last one.
    let (loop_start, loop_end) = playhead.loop_range();
    let in_loop = |k: &Keyframe| (loop_start..=loop_end).contains(&(k.position as f64));
    let position = |i: usize| keyframes[i].position as f64;
    let mut wraps_forward = false;
    let mut wraps_backward = false;

    if next == current || next.is_some_and(|n| position(n) > loop_end) {
        if let Some(n) = next {
            let first = keyframes.iter().position(|k| in_loop(k) && (k.position as f64) < position(n));
            if first.is_some() {
                next = first;
            }
        }
        wraps_forward = true;
    } else if current.is_none_or(|c| position(c) < loop_start) {
        let last = keyframes
            .iter()
            .enumerate()
            .filter(|(i, k)| in_loop(k) && current.is_none_or(|c| *i > c))
            .map(|(i, _)| i)
            .last();
        if last.is_some() {
            current = last;
        }
        wraps_backward = true;
    }

    let region_size = loop_end - loop_start;
    let progress = match (current, next) {
        (Some(c), Some(n)) if c != n => {
            let mut range = position(n) - position(c);
            let mut marker = marker;
            if wraps_forward {
                range += region_size;
            } else if wraps_backward {
                marker += region_size;
                range += region_size;
            }
            if range != 0.0 {
                (marker - position(c)) / range
            } else {
                0.0
            }
        }
        _ => {
            current = next;
            0.0
        }
    };

    Segment { current, next, progress }
}

/// The values that are animated for a timeline, given the value groups it
/// exposes. Values outside this list always keep the timeline's default.
pub fn animated_values(kind: TlType, types: ValueTypes) -> Vec<ValueId> {
    use ValueId::*;
    let mut ids = vec![Transition, EaseInX, EaseInY, EaseOutX, EaseOutY];
    let mut range = |from: ValueId, to: ValueId| {
        ids.extend(ValueId::ALL[from.index()..=to.index()].iter().copied());
    };

    if types.has(ValueType::TransformPos) {
        range(PosX, PosZ);
        if !matches!(kind, TlType::Path | TlType::PathPoint) {
            range(PathObj, PathOffset);
        }
    }
    if types.has(ValueType::TransformRot) {
        range(RotX, RotZ);
    }
    if types.has(ValueType::TransformSca) {
        range(ScaX, ScaZ);
    }
    if types.has(ValueType::TransformBend) {
        range(BendAngleX, BendAngleZ);
        range(IkTarget, IkAngleOffset);
    }
    if types.has(ValueType::TransformPathPoint) {
        range(PathPointAngle, PathPointScale);
    }
    if types.has(ValueType::MaterialColor) {
        range(Alpha, WindInfluence);
    }
    if types.has(ValueType::Particles) {
        range(Spawn, ForceVortex);
    }
    if types.has(ValueType::Light) {
        range(LightColor, LightFadeSize);
        if types.has(ValueType::Spotlight) {
            range(LightSpotRadius, LightSpotSharpness);
        }
    }
    if types.has(ValueType::Camera) {
        range(CamFov, CamHeight);
        // The lens dirt texture.
        range(TextureObj, TextureObj);
    }
    if types.has(ValueType::Background) {
        range(BgImageShow, BgTextureAniSpeed);
    }
    if types.has(ValueType::MaterialTexture) || types.has(ValueType::Item) {
        range(TextureObj, TextureNormalObj);
    }
    if types.has(ValueType::Sound) {
        range(SoundObj, SoundEnd);
    }
    if types.has(ValueType::Text) {
        range(Text, TextOutlineColor);
    }
    if types.has(ValueType::Item) {
        range(CustomItemSlot, ItemSlot);
    }
    ids.push(Visible);

    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Result of evaluating a timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluated {
    pub values: ValueSet,
    pub segment: Segment,
    /// Progress after applying the transition of the current keyframe.
    pub eased_progress: f64,
}

/// Evaluates all values of `timeline` at the playhead. `has_bend` tells
/// whether the timeline is a body part that can bend.
pub fn evaluate(timeline: &Timeline, playhead: &Playhead, has_bend: bool) -> Evaluated {
    let keyframes = &timeline.keyframes;
    let segment = find_segment(keyframes, playhead);
    let mut values = timeline.default_values.clone();

    let current = segment.current.map(|i| &keyframes[i].values);
    let next = segment.next.map(|i| &keyframes[i].values);

    // The transition and its handles come from the earlier keyframe while
    // animating, otherwise from wherever the other values come from.
    let source = if segment.animates() { current } else { next };
    let transition_source = source.unwrap_or(&timeline.default_values);
    let transition = Transition::from_name(transition_source[ValueId::Transition].as_str().unwrap_or("linear"));
    let handles = BezierHandles {
        ease_in: [transition_source.number(ValueId::EaseInX), transition_source.number(ValueId::EaseInY)],
        ease_out: [transition_source.number(ValueId::EaseOutX), transition_source.number(ValueId::EaseOutY)],
    };
    let eased_progress = transition.ease_with(segment.progress, handles);

    let types = timeline.kind.value_types(has_bend);
    for id in animated_values(timeline.kind, types) {
        values[id] = match (segment.animates(), current, next) {
            (true, Some(from), Some(to)) => interpolate(id, eased_progress, &from[id], &to[id]),
            (_, _, Some(to)) => to[id].clone(),
            _ => continue,
        };
    }

    Evaluated { values, segment, eased_progress }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{Color, SaveId, Value};
    use mi_format::project::Background;

    fn timeline(kind: TlType, frames: &[(i64, f64)]) -> Timeline {
        let defaults = ValueSet::project_defaults(&Background::default(), 0.0, 1.0);
        let mut tl = Timeline::new(SaveId::new("T"), kind, &defaults);
        for &(position, x) in frames {
            let mut values = tl.default_values.clone();
            values[ValueId::PosX] = Value::Number(x);
            tl.keyframes.push(Keyframe { position, values });
        }
        tl
    }

    fn pos_x(tl: &Timeline, marker: f64) -> f64 {
        evaluate(tl, &Playhead::at(marker), false).values.number(ValueId::PosX)
    }

    #[test]
    fn no_keyframes_gives_defaults() {
        let mut tl = timeline(TlType::Cube, &[]);
        tl.default_values[ValueId::PosX] = Value::Number(7.0);
        let result = evaluate(&tl, &Playhead::at(10.0), false);
        assert_eq!(result.values, tl.default_values);
        assert_eq!(result.segment, Segment { current: None, next: None, progress: 0.0 });
    }

    #[test]
    fn before_between_and_after() {
        let tl = timeline(TlType::Cube, &[(10, 100.0), (20, 200.0), (40, 0.0)]);
        // Before the first keyframe its values apply, not the defaults.
        assert_eq!(pos_x(&tl, 0.0), 100.0);
        assert_eq!(pos_x(&tl, 10.0), 100.0);
        assert_eq!(pos_x(&tl, 15.0), 150.0);
        assert_eq!(pos_x(&tl, 17.5), 175.0);
        assert_eq!(pos_x(&tl, 20.0), 200.0);
        assert_eq!(pos_x(&tl, 30.0), 100.0);
        assert_eq!(pos_x(&tl, 40.0), 0.0);
        assert_eq!(pos_x(&tl, 999.0), 0.0);

        let segment = find_segment(&tl.keyframes, &Playhead::at(15.0));
        assert_eq!((segment.current, segment.next), (Some(0), Some(1)));
        assert!(segment.animates());
        let end = find_segment(&tl.keyframes, &Playhead::at(50.0));
        assert_eq!((end.current, end.next), (Some(2), Some(2)));
        assert!(!end.animates());
    }

    #[test]
    fn transition_of_the_earlier_keyframe_is_used() {
        let mut tl = timeline(TlType::Cube, &[(0, 0.0), (10, 100.0), (20, 0.0)]);
        tl.keyframes[0].values[ValueId::Transition] = Value::Str("easeinquad".into());
        tl.keyframes[1].values[ValueId::Transition] = Value::Str("instant".into());
        assert_eq!(pos_x(&tl, 5.0), 25.0);
        assert_eq!(pos_x(&tl, 15.0), 100.0);
        assert_eq!(pos_x(&tl, 19.9), 100.0);
        assert_eq!(pos_x(&tl, 20.0), 0.0);

        let result = evaluate(&tl, &Playhead::at(5.0), false);
        assert_eq!(result.eased_progress, 0.25);
        assert_eq!(result.values[ValueId::Transition], Value::Str("easeinquad".into()));
    }

    #[test]
    fn bezier_transition_reads_its_handles() {
        let mut tl = timeline(TlType::Cube, &[(0, 0.0), (10, 100.0)]);
        let kf = &mut tl.keyframes[0].values;
        kf[ValueId::Transition] = Value::Str("bezier".into());
        kf[ValueId::EaseInX] = Value::Number(0.25);
        kf[ValueId::EaseInY] = Value::Number(0.25);
        kf[ValueId::EaseOutX] = Value::Number(0.75);
        kf[ValueId::EaseOutY] = Value::Number(0.75);
        assert!((pos_x(&tl, 3.0) - 30.0).abs() < 0.2);
    }

    #[test]
    fn colours_blend_and_switches_hold() {
        let mut tl = timeline(TlType::Cube, &[(0, 0.0), (10, 0.0)]);
        tl.keyframes[0].values[ValueId::RgbAdd] = Value::Color(Color::BLACK);
        tl.keyframes[1].values[ValueId::RgbAdd] = Value::Color(Color::rgb(200, 0, 0));
        tl.keyframes[1].values[ValueId::Visible] = Value::Bool(false);
        let mid = evaluate(&tl, &Playhead::at(5.0), false).values;
        assert_eq!(mid[ValueId::RgbAdd], Value::Color(Color::rgb(100, 0, 0)));
        assert_eq!(mid[ValueId::Visible], Value::Bool(true));
        let end = evaluate(&tl, &Playhead::at(10.0), false).values;
        assert_eq!(end[ValueId::Visible], Value::Bool(false));
    }

    #[test]
    fn values_outside_the_timeline_type_stay_at_their_default() {
        // A camera has no scale; keyframed scale is ignored.
        let mut tl = timeline(TlType::Camera, &[(0, 0.0), (10, 10.0)]);
        tl.keyframes[1].values[ValueId::ScaX] = Value::Number(5.0);
        tl.keyframes[1].values[ValueId::CamFov] = Value::Number(90.0);
        let v = evaluate(&tl, &Playhead::at(10.0), false).values;
        assert_eq!(v[ValueId::ScaX], Value::Number(1.0));
        assert_eq!(v[ValueId::CamFov], Value::Number(90.0));

        let cube = animated_values(TlType::Cube, TlType::Cube.value_types(false));
        assert!(cube.contains(&ValueId::ScaX) && cube.contains(&ValueId::TextureObj) && cube.contains(&ValueId::PathObj));
        assert!(!cube.contains(&ValueId::CamFov) && !cube.contains(&ValueId::BendAngleX));
        let part = animated_values(TlType::Bodypart, TlType::Bodypart.value_types(true));
        assert!(part.contains(&ValueId::BendAngleX) && part.contains(&ValueId::IkTarget));
        let audio = animated_values(TlType::Audio, TlType::Audio.value_types(false));
        assert!(audio.contains(&ValueId::SoundObj) && !audio.contains(&ValueId::PosX));
        let path = animated_values(TlType::Path, TlType::Path.value_types(false));
        assert!(!path.contains(&ValueId::PathObj));
    }

    #[test]
    fn seamless_repeat_wraps_around_the_loop() {
        let tl = timeline(TlType::Cube, &[(0, 0.0), (10, 100.0)]);
        let playhead = |marker| Playhead { marker, seamless_repeat: true, region: None, length: 20.0 };

        // Inside the keyframes nothing changes.
        assert_eq!(evaluate(&tl, &playhead(5.0), false).values.number(ValueId::PosX), 50.0);

        // After the last keyframe the animation heads back to the first one
        // over the rest of the loop (frames 10..20).
        let wrapped = evaluate(&tl, &playhead(15.0), false);
        assert_eq!((wrapped.segment.current, wrapped.segment.next), (Some(1), Some(0)));
        assert_eq!(wrapped.segment.progress, 0.5);
        assert_eq!(wrapped.values.number(ValueId::PosX), 50.0);

        // Without seamless repeat it would hold the last keyframe.
        assert_eq!(pos_x(&tl, 15.0), 100.0);
    }

    #[test]
    fn seamless_repeat_before_the_first_keyframe() {
        let tl = timeline(TlType::Cube, &[(10, 0.0), (20, 100.0)]);
        let playhead = Playhead { marker: 5.0, seamless_repeat: true, region: Some((0.0, 30.0)), length: 30.0 };
        // Coming from the last keyframe (20 → 10 across the loop end at 30):
        // 20 frames in total, 15 of them done at marker 5.
        let result = evaluate(&tl, &playhead, false);
        assert_eq!((result.segment.current, result.segment.next), (Some(1), Some(0)));
        assert_eq!(result.segment.progress, 0.75);
        assert_eq!(result.values.number(ValueId::PosX), 25.0);
    }

    #[test]
    fn seamless_repeat_with_a_single_keyframe() {
        let tl = timeline(TlType::Cube, &[(10, 42.0)]);
        let playhead = Playhead { marker: 15.0, seamless_repeat: true, region: None, length: 20.0 };
        assert_eq!(evaluate(&tl, &playhead, false).values.number(ValueId::PosX), 42.0);
    }
}
