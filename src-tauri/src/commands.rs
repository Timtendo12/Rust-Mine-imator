//! Commands callable from the frontend.

use crate::scene_builder::{saved_work_camera, ViewMode};
use crate::state::AppState;
use mi_core::{version, TlType};
use mi_project::{Project, ProjectContext, ProjectError};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

/// Error returned to the frontend; shown to the user as is.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("No project is open.")]
    NoProject,
    #[error("{0}")]
    Invalid(String),
    #[error("Could not write {path}: {reason}")]
    Write { path: String, reason: String },
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Version of this program.
    version: &'static str,
    /// Version of the original program whose behaviour is being matched.
    tracks_version: &'static str,
    minecraft_version: &'static str,
    project_format: i32,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        tracks_version: version::MINEIMATOR_VERSION,
        minecraft_version: version::MINECRAFT_VERSION,
        project_format: version::project::CURRENT,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSummary {
    id: String,
    name: String,
    kind: &'static str,
    /// Number of ancestors.
    depth: usize,
    /// Parent in the tree; absent at the root.
    parent: Option<String>,
    /// Position among the parent's children.
    index: usize,
    /// Part of a model or scenery, which cannot be moved or removed on its own.
    part: bool,
    /// Frames that have a keyframe.
    keyframes: Vec<i64>,
    /// For audio timelines: how many frames the sound of each keyframe
    /// lasts (0 while it is not known). Empty for other timelines.
    lengths: Vec<f64>,
    hidden: bool,
}

/// Background settings shown in the environment section.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    sky_time: f64,
    sky_rotation: f64,
    biome: String,
    sky_color: String,
    clouds_color: String,
    sunlight_color: String,
    ambient_color: String,
    night_color: String,
    twilight: bool,
    clouds_show: bool,
    ground_show: bool,
    fog_show: bool,
    wind: bool,
    texture_animation_speed: f64,
}

/// A marker of the timeline.
#[derive(Debug, Serialize)]
pub struct MarkerSummary {
    id: String,
    position: f64,
    name: String,
    /// Index of its colour tag.
    color: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    path: Option<String>,
    /// Name of the render settings preset, empty for custom settings.
    render_settings: String,
    render_samples: f64,
    environment: EnvironmentSummary,
    name: String,
    author: String,
    description: String,
    created_in: String,
    format: i32,
    tempo: f64,
    video_width: f64,
    video_height: f64,
    /// Last frame that has a keyframe.
    length: i64,
    /// Frame the project was saved at.
    marker: f64,
    templates: usize,
    resources: usize,
    markers: usize,
    /// The markers of the timeline, by position.
    marker_list: Vec<MarkerSummary>,
    /// `none`, `repeat` or `seamless`.
    repeat: &'static str,
    /// First and last frame of the region that is played and exported.
    region: Option<[i64; 2]>,
    cameras: usize,
    /// Timelines in tree order.
    timelines: Vec<TimelineSummary>,
    warnings: Vec<String>,
    /// Unsaved changes.
    changed: bool,
    /// What undo and redo would do.
    undo: Option<String>,
    redo: Option<String>,
}

