//! Resolves every block of the bundled asset pack in every value of its
//! states and builds the meshes.

use mi_assets::{block_mesh, AssetPack, Blocks};
use mi_mesh::MeshData;
use std::collections::HashMap;
use std::path::PathBuf;

fn pack() -> AssetPack {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft");
    AssetPack::open(&folder, "1.20.2").unwrap()
}

#[test]
fn every_block_value_has_a_model_with_existing_textures() {
    let pack = pack();
    let blocks = Blocks::load(&pack);
    assert!(blocks.len() > 250, "{}", blocks.len());

    let mut checked = 0;
    let mut missing_models = Vec::new();
    let mut missing_textures = Vec::new();
    let mut names: Vec<&str> = blocks.names().collect();
    names.sort();
    for name in names {
        let block = blocks.def(name).unwrap();
        let mut states = vec![block.default_full_state()];
        // Try each value of each state on top of the default.
        for (state, values) in &block.states {
            for value in values.iter().take(24) {
                let mut s = block.default_full_state();
                if let Some(slot) = s.iter_mut().find(|(n, _)| n == state) {
                    slot.1 = value.value.clone();
                }
                states.push(s);
            }
        }
        for state in states {
            let models = blocks.models(&pack, block, &state);
            if models.is_empty() {
                // Blocks drawn by special code (liquids, signs, ...) have
                // no blockstate file.
                if block.file.is_some() || block.states.iter().any(|(_, v)| v.iter().any(|v| v.file.is_some())) {
                    missing_models.push(format!("{name} {state:?}"));
                }
                continue;
            }
            let chosen: Vec<_> = models.iter().map(|c| &c[0]).collect();
            let mut meshes: HashMap<String, MeshData> = HashMap::new();
            block_mesh(&chosen, [0.0; 3], 0.0, &|_, _, _| false, &mut meshes);
            for (texture, mesh) in &meshes {
                assert_eq!(mesh.vertices.len() % 3, 0);
                assert!(mesh.vertices.iter().all(|v| v.position.iter().all(|c| c.is_finite())));
                if pack.read(&format!("textures/{texture}.png")).is_none() {
                    missing_textures.push(format!("{name}: {texture}"));
                }
            }
            checked += 1;
        }
    }
    missing_models.dedup();
    missing_textures.sort();
    missing_textures.dedup();
    assert!(checked > 1500, "{checked}");
    assert!(missing_models.len() < 20, "{} without a model, e.g. {:?}", missing_models.len(), &missing_models[..missing_models.len().min(10)]);
    assert!(missing_textures.is_empty(), "{missing_textures:?}");
}

#[test]
fn a_full_block_covers_its_faces() {
    let pack = pack();
    let blocks = Blocks::load(&pack);
    let stone = blocks.def("stone").unwrap();
    let models = blocks.models(&pack, stone, &stone.full_state(&[]));
    assert_eq!(models.len(), 1);
    assert_eq!(models[0][0].face_full, [true; 6]);

    let mut meshes = HashMap::new();
    block_mesh(&[&models[0][0]], [16.0, 0.0, 0.0], 0.0, &|_, _, _| false, &mut meshes);
    let mesh = &meshes["block/stone"];
    assert_eq!(mesh.triangle_count(), 12);
    let xs: Vec<f32> = mesh.vertices.iter().map(|v| v.position[0]).collect();
    assert!(xs.iter().all(|&x| x == 16.0 || x == 32.0));

    // Hidden sides are left out.
    let mut meshes = HashMap::new();
    block_mesh(&[&models[0][0]], [0.0; 3], 0.0, &|_, _, d| d != mi_assets::Dir::Up, &mut meshes);
    assert_eq!(meshes["block/stone"].triangle_count(), 2);

    let (block, state) = blocks.by_id("minecraft:granite").unwrap();
    assert_eq!(block.name, "stone");
    assert_eq!(state, &[("variant".to_owned(), "granite".to_owned())]);
}

#[test]
fn stairs_follow_their_facing() {
    let pack = pack();
    let blocks = Blocks::load(&pack);
    let stairs = blocks.def("stairs").unwrap();
    let bounds = |facing: &str| {
        let state = stairs.full_state(&[
            ("facing".into(), mi_format::StateValue::Str(facing.into())),
            ("half".into(), mi_format::StateValue::Str("bottom".into())),
        ]);
        let models = blocks.models(&pack, stairs, &state);
        let mut meshes = HashMap::new();
        block_mesh(&[&models[0][0]], [0.0; 3], 0.0, &|_, _, _| false, &mut meshes);
        // Centre of the upper step.
        let upper: Vec<[f32; 3]> = meshes.values().flat_map(|m| m.vertices.iter().map(|v| v.position)).filter(|p| p[2] > 8.0).collect();
        let n = upper.len() as f32;
        [upper.iter().map(|p| p[0]).sum::<f32>() / n, upper.iter().map(|p| p[1]).sum::<f32>() / n]
    };
    // The upper step sits towards the facing direction: east is +X, south +Y.
    let east = bounds("east");
    let west = bounds("west");
    let south = bounds("south");
    let north = bounds("north");
    assert!(east[0] > west[0], "{east:?} {west:?}");
    assert!(south[1] > north[1], "{south:?} {north:?}");
}
