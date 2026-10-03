//! Undo and redo.
//!
//! The original gives every action script its own undo and redo branches.
//! Here an edit records the objects it touches as they were before and
//! after; undo puts the "before" copies back. Edits with the same merge key
//! in a row (dragging a value or keyframes) become one step: each one
//! starts again from the state before the first, so a drag can be applied
//! as "original plus offset" every time.

use crate::Project;
use mi_core::SaveId;
use mi_format::project::{Background, ProjectInfo, RenderSettings, Resource, Template, Timeline};

/// Steps kept for undo (`history_max` in the original is 100).
pub const HISTORY_LIMIT: usize = 100;

/// An object as it was at one point; `None` for a timeline that did not
/// exist.
#[derive(Debug, Clone, PartialEq)]
enum Snapshot {
    Timeline { id: SaveId, index: usize, timeline: Option<Box<Timeline>> },
    Template { id: SaveId, index: usize, template: Option<Box<Template>> },
    Resource { id: SaveId, index: usize, resource: Option<Box<Resource>> },
    Info(Box<ProjectInfo>),
    Background(Box<Background>),
    Render(Box<RenderSettings>),
}

impl Snapshot {
    fn same_object(&self, other: &Snapshot) -> bool {
        match (self, other) {
            (Snapshot::Timeline { id: a, .. }, Snapshot::Timeline { id: b, .. }) => a == b,
            (Snapshot::Template { id: a, .. }, Snapshot::Template { id: b, .. }) => a == b,
            (Snapshot::Resource { id: a, .. }, Snapshot::Resource { id: b, .. }) => a == b,
            (Snapshot::Info(_), Snapshot::Info(_))
            | (Snapshot::Background(_), Snapshot::Background(_))
            | (Snapshot::Render(_), Snapshot::Render(_)) => true,
            _ => false,
        }
    }
}

/// One undoable step.
#[derive(Debug, Clone)]
struct Change {
    label: String,
    merge: Option<String>,
    before: Vec<Snapshot>,
    after: Vec<Snapshot>,
}

/// Undo and redo steps of a project.
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
}

impl History {
    /// What undo would revert, if anything.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|c| c.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|c| c.label.as_str())
    }
}

/// Changes being made to a project. Objects are recorded the first time
/// they are asked for mutably.
pub struct Edit<'p> {
    project: &'p mut Project,
    before: Vec<Snapshot>,
}

