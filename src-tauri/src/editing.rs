//! Commands that change the open project. Each returns the project and the
//! current frame as they are afterwards, so the frontend can redraw from
//! one answer.

use crate::commands::{frame_state, summarize, CommandError, FrameState, ProjectSummary};
use crate::state::AppState;
use mi_core::{SaveId, Value, ValueId};
use mi_project::{KeyframeRef, Project, ValueChange};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edited {
    project: ProjectSummary,
    frame: FrameState,
}

/// A keyframe as the frontend names it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeyframeKey {
    timeline: String,
    position: i64,
}

impl From<&KeyframeKey> for KeyframeRef {
    fn from(key: &KeyframeKey) -> Self {
        KeyframeRef { timeline: SaveId::new(&key.timeline), position: key.position }
    }
}

/// A number value to set, by its name in project files (`POS_X`, ...).
#[derive(Debug, Deserialize)]
pub struct NumberEdit {
    name: String,
    value: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    Set,
    Add,
}

/// Runs a change on the open project and redraws the viewport.
fn change<R>(state: &AppState, apply: impl FnOnce(&mut Project) -> R) -> Result<(R, Edited), CommandError> {
    let marker = state.view().marker;
    let (result, edited, reshaped) = {
        let mut guard = state.project();
        let project = guard.as_mut().ok_or(CommandError::NoProject)?;
        let ids: Vec<SaveId> = project.timelines().iter().map(|t| t.id.clone()).collect();
        let result = apply(project);
        let reshaped = project.timelines().len() != ids.len() || project.timelines().iter().zip(&ids).any(|(t, id)| &t.id != id);
        let edited = Edited { project: summarize(project, state.language(), Vec::new()), frame: frame_state(project, marker) };
        (result, edited, reshaped)
    };
    // Models and scenery are bound by timeline position.
    if reshaped {
        state.refresh_project_assets();
    }
    state.redraw();
    Ok((result, edited))
}

fn ids(list: &[String]) -> Vec<SaveId> {
    list.iter().map(SaveId::new).collect()
}

/// Changes number values of timelines at the current frame. Edits with the
/// same `merge` key in a row are one undo step (a drag).
#[tauri::command]
pub fn set_timeline_values(
    timelines: Vec<String>,
    values: Vec<NumberEdit>,
    mode: EditMode,
    merge: Option<String>,
    state: State<'_, AppState>,
) -> Result<Edited, CommandError> {
    let marker = state.view().marker.round() as i64;
    let values: Vec<(ValueId, Value)> =
        values.iter().filter_map(|v| Some((ValueId::from_name(&v.name)?, Value::Number(v.value)))).collect();
    let mode = match mode {
        EditMode::Set => ValueChange::Set,
        EditMode::Add => ValueChange::Add,
    };
    let (_, edited) =
        change(&state, |p| p.set_values(&ids(&timelines), marker, &values, mode, merge.as_deref()))?;
    Ok(edited)
}

/// Ends a drag, so the next edit is a step of its own.
#[tauri::command]
pub fn finish_edit(state: State<'_, AppState>) {
    if let Some(project) = state.project().as_mut() {
        project.finish_edit();
    }
}

#[derive(Debug, Serialize)]
pub struct Moved {
    #[serde(flatten)]
    edited: Edited,
    /// Where the keyframes are now, for keeping them selected.
    moved: Vec<KeyframeKey>,
}

/// Moves keyframes by a number of frames from where they were when the
/// drag started.
#[tauri::command]
pub fn move_keyframes(
    keys: Vec<KeyframeKey>,
    offset: i64,
    merge: Option<String>,
    state: State<'_, AppState>,
) -> Result<Moved, CommandError> {
    let from: Vec<KeyframeRef> = keys.iter().map(KeyframeRef::from).collect();
    let (moved, edited) = change(&state, |p| p.move_keyframes(&from, offset, merge.as_deref()))?;
    let moved = moved.into_iter().map(|k| KeyframeKey { timeline: k.timeline.to_string(), position: k.position }).collect();
    Ok(Moved { edited, moved })
}

#[tauri::command]
pub fn remove_keyframes(keys: Vec<KeyframeKey>, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    let keys: Vec<KeyframeRef> = keys.iter().map(KeyframeRef::from).collect();
    Ok(change(&state, |p| p.remove_keyframes(&keys))?.1)
}

#[tauri::command]
pub fn rename_timeline(id: String, name: String, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    Ok(change(&state, |p| p.rename_timeline(&SaveId::new(&id), &name))?.1)
}

#[tauri::command]
pub fn set_timelines_hidden(timelines: Vec<String>, hidden: bool, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    Ok(change(&state, |p| p.set_hidden(&ids(&timelines), hidden))?.1)
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> Result<Edited, CommandError> {
    Ok(change(&state, |p| p.undo())?.1)
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> Result<Edited, CommandError> {
    Ok(change(&state, |p| p.redo())?.1)
}

/// Saves the project to its file, or to `path` when given (save as).
#[tauri::command]
pub fn save_project(path: Option<String>, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    let (saved, edited) = change(&state, |p| match &path {
        Some(path) => p.save_as(Path::new(path)),
        None => p.save(),
    })?;
    saved?;
    Ok(edited)
}