pub(crate) fn summarize(project: &Project, language: &mi_format::language::Language, warnings: Vec<String>) -> ProjectSummary {
    let file = project.file();
    let timelines = project.timelines();
    let background = &file.background;
    ProjectSummary {
        path: project.path().map(|p| p.to_string_lossy().into_owned()),
        render_settings: file.info.render_settings.clone(),
        render_samples: file.render.samples,
        environment: EnvironmentSummary {
            sky_time: background.sky_time,
            sky_rotation: background.sky_rotation,
            biome: background.biome.clone(),
            sky_color: background.sky_color.to_hex(),
            clouds_color: background.sky_clouds_color.to_hex(),
            sunlight_color: background.sunlight_color.to_hex(),
            ambient_color: background.ambient_color.to_hex(),
            night_color: background.night_color.to_hex(),
            twilight: background.twilight,
            clouds_show: background.sky_clouds_show,
            ground_show: background.ground_show,
            fog_show: background.fog_show,
            wind: background.wind,
            texture_animation_speed: background.texture_animation_speed,
        },
        name: file.info.name.clone(),
        author: file.info.author.clone(),
        description: file.info.description.clone(),
        created_in: file.created_in.clone(),
        format: file.loaded_format,
        tempo: file.info.tempo,
        video_width: file.info.video_width,
        video_height: file.info.video_height,
        length: project.length(),
        marker: file.info.timeline.marker,
        templates: project.templates().len(),
        resources: project.resources().len(),
        markers: file.markers.len(),
        marker_list: file
            .markers
            .iter()
            .map(|m| MarkerSummary { id: m.id.to_string(), position: m.position, name: m.name.clone(), color: m.color })
            .collect(),
        repeat: match project.repeat() {
            mi_project::Repeat::None => "none",
            mi_project::Repeat::Repeat => "repeat",
            mi_project::Repeat::Seamless => "seamless",
        },
        region: project.region().map(|(start, end)| [start, end]),
        cameras: timelines.iter().filter(|tl| tl.kind == TlType::Camera).count(),
        timelines: project
            .tree()
            .order()
            .iter()
            .map(|&i| {
                let tl = &timelines[i];
                TimelineSummary {
                    id: tl.id.to_string(),
                    name: project.timeline_display_name(tl, language),
                    kind: tl.kind.name(),
                    depth: project.tree().depth(i),
                    parent: project.tree().parent(i).map(|p| timelines[p].id.to_string()),
                    index: project.tree().index_in_parent(i),
                    part: !tl.part_of.is_null(),
                    keyframes: tl.keyframes.iter().map(|k| k.position).collect(),
                    lengths: if tl.kind == TlType::Audio {
                        tl.keyframes.iter().map(|k| project.keyframe_length(tl, k)).collect()
                    } else {
                        Vec::new()
                    },
                    hidden: tl.hide,
                }
            })
            .collect(),
        warnings,
        changed: project.is_changed(),
        undo: project.history().undo_label().map(str::to_owned),
        redo: project.history().redo_label().map(str::to_owned),
    }
}

/// The project file the application was started with, if any.
#[tauri::command]
pub fn startup_project(state: State<'_, AppState>) -> Option<String> {
    state.take_startup_path()
}

/// Folders the recent list is read from and written to: the application's
/// data folder and the user's home folder (for an installation of the
/// original program).
fn recent_dirs(app: &AppHandle) -> (Option<std::path::PathBuf>, Option<std::path::PathBuf>) {
    (app.path().app_data_dir().ok(), app.path().home_dir().ok())
}

/// The recent projects for the startup screen.
#[tauri::command]
pub fn recent_projects(app: AppHandle) -> Vec<crate::recent::RecentItem> {
    let (data, home) = recent_dirs(&app);
    match data {
        Some(data) => crate::recent::items(&crate::recent::load(&data, home.as_deref())),
        None => Vec::new(),
    }
}

/// Removes a project from the recent list (the file itself is not touched).
#[tauri::command]
pub fn forget_recent_project(filename: String, app: AppHandle) -> Vec<crate::recent::RecentItem> {
    let (data, home) = recent_dirs(&app);
    let Some(data) = data else { return Vec::new() };
    let mut list = crate::recent::load(&data, home.as_deref());
    if list.remove(&filename) {
        if let Err(error) = crate::recent::save(&data, &list) {
            eprintln!("Could not save the recent list: {error}");
        }
    }
    crate::recent::items(&list)
}

/// Closes the current project and returns to the startup screen.
#[tauri::command]
pub fn close_project(state: State<'_, AppState>) {
    state.set_project(None);
    state.redraw();
}

/// Puts a project at the top of the recent list (`recent_add`). Failing to
/// do so is not a reason to refuse opening or saving the project.
pub(crate) fn remember(app: &AppHandle, project: &Project) {
    if let (Some(entry), (Some(data), home)) = (crate::recent::entry_for(project), recent_dirs(app)) {
        let mut list = crate::recent::load(&data, home.as_deref());
        list.add(entry);
        if let Err(error) = crate::recent::save(&data, &list) {
            eprintln!("Could not save the recent list: {error}");
        }
    }
}

/// Size of the picture of a project on the start screen
/// (`recent_thumbnail_width`, `recent_thumbnail_height`).
const THUMBNAIL_SIZE: (u32, u32) = (240, 180);

