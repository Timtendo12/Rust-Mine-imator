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

#[test]
fn skins_are_set_on_the_model_of_a_timeline_or_its_parts() {
    use mi_core::{ObjRef, SaveId};
    use mi_format::project::{ProjectFile, Template, Timeline};

    let dir = temp_dir("skin");
    std::fs::write(dir.join("hero.png"), b"png").unwrap();
    let mut file = ProjectFile::new(0.0, 1.0);
    let mut template = Template::new(SaveId::new("STEVE_T"), TempType::Character);
    template.model_name = "human".into();
    file.objects.templates.push(template);
    file.objects.templates.push(Template::new(SaveId::new("CUBE_T"), TempType::Cube));
    let mut add = |id: &str, kind: TlType, temp: &str| {
        let mut timeline = Timeline::new(SaveId::new(id), kind, &file.defaults);
        timeline.temp = ObjRef::id(temp);
        file.objects.timelines.push(timeline);
    };
    add("STEVE", TlType::Character, "STEVE_T");
    add("ARM", TlType::Bodypart, "STEVE_T");
    add("CUBE", TlType::Cube, "CUBE_T");
    let mut project = Project::from_file(file, mi_core::IdGenerator::new(1)).0;

    // A cube shows no model.
    assert_eq!(project.model_skin(&SaveId::new("CUBE")), None);
    assert!(!project.set_model_skin(&[SaveId::new("CUBE")], Some(&dir.join("hero.png")), &|_| true));
    assert_eq!(project.model_skin(&SaveId::new("STEVE")), Some(None));

    // Through a body part the skin of the whole character changes.
    assert!(project.set_model_skin(&[SaveId::new("ARM")], Some(&dir.join("hero.png")), &|t| t.model_name == "human"));
    assert_eq!(project.model_skin(&SaveId::new("STEVE")), Some(Some("hero.png".into())));
    let skin = project.resources()[0].clone();
    assert_eq!((skin.kind, skin.player_skin), (ResType::Skin, true));
    assert_eq!(project.resource_path(&skin).unwrap(), dir.join("hero.png"));

    // Back to the texture of the assets, and undo.
    assert!(project.set_model_skin(&[SaveId::new("STEVE")], None, &|_| true));
    assert_eq!(project.model_skin(&SaveId::new("ARM")), Some(None));
    project.undo();
    assert_eq!(project.model_skin(&SaveId::new("ARM")), Some(Some("hero.png".into())));
    project.undo();
    assert_eq!(project.model_skin(&SaveId::new("ARM")), Some(None));
    assert!(project.resources().is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn particle_presets_become_spawner_timelines() {
    let presets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Particles");
    let mut project = Project::new(ProjectContext::default());
    let mut made = 0;
    for entry in std::fs::read_dir(&presets).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        let id = project.create_particles(&bytes, "Preset").unwrap();
        made += 1;
        let timeline = project.timeline(&id).unwrap();
        assert_eq!(timeline.kind, TlType::ParticleSpawner);
        let template = project.template(timeline.temp.as_id().unwrap()).unwrap();
        assert_eq!((template.kind, template.name.as_str()), (TempType::ParticleSpawner, "Preset"));
        let spawner = template.particles.as_ref().unwrap();
        // Types that are objects of the preset's own library are left out.
        assert!(spawner.types.iter().all(|t| !matches!(t.source, mi_format::project::ParticleSource::Object(_))), "{path:?}");
    }
    assert!(made >= 10);
    // Every particle type has an id of its own.
    let mut ids: Vec<_> =
        project.templates().iter().flat_map(|t| t.particles.iter()).flat_map(|p| p.types.iter().map(|k| k.id.clone())).collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total);
    assert!(project.create_particles(b"not a file", "x").is_err());
    assert!(project.undo());
    assert_eq!(project.timelines().len(), made - 1);
}
