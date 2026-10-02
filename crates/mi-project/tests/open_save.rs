//! Opening, evaluating and saving a project through the project layer.

use mi_core::{IdGenerator, SaveId, TlType, Value, ValueId};
use mi_format::project::{Keyframe, ProjectFile, Timeline};
use mi_project::{Project, ProjectContext, ProjectError};
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mi-project-{}-{name}", std::process::id()))
}

fn sample() -> ProjectFile {
    let mut file = ProjectFile::new(0.0, 5.0);
    file.info.name = "Sample".into();
    file.info.tempo = 30.0;

    let mut folder = Timeline::new(SaveId::new("FOLDER"), TlType::Folder, &file.defaults);
    folder.name = "Folder".into();
    folder.parent_tree_index = Some(0);
    folder.default_values[ValueId::PosX] = Value::Number(100.0);

    let mut cube = Timeline::new(SaveId::new("CUBE"), TlType::Cube, &file.defaults);
    cube.name = "Cube".into();
    cube.parent = SaveId::new("FOLDER");
    cube.parent_tree_index = Some(0);
    for (position, z) in [(0, 0.0), (20, 40.0)] {
        let mut values = cube.default_values.clone();
        values[ValueId::PosZ] = Value::Number(z);
        cube.keyframes.push(Keyframe { position, values });
    }

    let mut hidden_camera = Timeline::new(SaveId::new("CAM1"), TlType::Camera, &file.defaults);
    hidden_camera.name = "Hidden".into();
    hidden_camera.hide = true;
    hidden_camera.parent_tree_index = Some(1);
    let mut camera = Timeline::new(SaveId::new("CAM2"), TlType::Camera, &file.defaults);
    camera.name = "Main".into();
    camera.parent_tree_index = Some(2);

    // Deliberately not in tree order.
    file.objects.timelines = vec![cube, camera, folder, hidden_camera];
    file
}

#[test]
fn evaluates_the_scene_in_tree_order() {
    let (project, warnings) = Project::from_file(sample(), IdGenerator::new(1));
    assert!(warnings.is_empty());
    assert_eq!(project.length(), 20);

    let names: Vec<&str> = project.tree().order().iter().map(|&i| project.timelines()[i].name.as_str()).collect();
    assert_eq!(names, ["Folder", "Cube", "Hidden", "Main"]);

    let (state, order) = project.evaluate(10.0);
    let cube_node = order.iter().position(|&i| project.timelines()[i].name == "Cube").unwrap();
    assert_eq!(state.nodes[cube_node].world_pos, [100.0, 0.0, 20.0]);

    let camera = project.active_camera(&state, &order).unwrap();
    assert_eq!(project.timelines()[camera].name, "Main");

    assert!(project.timeline(&SaveId::new("CUBE")).is_some());
    assert!(project.timeline(&SaveId::new("NOPE")).is_none());
}

#[test]
fn save_and_reopen() {
    let (mut project, _) = Project::from_file(sample(), IdGenerator::new(1));
    assert!(matches!(project.save(), Err(ProjectError::NoPath)));

    let path = temp_file("save.miproject");
    project.save_as(&path).unwrap();
    assert_eq!(project.path(), Some(path.as_path()));
    assert!(!project.is_changed());

    let (reopened, warnings) = Project::open(&path, ProjectContext::default()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(reopened.file().info, project.file().info);
    assert_eq!(reopened.timelines().len(), 4);
    let names = |p: &Project| -> Vec<String> {
        p.tree().order().iter().map(|&i| p.timelines()[i].name.clone()).collect()
    };
    assert_eq!(names(&reopened), names(&project));
    assert_eq!(reopened.evaluate(10.0).0.nodes.len(), 4);
}

#[test]
fn open_reports_problems() {
    let missing = Project::open(&temp_file("missing.miproject"), ProjectContext::default());
    assert!(matches!(missing, Err(ProjectError::Read { .. })));

    let legacy = Project::open(&PathBuf::from("old.mproj"), ProjectContext::default());
    assert!(matches!(legacy, Err(ProjectError::LegacyProject(_))));

    let path = temp_file("broken.miproject");
    std::fs::write(&path, "{ not json").unwrap();
    let broken = Project::open(&path, ProjectContext::default());
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(broken, Err(ProjectError::Format { .. })));
}

#[test]
fn file_name_is_used_when_the_project_has_no_name() {
    let mut file = sample();
    file.info.name.clear();
    let path = temp_file("Unnamed project.miproject");
    std::fs::write(&path, file.save()).unwrap();
    let (project, _) = Project::open(&path, ProjectContext::default()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(project.file().info.name.ends_with("Unnamed project"));
}

#[test]
fn new_ids_do_not_collide() {
    let mut project = Project::new(ProjectContext::default());
    let a = project.new_id();
    let b = project.new_id();
    assert_ne!(a, b);
    assert_eq!(project.length(), 0);
    assert!(project.evaluate(0.0).0.nodes.is_empty());
}