/// Writes `thumbnail.png` next to the project file: the work camera's view
/// in the low quality renderer, as the original does when saving.
pub(crate) fn write_thumbnail(state: &AppState) {
    let Some(folder) = state.project().as_ref().and_then(|p| p.folder().map(std::path::Path::to_owned)) else { return };
    let Some(viewport) = state.viewport() else { return };
    let (width, height) = THUMBNAIL_SIZE;
    let Some(pixels) = viewport.render_image(width, height, false, false) else { return };
    let path = folder.join("thumbnail.png");
    if let Err(error) = image::save_buffer(&path, &pixels, width, height, image::ColorType::Rgba8) {
        eprintln!("Could not write {}: {error}", path.display());
    }
}

/// Renders the current frame at the project's video size and saves it as an
/// image (`action_toolbar_exportimage_save`), through the active camera if
/// the project has one.
#[tauri::command]
pub fn export_image(path: String, high_quality: bool, state: State<'_, AppState>) -> Result<(), CommandError> {
    let (width, height) = {
        let guard = state.project();
        let info = &guard.as_ref().ok_or(CommandError::NoProject)?.file().info;
        (info.video_width.max(1.0) as u32, info.video_height.max(1.0) as u32)
    };
    let failed = |reason: String| CommandError::Write { path: path.clone(), reason };
    let viewport = state.viewport().ok_or_else(|| failed("the viewport is not running".into()))?;
    let pixels = viewport.render_image(width, height, true, high_quality).ok_or_else(|| failed("rendering failed".into()))?;
    image::save_buffer(&path, &pixels, width, height, image::ColorType::Rgba8).map_err(|e| failed(e.to_string()))
}

/// How far an export is, sent to the frontend as `export-progress`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportProgress {
    frame: usize,
    total: usize,
}

/// The choices of the export dialog.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovieRequest {
    /// `mp4`, `mov`, `wmv` or `png`.
    format: String,
    frames_per_second: f64,
    bit_rate: u64,
    include_audio: bool,
    high_quality: bool,
}

/// What an export made.
#[derive(Debug, Serialize)]
pub struct Exported {
    frames: usize,
    cancelled: bool,
}

/// Renders the animation at the project's video size through the active
/// camera and saves it as a video or as numbered images
/// (`action_toolbar_exportmovie_save`). Runs off the main thread; the
/// viewport shows the frames as they are made.
#[tauri::command(async)]
pub fn export_movie(path: String, options: MovieRequest, app: AppHandle, state: State<'_, AppState>) -> Result<Exported, CommandError> {
    let MovieRequest { format, frames_per_second, bit_rate, include_audio, high_quality } = options;
    use crate::export::{self, Format, MovieOptions, Outcome};
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    let failed = |reason: String| CommandError::Write { path: path.clone(), reason };
    let format = Format::from_name(&format).ok_or_else(|| CommandError::Invalid(format!("unknown export format {format}")))?;
    let mut options =
        MovieOptions { format, frames_per_second: frames_per_second.clamp(1.0, 120.0), bit_rate: bit_rate.max(1), audio: None };
    // The region is exported if there is one, else everything.
    let (size, tempo, start, end) = {
        let guard = state.project();
        let project = guard.as_ref().ok_or(CommandError::NoProject)?;
        let info = &project.file().info;
        let (start, end) = project.region().unwrap_or((0, project.length()));
        ((info.video_width.max(1.0) as u32, info.video_height.max(1.0) as u32), info.tempo.max(1.0), start as f64, end as f64)
    };
    // The sounds of the exported stretch, mixed into a file for the encoder.
    let mut mix_file = None;
    if include_audio && format != Format::Png {
        let guard = state.project();
        let project = guard.as_ref().ok_or(CommandError::NoProject)?;
        let sounds = state.sounds();
        if sounds.any(project) {
            let file = std::env::temp_dir().join(format!("mine-imator-export-{}.wav", std::process::id()));
            let wav = mi_audio::wav_bytes(&sounds.mix(project, start, end, false));
            std::fs::write(&file, wav).map_err(|e| failed(format!("the sound could not be prepared: {e}")))?;
            options.audio = Some(file.clone());
            mix_file = Some(file);
        }
    }
    let viewport = state.viewport().ok_or_else(|| failed("the viewport is not running".into()))?;
    let markers = export::frame_markers(start, end, tempo, options.frames_per_second);
    let sequence_total = export::sequence_total(start, end, tempo, options.frames_per_second);
    let total = markers.len();

    let previous = state.view().marker;
    state.cancel_export().store(false, Ordering::Relaxed);
    let mut done = 0;
    let result = export::export(
        &options,
        Path::new(&path),
        size,
        &markers,
        sequence_total,
        |marker| {
            state.update_view(|view| view.marker = marker);
            viewport.render_image(size.0, size.1, true, high_quality)
        },
        |frame| {
            done = frame;
            let _ = app.emit("export-progress", ExportProgress { frame, total });
            !state.cancel_export().load(Ordering::Relaxed)
        },
    );
    state.update_view(|view| view.marker = previous);
    state.redraw();
    if let Some(file) = mix_file {
        let _ = std::fs::remove_file(file);
    }
    match result.map_err(|e| failed(e.to_string()))? {
        Outcome::Done => Ok(Exported { frames: total, cancelled: false }),
        Outcome::Cancelled => Ok(Exported { frames: done, cancelled: true }),
    }
}

