//! Characters, special blocks and blocks created from the asset pack.

use mi_assets::AssetPack;
use mi_core::{IdGenerator, TlType};
use mi_format::project::ProjectFile;
use mi_format::StateValue;
use mi_project::{ModelBindings, Project};
use std::path::PathBuf;

fn pack() -> AssetPack {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft");
    AssetPack::open(&folder, "1.20.2").unwrap()
}

fn create(project: &mut Project, pack: &AssetPack, kind: TlType, name: &str) -> mi_core::SaveId {
    let def = pack.model(name).unwrap();
    let state: Vec<(String, StateValue)> =
        def.default_state.iter().map(|(k, v)| (k.clone(), StateValue::Str(v.clone()))).collect();
    let resolved = pack.resolve(name, &state).unwrap();
    project.create_model(kind, name, state, &resolved.file, &resolved.hide).unwrap()
}

#[test]
fn a_character_gets_a_timeline_per_part_in_the_model_hierarchy() {
    let pack = pack();
    let mut project = Project::from_file(ProjectFile::new(0.0, 1.0), IdGenerator::new(3)).0;
    let steve = create(&mut project, &pack, TlType::Character, "human");

    let owner = project.timeline(&steve).unwrap();
    let parts = owner.parts.as_ref().unwrap();
    assert!(parts.len() >= 6, "{}", parts.len());
    // Every part is bound to its model part, so it can be drawn and bent.
    let bindings = ModelBindings::bind(&project, &pack);
    assert_eq!(bindings.len(), parts.len());

    // The head hangs below the body, which hangs below the character.
    let by_name = |name: &str| project.timelines().iter().find(|t| t.model_part_name == name).unwrap();
    let body = by_name("body");
    let head = by_name("head");
    assert_eq!(body.parent, steve);
    assert_eq!(head.parent, body.id);
    assert!(head.inherit.alpha && head.inherit.rot_point && !head.scale_resize);
    let order: Vec<usize> = project.tree().order().to_vec();
    let depth_of = |id: &mi_core::SaveId| project.tree().depth(project.timeline_index(id).unwrap());
    assert_eq!((depth_of(&steve), depth_of(&body.id), depth_of(&head.id)), (0, 1, 2));
    assert_eq!(order.len(), 1 + parts.len());

    // One undo removes it all again.
    project.undo();
    assert!(project.timelines().is_empty() && project.templates().is_empty());
}

#[test]
fn special_blocks_and_blocks() {
    let pack = pack();
    let mut project = Project::from_file(ProjectFile::new(0.0, 1.0), IdGenerator::new(3)).0;
    let chest = create(&mut project, &pack, TlType::SpecialBlock, "chest");
    assert!(!project.timeline(&chest).unwrap().parts.as_ref().unwrap().is_empty());
    let block = project.create_block("stone", Vec::new());
    let timeline = project.timeline(&block).unwrap();
    let template = project.template(timeline.temp.as_id().unwrap()).unwrap();
    assert_eq!((timeline.kind, template.block_name.as_str()), (TlType::Block, "stone"));
}