impl Edit<'_> {
    /// The project as it is now, for reading.
    pub fn project(&self) -> &Project {
        self.project
    }

    fn record(&mut self, snapshot: impl FnOnce(&Project) -> Snapshot, probe: &Snapshot) {
        if !self.before.iter().any(|s| s.same_object(probe)) {
            let snapshot = snapshot(self.project);
            self.before.push(snapshot);
        }
    }

    /// A timeline to change.
    pub fn timeline(&mut self, id: &SaveId) -> Option<&mut Timeline> {
        let index = self.project.timeline_index(id)?;
        let probe = Snapshot::Timeline { id: id.clone(), index, timeline: None };
        self.record(
            |p| Snapshot::Timeline {
                id: id.clone(),
                index,
                timeline: Some(Box::new(p.file.objects.timelines[index].clone())),
            },
            &probe,
        );
        Some(&mut self.project.file.objects.timelines[index])
    }

    /// Adds a timeline at `index` in the list (the tree order comes from
    /// its parent and tree index).
    pub fn insert_timeline(&mut self, index: usize, timeline: Timeline) {
        let id = timeline.id.clone();
        let probe = Snapshot::Timeline { id: id.clone(), index, timeline: None };
        self.record(|_| Snapshot::Timeline { id: id.clone(), index, timeline: None }, &probe);
        let timelines = &mut self.project.file.objects.timelines;
        timelines.insert(index.min(timelines.len()), timeline);
        self.project.rebuild_indices();
    }

    /// Removes a timeline (not its children).
    pub fn remove_timeline(&mut self, id: &SaveId) -> Option<Timeline> {
        self.timeline(id)?;
        let index = self.project.timeline_index(id)?;
        let removed = self.project.file.objects.timelines.remove(index);
        self.project.rebuild_indices();
        Some(removed)
    }

    /// A template to change.
    pub fn template(&mut self, id: &SaveId) -> Option<&mut Template> {
        let index = self.project.file.objects.templates.iter().position(|t| &t.id == id)?;
        let probe = Snapshot::Template { id: id.clone(), index, template: None };
        self.record(
            |p| Snapshot::Template {
                id: id.clone(),
                index,
                template: Some(Box::new(p.file.objects.templates[index].clone())),
            },
            &probe,
        );
        Some(&mut self.project.file.objects.templates[index])
    }

    /// Adds a template to the library.
    pub fn insert_template(&mut self, template: Template) {
        let id = template.id.clone();
        let index = self.project.file.objects.templates.len();
        let probe = Snapshot::Template { id: id.clone(), index, template: None };
        self.record(|_| Snapshot::Template { id: id.clone(), index, template: None }, &probe);
        self.project.file.objects.templates.push(template);
        self.project.rebuild_indices();
    }

    /// Adds a resource to the project.
    pub fn insert_resource(&mut self, resource: Resource) {
        let id = resource.id.clone();
        let index = self.project.file.objects.resources.len();
        let probe = Snapshot::Resource { id: id.clone(), index, resource: None };
        self.record(|_| Snapshot::Resource { id: id.clone(), index, resource: None }, &probe);
        self.project.file.objects.resources.push(resource);
        self.project.rebuild_indices();
    }

    /// Records where the file of a resource is until the project is saved.
    pub fn set_resource_source(&mut self, id: &SaveId, source: &std::path::Path) {
        self.project.resource_sources.insert(id.clone(), source.to_owned());
    }

    /// A new save id that no object of the project has.
    pub fn new_id(&mut self) -> SaveId {
        self.project.new_id()
    }

    /// Rebuilds the tree, for edits that look at it after moving timelines.
    pub fn refresh_tree(&mut self) {
        self.project.rebuild_tree();
    }

    pub fn info(&mut self) -> &mut ProjectInfo {
        self.record(|p| Snapshot::Info(Box::new(p.file.info.clone())), &Snapshot::Info(Box::default()));
        &mut self.project.file.info
    }

    pub fn background(&mut self) -> &mut Background {
        self.record(
            |p| Snapshot::Background(Box::new(p.file.background.clone())),
            &Snapshot::Background(Box::default()),
        );
        &mut self.project.file.background
    }

    pub fn render(&mut self) -> &mut RenderSettings {
        self.record(
            |p| Snapshot::Render(Box::new(p.file.render.clone())),
            &Snapshot::Render(Box::default()),
        );
        &mut self.project.file.render
    }
}

impl Project {
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Makes changes as one undoable step called `label`. With a merge key
    /// equal to that of the previous step, the previous step is reverted
    /// first and replaced, keeping its starting point.
    pub fn edit<R>(&mut self, label: &str, merge: Option<&str>, change: impl FnOnce(&mut Edit) -> R) -> R {
        let merging = merge.is_some() && self.history.redo.is_empty() && self.history.undo.last().is_some_and(|c| c.merge.as_deref() == merge);
        let mut start = Vec::new();
        if merging {
            let previous = self.history.undo.pop().expect("checked above");
            self.restore(&previous.before);
            start = previous.before;
        }

        let mut edit = Edit { project: self, before: start };
        let result = change(&mut edit);
        let before = edit.before;

        let after: Vec<Snapshot> = before.iter().map(|s| self.capture(s)).collect();
        if before != after || merging {
            self.history.undo.push(Change { label: label.to_owned(), merge: merge.map(str::to_owned), before, after });
            if self.history.undo.len() > HISTORY_LIMIT {
                self.history.undo.remove(0);
            }
            self.history.redo.clear();
            self.after_change();
        }
        result
    }

