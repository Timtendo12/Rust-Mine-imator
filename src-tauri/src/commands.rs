//! Commands callable from the frontend.

use mi_core::{version, IdGenerator, TlType};
use mi_format::project::{LoadOptions, ProjectFile};
use serde::Serialize;
use std::path::Path;

/// Error returned to the frontend; shown to the user as is.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Could not read \"{path}\": {source}")]
    Read { path: String, source: std::io::Error },
    #[error("Could not open \"{path}\": {source}")]
    Format { path: String, source: mi_format::FormatError },
    #[error("\"{0}\" is a project from before version 1.1.0. Those cannot be opened yet.")]
    LegacyProject(String),
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
    parent: String,
    tree_index: Option<i64>,
    keyframes: usize,
    hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    path: String,
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
    templates: usize,
    resources: usize,
    markers: usize,
    cameras: usize,
    timelines: Vec<TimelineSummary>,
    warnings: Vec<String>,
}

/// Reads a project file and reports what is in it.
#[tauri::command]
pub fn inspect_project(path: String) -> Result<ProjectSummary, CommandError> {
    let extension = Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if extension == "mproj" || extension == "mani" {
        return Err(CommandError::LegacyProject(path));
    }

    let bytes = std::fs::read(&path).map_err(|source| CommandError::Read { path: path.clone(), source })?;
    let mut ids = IdGenerator::from_time();
    let options = LoadOptions { ground_slot: 0.0, seed: 1.0, new_id: &mut || ids.next_id() };
    let loaded =
        ProjectFile::load(&bytes, options).map_err(|source| CommandError::Format { path: path.clone(), source })?;
    let project = loaded.file;

    let name = if project.info.name.is_empty() {
        Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_owned()
    } else {
        project.info.name.clone()
    };
    let timelines = &project.objects.timelines;

    Ok(ProjectSummary {
        name,
        author: project.info.author.clone(),
        description: project.info.description.clone(),
        created_in: project.created_in.clone(),
        format: project.loaded_format,
        tempo: project.info.tempo,
        video_width: project.info.video_width,
        video_height: project.info.video_height,
        length: timelines.iter().filter_map(|tl| tl.keyframes.last()).map(|k| k.position).max().unwrap_or(0),
        templates: project.objects.templates.len(),
        resources: project.objects.resources.len(),
        markers: project.markers.len(),
        cameras: timelines.iter().filter(|tl| tl.kind == TlType::Camera).count(),
        timelines: timelines
            .iter()
            .map(|tl| TimelineSummary {
                id: tl.id.to_string(),
                name: tl.name.clone(),
                kind: tl.kind.name(),
                parent: tl.parent.to_string(),
                tree_index: tl.parent_tree_index,
                keyframes: tl.keyframes.len(),
                hidden: tl.hide,
            })
            .collect(),
        warnings: loaded.warnings,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::SaveId;
    use mi_format::project::Timeline;

    #[test]
    fn inspects_a_saved_project() {
        let mut project = ProjectFile::new(0.0, 1.0);
        project.info.author = "Someone".to_owned();
        project.info.tempo = 30.0;
        let mut camera = Timeline::new(SaveId::new("CAMERA0000000000"), TlType::Camera, &project.defaults);
        camera.name = "Main camera".to_owned();
        camera.parent_tree_index = Some(0);
        project.objects.timelines.push(camera);

        let path = std::env::temp_dir().join(format!("mi-inspect-{}.miproject", std::process::id()));
        std::fs::write(&path, project.save()).unwrap();
        let summary = inspect_project(path.to_string_lossy().into_owned());
        std::fs::remove_file(&path).unwrap();

        let summary = summary.unwrap();
        assert!(summary.name.starts_with("mi-inspect-"), "falls back to the file name: {}", summary.name);
        assert_eq!(summary.author, "Someone");
        assert_eq!(summary.tempo, 30.0);
        assert_eq!(summary.cameras, 1);
        assert_eq!(summary.timelines[0].kind, "camera");
        assert_eq!(summary.timelines[0].parent, "root");
    }

    #[test]
    fn reports_unreadable_and_legacy_files() {
        let missing = inspect_project("does/not/exist.miproject".to_owned()).unwrap_err();
        assert!(matches!(missing, CommandError::Read { .. }));
        let legacy = inspect_project("old.mproj".to_owned()).unwrap_err();
        assert!(matches!(legacy, CommandError::LegacyProject(_)));
    }
}
