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

/// A value to set, by its name in project files (`POS_X`, ...): a number,
/// boolean, `#RRGGBB` colour or text, as the value's kind needs.
#[derive(Debug, Deserialize)]
pub struct ValueEdit {
    name: String,
    value: serde_json::Value,
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
        let edited = Edited { project: summarize(project, state.language(), Vec::new()), frame: frame_state(project, marker, state) };
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

/// Changes values of timelines: of their keyframes among `keyframes`
/// (the selected ones), or else at the current frame. Edits with the same
/// `merge` key in a row are one undo step (a drag).
#[tauri::command]
pub fn set_timeline_values(
    timelines: Vec<String>,
    keyframes: Option<Vec<KeyframeKey>>,
    values: Vec<ValueEdit>,
    mode: EditMode,
    merge: Option<String>,
    state: State<'_, AppState>,
) -> Result<Edited, CommandError> {
    let marker = state.view().marker.round() as i64;
    let values: Vec<(ValueId, Value)> = values
        .iter()
        .filter_map(|v| {
            let id = ValueId::from_name(&v.name)?;
            Some((id, crate::frame_editor::parse_value(id, &v.value)?))
        })
        .collect();
    let mode = match mode {
        EditMode::Set => ValueChange::Set,
        EditMode::Add => ValueChange::Add,
    };
    let selected: Vec<KeyframeRef> = keyframes.iter().flatten().map(KeyframeRef::from).collect();
    let (_, edited) =
        change(&state, |p| p.set_values(&ids(&timelines), &selected, marker, &values, mode, merge.as_deref()))?;
    Ok(edited)
}

#[derive(Debug, Serialize)]
pub struct Keyframes {
    #[serde(flatten)]
    edited: Edited,
    /// The keyframes that were made, to select them.
    keys: Vec<KeyframeKey>,
}

fn keys_of(keys: Vec<KeyframeRef>) -> Vec<KeyframeKey> {
    keys.into_iter().map(|k| KeyframeKey { timeline: k.timeline.to_string(), position: k.position }).collect()
}

/// Adds a keyframe at the current frame to the timelines that have none
/// there.
#[tauri::command]
pub fn create_keyframes(timelines: Vec<String>, state: State<'_, AppState>) -> Result<Keyframes, CommandError> {
    let marker = state.view().marker.round() as i64;
    let (keys, edited) = change(&state, |p| p.create_keyframes(&ids(&timelines), marker))?;
    Ok(Keyframes { edited, keys: keys_of(keys) })
}

/// Copies keyframes, and removes them with `cut`. Returns how many were
/// copied.
#[tauri::command]
pub fn copy_keyframes(keys: Vec<KeyframeKey>, cut: bool, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    let keys: Vec<KeyframeRef> = keys.iter().map(KeyframeRef::from).collect();
    let (clipboard, edited) = change(&state, |p| {
        let clipboard = p.copy_keyframes(&keys);
        if cut {
            p.remove_keyframes(&keys);
        }
        clipboard
    })?;
    *state.clipboard() = clipboard;
    Ok(edited)
}

/// Pastes the copied keyframes with the first one at frame `position`;
/// `timelines` are the selected timelines, which decide where they go.
#[tauri::command]
pub fn paste_keyframes(position: i64, timelines: Vec<String>, state: State<'_, AppState>) -> Result<Keyframes, CommandError> {
    let clipboard = state.clipboard().clone();
    let (keys, edited) = change(&state, |p| p.paste_keyframes(&clipboard, position, &ids(&timelines)))?;
    Ok(Keyframes { edited, keys: keys_of(keys) })
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
pub fn save_project(path: Option<String>, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    let (saved, edited) = change(&state, |p| match &path {
        Some(path) => p.save_as(Path::new(path)),
        None => p.save(),
    })?;
    saved?;
    // As the original does on saving: a picture for the start screen and a
    // place at the top of the recent list.
    crate::commands::write_thumbnail(&state);
    if let Some(project) = state.project().as_ref() {
        crate::commands::remember(&app, project);
    }
    Ok(edited)
}

/// Backups kept next to a project (`setting_backup_amount`).
const BACKUP_AMOUNT: usize = 5;

/// Writes a backup of the open project if it changed since the last one or
/// since it was saved. Returns the file, if one was written.
#[tauri::command]
pub fn backup_project(state: State<'_, AppState>) -> Result<Option<String>, CommandError> {
    let mut guard = state.project();
    let Some(project) = guard.as_mut() else { return Ok(None) };
    Ok(project.backup(BACKUP_AMOUNT)?.map(|path| path.to_string_lossy().into_owned()))
}

/// The newest backup of the open project, if it has one.
#[tauri::command]
pub fn last_backup(state: State<'_, AppState>) -> Option<String> {
    state.project().as_ref()?.last_backup().map(|path| path.to_string_lossy().into_owned())
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
    items: Vec<WorkbenchItem>,
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
    // Items are textures; their names come from the file names.
    items.items = pack
        .manifest()
        .array("item_textures")
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str())
        // The list is the layout of the item sheet, which has empty slots.
        .filter(|name| !name.is_empty())
        .map(|name| {
            let short = name.rsplit('/').next().unwrap_or(name);
            WorkbenchItem { name: name.to_owned(), label: language.asset_name("item", short) }
        })
        .collect();
    for list in [&mut items.characters, &mut items.special_blocks, &mut items.blocks, &mut items.items] {
        list.sort_by_key(|item| item.label.to_lowercase());
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

/// A JSON value from the frontend as the project file reader takes it.
fn file_json(value: &serde_json::Value) -> mi_format::json::Json {
    use mi_format::json::Json;
    match value {
        serde_json::Value::Null => Json::Null,
        serde_json::Value::Bool(b) => Json::Bool(*b),
        serde_json::Value::Number(n) => Json::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Json::String(s.clone()),
        serde_json::Value::Array(list) => Json::Array(list.iter().map(file_json).collect()),
        serde_json::Value::Object(map) => Json::Object(map.iter().map(|(k, v)| (k.clone(), file_json(v))).collect()),
    }
}

fn frontend_json(value: &mi_format::json::Json) -> serde_json::Value {
    use mi_format::json::Json;
    match value {
        Json::Null => serde_json::Value::Null,
        Json::Bool(b) => (*b).into(),
        Json::Number(n) => serde_json::Number::from_f64(*n).map_or(serde_json::Value::Null, serde_json::Value::Number),
        Json::String(s) => s.clone().into(),
        Json::Array(list) => list.iter().map(frontend_json).collect(),
        Json::Object(map) => map.iter().map(|(k, v)| (k.to_owned(), frontend_json(v))).collect::<serde_json::Map<_, _>>().into(),
    }
}

/// Background and render settings as project files store them.
#[derive(Debug, Serialize)]
pub struct Settings {
    background: serde_json::Value,
    render: serde_json::Value,
}

#[tauri::command]
pub fn project_settings(state: State<'_, AppState>) -> Result<Settings, CommandError> {
    let guard = state.project();
    let project = guard.as_ref().ok_or(CommandError::NoProject)?;
    let file = project.file();
    Ok(Settings {
        background: frontend_json(&mi_format::json::Json::Object(file.background.fields_json())),
        render: frontend_json(&mi_format::json::Json::Object(file.render.fields_json())),
    })
}

/// Changes a background (`group: "background"`) or render setting by its
/// key in project files.
#[tauri::command]
pub fn set_setting(
    group: String,
    key: String,
    value: serde_json::Value,
    merge: Option<String>,
    state: State<'_, AppState>,
) -> Result<Edited, CommandError> {
    let value = file_json(&value);
    let (known, edited) = change(&state, |p| match group.as_str() {
        "background" => p.set_background_field(&key, value, merge.as_deref()),
        "render" => p.set_render_field(&key, value, merge.as_deref()),
        _ => false,
    })?;
    if !known {
        return Err(CommandError::Invalid(format!("unknown setting {group}.{key}")));
    }
    Ok(edited)
}

/// Changes a project setting: `name`, `author`, `description`, `tempo` or
/// `video_size` (`[width, height]`).
#[tauri::command]
pub fn set_project_info(field: String, value: serde_json::Value, state: State<'_, AppState>) -> Result<Edited, CommandError> {
    use mi_project::InfoChange;
    let text = || value.as_str().unwrap_or("").to_owned();
    let change_ = match field.as_str() {
        "name" => InfoChange::Name(text()),
        "author" => InfoChange::Author(text()),
        "description" => InfoChange::Description(text()),
        "tempo" => InfoChange::Tempo(value.as_f64().unwrap_or(24.0)),
        "video_size" => {
            let size = value.as_array().map(|a| (a.first().and_then(|v| v.as_f64()), a.get(1).and_then(|v| v.as_f64())));
            match size {
                Some((Some(w), Some(h))) => InfoChange::VideoSize(w, h),
                _ => return Err(CommandError::Invalid("video_size needs [width, height]".into())),
            }
        }
        other => return Err(CommandError::Invalid(format!("unknown project setting {other}"))),
    };
    Ok(change(&state, |p| p.set_project_info(change_))?.1)
}

/// Starts an empty project, which is saved with "save as".
#[tauri::command]
pub fn new_project(state: State<'_, AppState>) -> ProjectSummary {
    let project = Project::new(mi_project::ProjectContext::default());
    let summary = summarize(&project, state.language(), Vec::new());
    let work_camera = crate::scene_builder::saved_work_camera(&project);
    state.set_project(Some(project));
    state.update_view(|view| {
        view.marker = 0.0;
        view.work_camera = work_camera;
    });
    summary
}

/// The frame editor of a timeline: its values at the current frame, in
/// groups.
#[tauri::command]
pub fn timeline_values(id: String, state: State<'_, AppState>) -> Result<Vec<crate::frame_editor::ValueGroup>, CommandError> {
    let marker = state.view().marker;
    let guard = state.project();
    let project = guard.as_ref().ok_or(CommandError::NoProject)?;
    let Some(index) = project.timeline_index(&SaveId::new(&id)) else { return Ok(Vec::new()) };
    let has_bend = state.bindings().as_ref().and_then(|b| b.part_info(index)).is_some_and(|part| part.bend.is_some());
    let (scene, order) = state.evaluate(project, marker);
    let Some(node) = order.iter().position(|&i| i == index) else { return Ok(Vec::new()) };
    Ok(crate::frame_editor::value_groups(project.timelines()[index].kind, has_bend, &scene.nodes[node].values))
}

/// Adds an item drawn from a texture of the asset pack (`item/apple`).
#[tauri::command]
pub fn create_item(name: String, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let pack = state.pack().ok_or_else(|| CommandError::Invalid("the Minecraft assets are not loaded".into()))?;
    if pack.block_texture(&name).is_none() {
        return Err(CommandError::Invalid(format!("unknown item texture {name}")));
    }
    let (id, edited) = change(&state, |p| p.create_item(&name))?;
    Ok(Created { edited, created: vec![id.to_string()] })
}

/// Adds scenery from a schematic (`.schematic`), structure (`.nbt`) or
/// `.blocks` file. The file is read first, so that one that is no scenery
/// adds nothing.
#[tauri::command(async)]
pub fn create_scenery(path: String, state: State<'_, AppState>) -> Result<Created, CommandError> {
    let pack = state.pack().ok_or_else(|| CommandError::Invalid("the Minecraft assets are not loaded".into()))?;
    let file = Path::new(&path);
    let bytes = std::fs::read(file).map_err(|e| CommandError::Invalid(format!("{path}: {e}")))?;
    let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    let empty = mi_assets::LegacyBlocks::empty();
    let legacy = state.legacy().unwrap_or(&empty);
    mi_assets::Scenery::read(&bytes, extension, pack.blocks(), legacy, Default::default())
        .map_err(|e| CommandError::Invalid(format!("{path}: {e}")))?;

    let id = {
        let mut guard = state.project();
        let project = guard.as_mut().ok_or(CommandError::NoProject)?;
        project.create_scenery(file)
    };
    // The viewport needs the blocks before it draws the new timeline.
    state.load_new_scenery();
    let (_, edited) = change(&state, |_| ())?;
    state.refresh_project_assets();
    Ok(Created { edited, created: vec![id.to_string()] })
}

/// Adds a text object with the sample text.
#[tauri::command]
pub fn create_text(state: State<'_, AppState>) -> Result<Created, CommandError> {
    let mut text = state.language().text("timelineeditortextsample", &[]);
    if text.starts_with('<') {
        text = "Sample text".to_owned();
    }
    let (id, edited) = change(&state, |p| p.create_text(&text))?;
    Ok(Created { edited, created: vec![id.to_string()] })
}

/// Settings of a timeline that are not animated, as project files store
/// them. Groups a timeline's type has no use for are absent.
#[derive(Debug, Serialize)]
pub struct TimelineSettings {
    inherit: Option<serde_json::Value>,
    appearance: Option<serde_json::Value>,
    flags: serde_json::Value,
}

#[tauri::command]
pub fn timeline_settings(id: String, state: State<'_, AppState>) -> Result<Option<TimelineSettings>, CommandError> {
    use mi_core::ValueType;
    let guard = state.project();
    let project = guard.as_ref().ok_or(CommandError::NoProject)?;
    let Some(timeline) = project.timeline(&SaveId::new(&id)) else { return Ok(None) };
    let types = timeline.kind.value_types(false);
    let object = |map| frontend_json(&mi_format::json::Json::Object(map));
    Ok(Some(TimelineSettings {
        inherit: types.has(ValueType::Hierarchy).then(|| object(timeline.inherit.fields_json())),
        appearance: types.has(ValueType::Appearance).then(|| object(timeline.appearance.fields_json())),
        flags: serde_json::json!({
            "lock": timeline.lock,
            "lock_bend": timeline.lock_bend,
            "scale_resize": timeline.scale_resize,
            "hq_hiding": timeline.hq_hiding,
            "lq_hiding": timeline.lq_hiding,
            "wind": timeline.wind,
            "wind_terrain": timeline.wind_terrain,
        }),
    }))
}

/// Changes a setting of timelines: `group` is `inherit`, `appearance` or
/// `flags`, `key` the name in project files.
#[tauri::command]
pub fn set_timeline_setting(
    timelines: Vec<String>,
    group: String,
    key: String,
    value: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<Edited, CommandError> {
    use mi_project::TimelineSetting;
    let setting = match group.as_str() {
        "inherit" => TimelineSetting::Inherit,
        "appearance" => TimelineSetting::Appearance,
        "flags" => TimelineSetting::Flag,
        other => return Err(CommandError::Invalid(format!("unknown group of timeline settings {other}"))),
    };
    let value = file_json(&value);
    let (known, edited) = change(&state, |p| p.set_timeline_setting(&ids(&timelines), setting, &key, value))?;
    if !known {
        return Err(CommandError::Invalid(format!("unknown timeline setting {group}.{key}")));
    }
    Ok(edited)
}