/// Plays the sounds of the animation from frame `marker` on, replacing
/// what is playing.
#[tauri::command]
pub fn audio_play(marker: f64, state: State<'_, AppState>) {
    let samples = {
        let guard = state.project();
        let Some(project) = guard.as_ref() else { return };
        let sounds = state.sounds();
        if !sounds.any(project) {
            return;
        }
        sounds.mix(project, marker.max(0.0), project.length() as f64, false)
    };
    state.player().play(samples);
}

#[tauri::command]
pub fn audio_stop(state: State<'_, AppState>) {
    state.player().stop();
}

/// The waveform of a sound resource: the loudest sample of each of
/// `buckets` equal stretches.
#[tauri::command]
pub fn sound_peaks(resource: String, buckets: usize, state: State<'_, AppState>) -> Option<Vec<f32>> {
    state.sounds().peaks(&mi_core::SaveId::new(&resource), buckets.clamp(1, 4096))
}

/// Stops the export that is running after the frame it is at.
#[tauri::command]
pub fn cancel_export(state: State<'_, AppState>) {
    state.cancel_export().store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Opens a project file and makes it the current project.
#[tauri::command]
pub fn open_project(path: String, app: AppHandle, state: State<'_, AppState>) -> Result<ProjectSummary, CommandError> {
    let (project, warnings) = Project::open(Path::new(&path), ProjectContext::default())?;

    remember(&app, &project);

    let summary = summarize(&project, state.language(), warnings);
    let work_camera = saved_work_camera(&project);
    let marker = summary.marker.min(summary.length as f64);
    state.set_project(Some(project));
    state.update_view(|view| {
        view.marker = marker;
        view.work_camera = work_camera;
    });
    Ok(summary)
}

/// State of one timeline at a frame.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineFrame {
    id: String,
    position: [f64; 3],
    rotation: [f64; 3],
    scale: [f64; 3],
    world_position: [f64; 3],
    /// Visible after taking the parents into account.
    visible: bool,
    /// Alpha after multiplying with the parents'.
    alpha: f64,
    /// The timeline's own alpha value.
    alpha_value: f64,
    transition: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameState {
    marker: f64,
    /// In the same order as [`ProjectSummary::timelines`].
    timelines: Vec<TimelineFrame>,
    /// Id of the camera the scene is seen through at this frame.
    active_camera: Option<String>,
}

pub(crate) fn frame_state(project: &Project, marker: f64, state: &AppState) -> FrameState {
    use mi_core::ValueId::*;
    let (scene, order) = state.evaluate(project, marker);
    let timelines = order
        .iter()
        .zip(&scene.nodes)
        .map(|(&index, node)| {
            let v = &node.values;
            TimelineFrame {
                id: project.timelines()[index].id.to_string(),
                position: [v.number(PosX), v.number(PosY), v.number(PosZ)],
                rotation: [v.number(RotX), v.number(RotY), v.number(RotZ)],
                scale: [v.number(ScaX), v.number(ScaY), v.number(ScaZ)],
                world_position: node.world_pos,
                visible: node.inherited.visible,
                alpha: node.inherited.alpha,
                alpha_value: v.number(Alpha),
                transition: v[Transition].as_str().unwrap_or("linear").to_owned(),
            }
        })
        .collect();
    let active_camera =
        project.active_camera(&scene, &order).map(|index| project.timelines()[index].id.to_string());
    FrameState { marker, timelines, active_camera }
}

/// Moves to a frame: evaluates the open project there and shows it in the
/// viewport.
#[tauri::command]
pub fn evaluate_frame(marker: f64, state: State<'_, AppState>) -> Result<FrameState, CommandError> {
    let frame = {
        let guard = state.project();
        let project = guard.as_ref().ok_or(CommandError::NoProject)?;
        frame_state(project, marker, &state)
    };
    state.update_view(|view| view.marker = marker);
    Ok(frame)
}

/// Tells the backend where on the window the viewport element is, in
/// physical pixels. A size of zero hides the scene.
#[tauri::command]
pub fn set_viewport_rect(x: f64, y: f64, width: f64, height: f64, state: State<'_, AppState>) {
    let pixels = |v: f64| v.round().clamp(0.0, 65_535.0) as u32;
    let rect = mi_render::Viewport { x: pixels(x), y: pixels(y), width: pixels(width), height: pixels(height) };
    state.update_view(|view| view.rect = Some(rect));
}

/// A mouse drag in the viewport, in pixels.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CameraDrag {
    Orbit,
    Pan,
}

