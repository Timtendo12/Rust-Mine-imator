//! Resources added in the editor: read from where they are, copied next to
//! the project when it is saved.

use mi_core::{ResType, TempType, TlType};
use mi_project::{Project, ProjectContext};
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mi-resources-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn scenery_is_added_once_per_file_and_copied_on_saving() {
    let dir = temp_dir("scenery");
    let (downloads, other) = (dir.join("downloads"), dir.join("other"));
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(downloads.join("house.schematic"), b"first").unwrap();
    std::fs::write(other.join("house.schematic"), b"second").unwrap();

    let mut project = Project::new(ProjectContext::default());
    let first = project.create_scenery(&downloads.join("house.schematic"));
    let again = project.create_scenery(&downloads.join("house.schematic"));
    let different = project.create_scenery(&other.join("house.schematic"));

    // Two timelines share the first file's resource; the other file of the
    // same name gets a numbered name.
    let names: Vec<&str> = project.resources().iter().map(|r| r.filename.as_str()).collect();
    assert_eq!(names, ["house.schematic", "house (2).schematic"]);
    assert!(project.resources().iter().all(|r| r.kind == ResType::Scenery && r.scenery_tl_add == Some(false)));
    assert_eq!(project.templates().len(), 3);
    assert!(project.templates().iter().all(|t| t.kind == TempType::Scenery));
    for id in [&first, &again, &different] {
        assert_eq!(project.timeline(id).unwrap().kind, TlType::Scenery);
    }
    let template_of = |id| project.template(project.timeline(id).unwrap().temp.as_id().unwrap()).unwrap();
    assert_eq!(template_of(&first).scenery, template_of(&again).scenery);
    assert_ne!(template_of(&first).scenery, template_of(&different).scenery);

    // Until the project is saved the files are read from where they are.
    let resources = project.resources().to_vec();
    assert_eq!(project.resource_path(&resources[0]).unwrap(), downloads.join("house.schematic"));
    assert_eq!(project.resource_path(&resources[1]).unwrap(), other.join("house.schematic"));

    // Saving copies them next to the project under their names in it.
    let saved = dir.join("project");
    std::fs::create_dir_all(&saved).unwrap();
    project.save_as(&saved.join("scene.miproject")).unwrap();
    assert_eq!(std::fs::read(saved.join("house.schematic")).unwrap(), b"first");
    assert_eq!(std::fs::read(saved.join("house (2).schematic")).unwrap(), b"second");
    assert_eq!(project.resource_path(&resources[1]).unwrap(), saved.join("house (2).schematic"));

    // Saving somewhere else takes the resources along; saving in place
    // does not touch them.
    let moved = dir.join("moved");
    std::fs::create_dir_all(&moved).unwrap();
    project.save_as(&moved.join("scene.miproject")).unwrap();
    assert_eq!(std::fs::read(moved.join("house (2).schematic")).unwrap(), b"second");
    project.save().unwrap();

    // The saved project opens with the same resources.
    let (reopened, _) = Project::open(&moved.join("scene.miproject"), ProjectContext::default()).unwrap();
    assert_eq!(reopened.resources().len(), 2);
    assert_eq!(reopened.timelines().len(), 3);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn creating_scenery_and_text_is_one_undo_step_each() {
    let dir = temp_dir("undo");
    std::fs::write(dir.join("a.nbt"), b"x").unwrap();
    let mut project = Project::new(ProjectContext::default());
    project.create_scenery(&dir.join("a.nbt"));
    let text = project.create_text("Hello");
    assert_eq!(project.timeline(&text).unwrap().text, "Hello");
    assert_eq!(project.timeline(&text).unwrap().kind, TlType::Text);
    assert_eq!((project.resources().len(), project.templates().len(), project.timelines().len()), (1, 2, 2));

    assert!(project.undo());
    assert_eq!((project.resources().len(), project.templates().len(), project.timelines().len()), (1, 1, 1));
    assert!(project.undo());
    assert_eq!((project.resources().len(), project.templates().len(), project.timelines().len()), (0, 0, 0));
    // Redo brings the resource back with its file.
    assert!(project.redo());
    let resource = project.resources()[0].clone();
    assert_eq!(project.resource_path(&resource).unwrap(), dir.join("a.nbt"));
    std::fs::remove_dir_all(&dir).unwrap();
}
