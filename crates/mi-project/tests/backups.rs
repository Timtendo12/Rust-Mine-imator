//! Autosave backups next to the project (`project_backup`).

use mi_core::TlType;
use mi_project::{Project, ProjectContext};
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mi-backups-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn backups_rotate_and_only_follow_changes() {
    let dir = temp_dir("rotate");
    let folder = dir.join("My Movie");
    std::fs::create_dir_all(&folder).unwrap();
    let mut project = Project::new(ProjectContext::default());

    // Nothing to back up before the project has a file.
    project.create_timeline(TlType::Cube, None);
    assert_eq!(project.backup(3).unwrap(), None);
    assert_eq!(project.last_backup(), None);
    project.save_as(&folder.join("scene.miproject")).unwrap();
    // Or when nothing changed since it was saved.
    assert_eq!(project.backup(3).unwrap(), None);

    // Backups are named after the folder; the newest is number 1.
    let backup = |n: u32| folder.join(format!("My Movie.backup{n}"));
    let timelines = |n: u32| Project::open(&backup(n), ProjectContext::default()).unwrap().0.timelines().len();
    for expected in 2..=5 {
        project.create_timeline(TlType::Cube, None);
        assert_eq!(project.backup(3).unwrap(), Some(backup(1)));
        assert_eq!(timelines(1), expected);
        // A second backup without changes does not push the others out.
        assert_eq!(project.backup(3).unwrap(), None);
    }
    // Three are kept: 5, 4 and 3 cubes.
    assert_eq!((timelines(1), timelines(2), timelines(3)), (5, 4, 3));
    assert!(!backup(4).exists());
    assert_eq!(project.last_backup(), Some(backup(1)));
    // Backing up does not save the project.
    assert!(project.is_changed());
    assert_eq!(Project::open(&folder.join("scene.miproject"), ProjectContext::default()).unwrap().0.timelines().len(), 1);

    // With a single backup the file has no number.
    project.create_timeline(TlType::Cube, None);
    assert_eq!(project.backup(1).unwrap(), Some(folder.join("My Movie.backup")));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_opened_backup_finds_the_resources_but_is_not_saved_over() {
    let dir = temp_dir("open");
    let folder = dir.join("Movie");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(dir.join("house.schematic"), b"blocks").unwrap();
    let mut project = Project::new(ProjectContext::default());
    project.save_as(&folder.join("Movie.miproject")).unwrap();
    project.create_scenery(&dir.join("house.schematic"));
    project.save().unwrap();
    project.create_timeline(TlType::Cube, None);
    let backup = project.backup(5).unwrap().unwrap();

    let (mut opened, _) = Project::open(&backup, ProjectContext::default()).unwrap();
    assert_eq!(opened.timelines().len(), 2);
    assert_eq!(opened.folder(), Some(folder.as_path()));
    let resource = opened.resources()[0].clone();
    assert_eq!(opened.resource_path(&resource).unwrap(), folder.join("house.schematic"));
    // It has no file of its own: saving asks where to.
    assert_eq!(opened.path(), None);
    assert!(opened.save().is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}
