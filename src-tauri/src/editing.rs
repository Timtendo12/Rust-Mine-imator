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

#[derive(Debug, Serialize)]
pub struct Created {
    #[serde(flatten)]
    edited: Edited,
    /// The new timelines, to select them.
    created: Vec<String>,
}

/// Adds a folder, camera, light or shape at the end of the timeline list.
/// Cameras start where the work camera is.
#[tauri::command]
pub fn create_timeline(kind: String, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let kind = mi_core::TlType::from_name(&kind).ok_or_else(|| CommandError::Invalid(format!("unknown timeline type {kind}")))?;
    let work = state.view().work_camera;
    let position = work.position();
    let pose = mi_project::CameraPose {
        position: [position.x as f64, position.y as f64, position.z as f64],
        look_xy: work.look_xy as f64,
        look_z: work.look_z as f64,
        roll: work.roll as f64,
    };
    let (id, edited) = change(&state, |p| p.create_timeline(kind, Some(pose)))?;
    Ok(Created { edited, created: id.into_iter().map(|i| i.to_string()).collect() })
}

/// Removes timelines and everything below them.
#[tauri::command]
pub fn remove_timelines(timelines: Vec<String>, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    Ok(change(&state, |p| p.remove_timelines(&ids(&timelines)))?.1)
}

/// Copies timelines with everything below them.
#[tauri::command]
pub fn duplicate_timelines(timelines: Vec<String>, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let (copies, edited) = change(&state, |p| p.duplicate_timelines(&ids(&timelines)))?;
    Ok(Created { edited, created: copies.iter().map(|i| i.to_string()).collect() })
}

/// Moves timelines under `parent` (the root when absent), at `index` among
/// its children or at the end.
#[tauri::command]
pub fn reparent_timelines(
    timelines: Vec<String>,
    parent: Option<String>,
    index: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Edited, CommandError> {
    let parent = parent.map(SaveId::new);
    Ok(change(&state, |p| p.reparent_timelines(&ids(&timelines), parent.as_ref(), index))?.1)
}

/// Something the workbench can create.
#[derive(Debug, Serialize)]
pub struct WorkbenchItem {
    name: String,
    /// Name in the user's language.
    label: String,
}

/// What the workbench offers from the asset pack, sorted by label.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchItems {
    characters: Vec<WorkbenchItem>,
    special_blocks: Vec<WorkbenchItem>,
    blocks: Vec<WorkbenchItem>,
}

#[tauri::command]
pub fn workbench_items(state: State<'_, AppState>) -> WorkbenchItems {
    let Some(pack) = state.pack() else { return WorkbenchItems::default() };
    let language = state.language();
    let mut items = WorkbenchItems::default();
    for name in pack.model_names() {
        let Some(def) = pack.model(name) else { continue };
        let item = WorkbenchItem { name: name.clone(), label: language.asset_name("model", name) };
        if def.folder == "character" {
            items.characters.push(item);
        } else {
            items.special_blocks.push(item);
        }
    }
    let blocks = pack.blocks();
    items.blocks = blocks.names().map(|n| WorkbenchItem { name: n.to_owned(), label: language.asset_name("block", n) }).collect();
    for list in [&mut items.characters, &mut items.special_blocks, &mut items.blocks] {
        list.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    }
    items
}

/// Adds a character or special block of the asset pack in its default state.
#[tauri::command]
pub fn create_model(name: String, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let pack = state.pack().ok_or_else(|| CommandError::Invalid("the Minecraft assets are not loaded".into()))?;
    let def = pack.model(&name).ok_or_else(|| CommandError::Invalid(format!("unknown model {name}")))?;
    let kind = if def.folder == "character" { mi_core::TlType::Character } else { mi_core::TlType::SpecialBlock };
    let model_state: Vec<(String, mi_format::StateValue)> =
        def.default_state.iter().map(|(k, v)| (k.clone(), mi_format::StateValue::Str(v.clone()))).collect();
    let resolved = pack.resolve(&name, &model_state).ok_or_else(|| CommandError::Invalid(format!("model {name} could not be loaded")))?;
    let (id, edited) = change(&state, |p| p.create_model(kind, &name, model_state, &resolved.file, &resolved.hide))?;
    Ok(Created { edited, created: id.into_iter().map(|i| i.to_string()).collect() })
}

/// Adds a block of the asset pack in its default state.
#[tauri::command]
pub fn create_block(name: String, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let pack = state.pack().ok_or_else(|| CommandError::Invalid("the Minecraft assets are not loaded".into()))?;
    let def = pack.blocks().def(&name).ok_or_else(|| CommandError::Invalid(format!("unknown block {name}")))?;
    let block_state: Vec<(String, mi_format::StateValue)> =
        def.default_state.iter().map(|(k, v)| (k.clone(), mi_format::StateValue::Str(v.clone()))).collect();
    let (id, edited) = change(&state, |p| p.create_block(&name, block_state))?;
    Ok(Created { edited, created: vec![id.to_string()] })
}
