//! Reads scenery files named in `MI_SCENERY_FIXTURES` (separated by `;`)
//! and meshes them. Without the variable the test does nothing; it is for
//! checking real files that cannot be part of the repository.

use mi_assets::{AssetPack, LegacyBlocks, Scenery};
use std::path::{Path, PathBuf};

#[test]
fn scenery_fixtures_load_and_mesh() {
    let Ok(list) = std::env::var("MI_SCENERY_FIXTURES") else { return };
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data");
    let pack = AssetPack::open(&data.join("Minecraft"), "1.20.2").unwrap();
    let legacy = LegacyBlocks::load(&std::fs::read(data.join("legacy.midata")).unwrap(), pack.blocks());

    for path in list.split(';').filter(|p| !p.is_empty()) {
        let path = Path::new(path);
        let bytes = std::fs::read(path).unwrap();
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let start = std::time::Instant::now();
        let scenery = Scenery::read(&bytes, extension, pack.blocks(), &legacy, Default::default()).unwrap();
        let read = start.elapsed();
        let meshes = scenery.meshes(&pack, true, true, true);
        let triangles: usize = meshes.iter().map(|(_, m)| m.triangle_count()).sum();
        eprintln!(
            "{}: size {:?}, {} palette entries, {} timeline blocks, {} textures, {triangles} triangles (read {read:?}, total {:?})",
            path.display(),
            scenery.size,
            scenery.palette.len(),
            scenery.timeline_count(pack.blocks()),
            meshes.len(),
            start.elapsed()
        );
        let mut names: Vec<&str> = scenery.palette.iter().map(|b| b.block.as_str()).collect();
        names.sort();
        names.dedup();
        eprintln!("  blocks: {}", names.join(", "));
        assert!(triangles > 0);
    }
}