    /// Reverts the last step. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(change) = self.history.undo.pop() else { return false };
        self.restore(&change.before);
        self.history.redo.push(change);
        self.after_change();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(change) = self.history.redo.pop() else { return false };
        self.restore(&change.after);
        self.history.undo.push(change);
        self.after_change();
        true
    }

    /// Ends merging, so the next edit starts a new step even with the
    /// same key (the end of a drag).
    pub fn finish_edit(&mut self) {
        if let Some(last) = self.history.undo.last_mut() {
            last.merge = None;
        }
    }

    /// The current state of the object a snapshot is of.
    fn capture(&self, snapshot: &Snapshot) -> Snapshot {
        match snapshot {
            Snapshot::Timeline { id, index, .. } => {
                let found = self.timeline_index(id);
                Snapshot::Timeline {
                    id: id.clone(),
                    index: found.unwrap_or(*index),
                    timeline: found.map(|i| Box::new(self.file.objects.timelines[i].clone())),
                }
            }
            Snapshot::Template { id, index, .. } => {
                let found = self.file.objects.templates.iter().position(|t| &t.id == id);
                Snapshot::Template {
                    id: id.clone(),
                    index: found.unwrap_or(*index),
                    template: found.map(|i| Box::new(self.file.objects.templates[i].clone())),
                }
            }
            Snapshot::Resource { id, index, .. } => {
                let found = self.file.objects.resources.iter().position(|r| &r.id == id);
                Snapshot::Resource {
                    id: id.clone(),
                    index: found.unwrap_or(*index),
                    resource: found.map(|i| Box::new(self.file.objects.resources[i].clone())),
                }
            }
            Snapshot::Info(_) => Snapshot::Info(Box::new(self.file.info.clone())),
            Snapshot::Background(_) => Snapshot::Background(Box::new(self.file.background.clone())),
            Snapshot::Render(_) => Snapshot::Render(Box::new(self.file.render.clone())),
        }
    }

    fn restore(&mut self, snapshots: &[Snapshot]) {
        for snapshot in snapshots {
            match snapshot {
                Snapshot::Timeline { id, index, timeline } => {
                    let timelines = &mut self.file.objects.timelines;
                    let current = timelines.iter().position(|t| &t.id == id);
                    match (current, timeline) {
                        (Some(i), Some(t)) => timelines[i] = (**t).clone(),
                        (Some(i), None) => {
                            timelines.remove(i);
                        }
                        (None, Some(t)) => timelines.insert((*index).min(timelines.len()), (**t).clone()),
                        (None, None) => {}
                    }
                    // Indices shift when timelines come and go.
                    self.rebuild_indices();
                }
                Snapshot::Template { id, index, template } => {
                    let templates = &mut self.file.objects.templates;
                    let current = templates.iter().position(|t| &t.id == id);
                    match (current, template) {
                        (Some(i), Some(t)) => templates[i] = (**t).clone(),
                        (Some(i), None) => {
                            templates.remove(i);
                        }
                        (None, Some(t)) => templates.insert((*index).min(templates.len()), (**t).clone()),
                        (None, None) => {}
                    }
                    self.rebuild_indices();
                }
                Snapshot::Resource { id, index, resource } => {
                    let resources = &mut self.file.objects.resources;
                    let current = resources.iter().position(|r| &r.id == id);
                    match (current, resource) {
                        (Some(i), Some(r)) => resources[i] = (**r).clone(),
                        (Some(i), None) => {
                            resources.remove(i);
                        }
                        (None, Some(r)) => resources.insert((*index).min(resources.len()), (**r).clone()),
                        (None, None) => {}
                    }
                    self.rebuild_indices();
                }
                Snapshot::Info(info) => self.file.info = (**info).clone(),
                Snapshot::Background(background) => self.file.background = (**background).clone(),
                Snapshot::Render(render) => self.file.render = (**render).clone(),
            }
        }
    }

    fn after_change(&mut self) {
        self.rebuild_indices();
        self.rebuild_tree();
        self.changed = true;
    }
}
