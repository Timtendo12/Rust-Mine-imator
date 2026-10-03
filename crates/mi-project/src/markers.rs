//! The playback region, the repeat mode and the markers of the timeline
//! (`action_tl_play_repeat`, the region dragging of `tab_timeline`,
//! `action_tl_marker_new`, `action_tl_marker_pos`, `action_tl_marker_edit`,
//! `action_tl_marker_delete`).

use crate::Project;
use mi_core::SaveId;
use mi_format::project::Marker;

/// How playback goes on at the end (`timeline_repeat`,
/// `timeline_seamless_repeat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// It plays on past the end.
    None,
    /// It starts again from the beginning.
    Repeat,
    /// Like `Repeat`, and the animation is evaluated so that its end runs
    /// into its start.
    Seamless,
}

/// What to change of a marker; `None` leaves a part as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkerChange {
    pub position: Option<i64>,
    pub name: Option<String>,
    /// Index of the colour tag.
    pub color: Option<u32>,
}

/// Number of colour tags a marker can have.
pub const MARKER_COLORS: u32 = 9;

impl Project {
    pub fn repeat(&self) -> Repeat {
        let timeline = &self.file.info.timeline;
        if timeline.seamless_repeat {
            Repeat::Seamless
        } else if timeline.repeat {
            Repeat::Repeat
        } else {
            Repeat::None
        }
    }

    /// Steps to the next repeat mode: none, repeat, seamless, none. Like
    /// the region, this is a setting of the project that is not undone.
    pub fn cycle_repeat(&mut self) -> Repeat {
        let next = match self.repeat() {
            Repeat::None => Repeat::Repeat,
            Repeat::Repeat => Repeat::Seamless,
            Repeat::Seamless => Repeat::None,
        };
        let timeline = &mut self.file.info.timeline;
        timeline.repeat = next == Repeat::Repeat;
        timeline.seamless_repeat = next == Repeat::Seamless;
        self.changed = true;
        self.unsaved_backup = true;
        next
    }

    /// The part of the timeline that is played and exported, in frames.
    pub fn region(&self) -> Option<(i64, i64)> {
        let timeline = &self.file.info.timeline;
        Some((timeline.region_start? as i64, timeline.region_end? as i64))
    }

    /// Sets the region to the frames between `a` and `b`, in either order;
    /// an empty one, or `None`, removes it.
    pub fn set_region(&mut self, region: Option<(i64, i64)>) {
        let region = region.map(|(a, b)| (a.min(b).max(0), a.max(b).max(0))).filter(|(start, end)| start != end);
        if region == self.region() {
            return;
        }
        let timeline = &mut self.file.info.timeline;
        timeline.region_start = region.map(|r| r.0 as f64);
        timeline.region_end = region.map(|r| r.1 as f64);
        self.changed = true;
        self.unsaved_backup = true;
    }

    pub fn markers(&self) -> &[Marker] {
        &self.file.markers
    }

    /// Adds a marker called `name` at a frame. Returns its id.
    pub fn add_marker(&mut self, position: i64, name: &str) -> SaveId {
        self.edit("Add marker", None, |edit| {
            let id = edit.new_id();
            let marker = Marker { id: id.clone(), position: position.max(0) as f64, name: name.to_owned(), color: 0.0 };
            let markers = edit.markers();
            markers.push(marker);
            sort(markers);
            id
        })
    }

    /// Moves, renames or recolours a marker. Returns whether it exists.
    pub fn edit_marker(&mut self, id: &SaveId, change: MarkerChange, merge: Option<&str>) -> bool {
        if !self.file.markers.iter().any(|m| &m.id == id) {
            return false;
        }
        self.edit("Edit marker", merge, |edit| {
            let markers = edit.markers();
            if let Some(marker) = markers.iter_mut().find(|m| &m.id == id) {
                if let Some(position) = change.position {
                    marker.position = position.max(0) as f64;
                }
                if let Some(name) = change.name {
                    marker.name = name;
                }
                if let Some(color) = change.color {
                    marker.color = color.min(MARKER_COLORS - 1) as f64;
                }
            }
            sort(markers);
        });
        true
    }

    pub fn remove_marker(&mut self, id: &SaveId) {
        self.edit("Delete marker", None, |edit| edit.markers().retain(|m| &m.id != id));
    }
}

/// `marker_list_sort`: by position; markers on one frame keep their order.
fn sort(markers: &mut [Marker]) {
    markers.sort_by(|a, b| a.position.total_cmp(&b.position));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectContext;

    #[test]
    fn the_repeat_mode_cycles_and_regions_are_ordered() {
        let mut project = Project::new(ProjectContext::default());
        assert_eq!((project.repeat(), project.region()), (Repeat::None, None));
        assert_eq!(project.cycle_repeat(), Repeat::Repeat);
        assert_eq!(project.cycle_repeat(), Repeat::Seamless);
        assert!(project.file().info.timeline.seamless_repeat && !project.file().info.timeline.repeat);
        assert_eq!(project.cycle_repeat(), Repeat::None);
        assert!(project.is_changed());

        project.set_region(Some((40, 10)));
        assert_eq!(project.region(), Some((10, 40)));
        // The playhead the animation is evaluated with knows the region.
        assert_eq!(project.playhead(0.0).region, Some((10.0, 40.0)));
        project.set_region(Some((-5, 20)));
        assert_eq!(project.region(), Some((0, 20)));
        // An empty region is no region.
        project.set_region(Some((7, 7)));
        assert_eq!(project.region(), None);
        // These are not undo steps.
        assert!(!project.undo());
    }

    #[test]
    fn markers_stay_sorted_and_are_undone() {
        let mut project = Project::new(ProjectContext::default());
        let late = project.add_marker(50, "Late");
        let early = project.add_marker(10, "Early");
        let names = |p: &Project| p.markers().iter().map(|m| m.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&project), ["Early", "Late"]);

        let change = MarkerChange { position: Some(80), name: Some("Moved".into()), color: Some(99) };
        assert!(project.edit_marker(&early, change, None));
        assert_eq!(names(&project), ["Late", "Moved"]);
        assert_eq!(project.markers()[1].color, (MARKER_COLORS - 1) as f64);
        assert!(!project.edit_marker(&SaveId::new("NOPE"), MarkerChange::default(), None));

        // A drag is one step from where it started.
        for position in [20, 30, 40] {
            project.edit_marker(&late, MarkerChange { position: Some(position), ..Default::default() }, Some("drag"));
        }
        project.finish_edit();
        assert_eq!(project.markers()[0].position, 40.0);
        project.undo();
        assert_eq!(project.markers()[0].position, 50.0);

        project.remove_marker(&late);
        assert_eq!(names(&project), ["Moved"]);
        project.undo();
        assert_eq!(names(&project), ["Late", "Moved"]);
        project.undo();
        project.undo();
        project.undo();
        assert!(project.markers().is_empty());
    }

    #[test]
    fn markers_and_the_region_are_saved() {
        let dir = std::env::temp_dir().join(format!("mi-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.miproject");
        let mut project = Project::new(ProjectContext::default());
        project.add_marker(12, "Jump");
        project.set_region(Some((5, 30)));
        project.cycle_repeat();
        project.save_as(&path).unwrap();

        let (opened, _) = Project::open(&path, ProjectContext::default()).unwrap();
        assert_eq!(opened.markers().len(), 1);
        assert_eq!((opened.markers()[0].name.as_str(), opened.markers()[0].position), ("Jump", 12.0));
        assert_eq!((opened.region(), opened.repeat()), (Some((5, 30)), Repeat::Repeat));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