/// Moves the work camera by a mouse drag.
#[tauri::command]
pub fn viewport_drag(kind: CameraDrag, dx: f64, dy: f64, state: State<'_, AppState>) {
    state.update_view(|view| match kind {
        CameraDrag::Orbit => view.work_camera.orbit(dx as f32, dy as f32),
        CameraDrag::Pan => view.work_camera.pan(dx as f32, dy as f32, 1.0),
    });
}

/// Zooms the work camera by mouse wheel steps; positive moves away.
#[tauri::command]
pub fn viewport_zoom(steps: f64, state: State<'_, AppState>) {
    let far = state.project().as_ref().map_or(30000.0, |p| p.file().render.distance as f32);
    state.update_view(|view| view.work_camera.zoom_by(steps as f32, far));
}

/// Puts the work camera back where the project was saved with it.
#[tauri::command]
pub fn viewport_reset_camera(state: State<'_, AppState>) {
    let camera = state.project().as_ref().map(saved_work_camera).unwrap_or_default();
    state.update_view(|view| view.work_camera = camera);
}

/// Sets how the viewport shows the scene and through which camera.
#[tauri::command]
pub fn set_view_options(mode: ViewMode, timeline_camera: bool, state: State<'_, AppState>) {
    state.update_view(|view| {
        view.mode = mode;
        view.use_timeline_camera = timeline_camera;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{IdGenerator, SaveId, Value, ValueId};
    use mi_format::project::{Keyframe, ProjectFile, Timeline};

    fn project() -> Project {
        let mut file = ProjectFile::new(0.0, 1.0);
        file.info.name = "Test".to_owned();
        file.info.author = "Someone".to_owned();
        file.info.tempo = 30.0;

        let mut folder = Timeline::new(SaveId::new("FOLDER0000000000"), TlType::Folder, &file.defaults);
        folder.name = "Folder".to_owned();
        folder.parent_tree_index = Some(1);
        folder.default_values[ValueId::Visible] = Value::Bool(false);

        let mut camera = Timeline::new(SaveId::new("CAMERA0000000000"), TlType::Camera, &file.defaults);
        camera.name = "Main camera".to_owned();
        camera.parent_tree_index = Some(0);
        for (position, x) in [(0, 0.0), (30, 60.0)] {
            let mut values = camera.default_values.clone();
            values[ValueId::PosX] = Value::Number(x);
            camera.keyframes.push(Keyframe { position, values });
        }

        let mut cube = Timeline::new(SaveId::new("CUBE000000000000"), TlType::Cube, &file.defaults);
        cube.name = "Cube".to_owned();
        cube.parent = SaveId::new("FOLDER0000000000");
        cube.parent_tree_index = Some(0);

        file.objects.timelines = vec![folder, camera, cube];
        Project::from_file(file, IdGenerator::new(7)).0
    }

    #[test]
    fn summary_lists_timelines_in_tree_order() {
        let summary = summarize(&project(), &Default::default(), vec!["note".to_owned()]);
        assert_eq!(summary.name, "Test");
        assert_eq!(summary.length, 30);
        assert_eq!(summary.cameras, 1);
        let names: Vec<&str> = summary.timelines.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Main camera", "Folder", "Cube"]);
        assert_eq!(summary.timelines[2].depth, 1);
        assert_eq!(summary.timelines[0].keyframes, [0, 30]);
        assert_eq!(summary.warnings, ["note"]);
    }

    #[test]
    fn frame_state_interpolates_and_inherits() {
        let project = project();
        let frame = frame_state(&project, 15.0, &AppState::default());
        assert_eq!(frame.timelines.len(), 3);
        assert_eq!(frame.timelines[0].world_position, [30.0, 0.0, 0.0]);
        assert_eq!(frame.active_camera.as_deref(), Some("CAMERA0000000000"));
        // The cube inherits the folder's invisibility.
        assert!(!frame.timelines[1].visible);
        assert!(!frame.timelines[2].visible);
        assert_eq!(frame.timelines[2].scale, [1.0, 1.0, 1.0]);
    }
}

/// What a click at (`x`, `y`) of the viewport (physical pixels from its top
/// left corner) selects (`view_click`): the clicked timeline, or with
/// nothing selected yet and without `exact` (Ctrl) its outermost unlocked
/// ancestor. Selected and locked timelines let clicks through to what is
/// behind them. `None` means the click hit nothing.
#[tauri::command]
pub fn viewport_pick(
    x: f64,
    y: f64,
    exact: bool,
    selected: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    // Clicks go through what is selected already, to what is behind it.
    let selection = state.selection();
    let exclude: Vec<usize> = {
        let guard = state.project();
        let project = guard.as_ref().ok_or(CommandError::NoProject)?;
        project
            .timelines()
            .iter()
            .enumerate()
            .filter(|(_, t)| t.lock || selected.as_deref() == Some(t.id.as_str()) || selection.contains(&t.id))
            .map(|(i, _)| i)
            .collect()
    };
    let Some(viewport) = state.viewport() else { return Ok(None) };
    // The project must not be locked while the render thread draws the pick.
    let Some(mut index) = viewport.pick(x.max(0.0) as u32, y.max(0.0) as u32, exclude) else { return Ok(None) };

    let guard = state.project();
    let project = guard.as_ref().ok_or(CommandError::NoProject)?;
    if index >= project.timelines().len() {
        return Ok(None);
    }
    if selected.is_none() && !exact {
        while let Some(parent) = project.tree().parent(index) {
            if project.timelines()[parent].lock {
                break;
            }
            index = parent;
        }
    }
    Ok(Some(project.timelines()[index].id.to_string()))
}

/// Tells the backend which timelines are selected, for the outline in the
/// viewport.
#[tauri::command]
pub fn set_selection(timelines: Vec<String>, state: State<'_, AppState>) {
    state.set_selection(timelines.iter().map(mi_core::SaveId::new).collect());
}

/// The move arrows and rotation rings of a timeline as the viewport shows
/// them now, for the frontend to draw and drag.
#[tauri::command]
pub fn viewport_gizmo(id: String, state: State<'_, AppState>) -> Result<crate::gizmo::Gizmo, CommandError> {
    use crate::scene_builder::{scene_camera, ViewCamera};
    let view = state.view();
    let Some(rect) = view.rect else { return Ok(Default::default()) };
    let guard = state.project();
    let project = guard.as_ref().ok_or(CommandError::NoProject)?;
    let Some(index) = project.timeline_index(&mi_core::SaveId::new(&id)) else { return Ok(Default::default()) };
    let (scene, order) = state.evaluate(project, view.marker);
    let Some(node) = order.iter().position(|&i| i == index) else { return Ok(Default::default()) };
    let view_camera =
        if view.use_timeline_camera { ViewCamera::Active(view.work_camera) } else { ViewCamera::Work(view.work_camera) };
    let camera = scene_camera(project, &scene, &order, view.marker, view_camera);
    let timeline = &project.timelines()[index];
    Ok(crate::gizmo::gizmo(timeline.kind, &scene.nodes[node], timeline.scale_resize, &camera, rect.width as f64, rect.height as f64))
}
