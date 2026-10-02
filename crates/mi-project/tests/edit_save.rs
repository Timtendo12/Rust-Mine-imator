//! Edits survive saving and opening the project again.

use mi_core::{IdGenerator, SaveId, TlType, Value, ValueId};
use mi_format::project::{ProjectFile, Timeline};
use mi_project::{KeyframeRef, Project, ProjectContext, ValueChange};

#[test]
fn edited_keyframes_are_saved() {
    let mut file = ProjectFile::new(0.0, 1.0);
    file.objects.timelines.push(Timeline::new(SaveId::new("CUBE00000000000A"), TlType::Cube, &file.defaults));
    let mut project = Project::from_file(file, IdGenerator::new(7)).0;
    let id = SaveId::new("CUBE00000000000A");

    project.set_values(std::slice::from_ref(&id), &[], 0, &[(ValueId::PosX, Value::Number(12.5))], ValueChange::Set, None);
    project.set_values(std::slice::from_ref(&id), &[], 30, &[(ValueId::RotZ, Value::Number(90.0))], ValueChange::Set, None);
    project.move_keyframes(&[KeyframeRef { timeline: id.clone(), position: 30 }], 10, None);
    project.rename_timeline(&id, "Spinning cube");

    let folder = std::env::temp_dir().join(format!("mi-edit-save-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("Edited.miproject");
    project.save_as(&path).unwrap();
    assert!(!project.is_changed());

    let (reopened, warnings) = Project::open(&path, ProjectContext::default()).unwrap();
    std::fs::remove_dir_all(&folder).ok();
    assert!(warnings.is_empty(), "{warnings:?}");
    let timeline = reopened.timeline(&id).unwrap();
    assert_eq!(timeline.name, "Spinning cube");
    let positions: Vec<i64> = timeline.keyframes.iter().map(|k| k.position).collect();
    assert_eq!(positions, [0, 40]);
    assert_eq!(timeline.keyframes[0].values.number(ValueId::PosX), 12.5);
    // The second keyframe was made from the values at frame 30, so it
    // keeps the position.
    assert_eq!(timeline.keyframes[1].values.number(ValueId::PosX), 12.5);
    assert_eq!(timeline.keyframes[1].values.number(ValueId::RotZ), 90.0);
}
