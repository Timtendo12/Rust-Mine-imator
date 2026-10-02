//! Loads project files, reports what is in them and checks how closely
//! writing them back reproduces the original bytes.
//!
//! ```sh
//! cargo run -p mi-project --example check_project -- path/to/a.miproject [more...]
//! ```

use mi_format::project::{LoadOptions, ProjectFile};
use mi_project::{Project, ProjectContext};
use std::path::Path;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: check_project <file.miproject>...");
        std::process::exit(2);
    }

    let mut failed = false;
    for path in &paths {
        println!("== {path}");
        // Ground slot 1 is grass_block_top in the bundled 1.20.2 assets.
        let (project, warnings) = match Project::open(Path::new(path), ProjectContext { ground_slot: 1.0 }) {
            Ok(result) => result,
            Err(error) => {
                println!("   FAILED: {error}");
                failed = true;
                continue;
            }
        };
        let file = project.file();
        println!(
            "   format {} ({}), {} templates, {} timelines, {} resources, {} markers, length {} frames",
            file.loaded_format,
            file.created_in,
            project.templates().len(),
            project.timelines().len(),
            project.resources().len(),
            file.markers.len(),
            project.length()
        );
        for warning in &warnings {
            println!("   warning: {warning}");
        }

        let (scene, _) = project.evaluate(0.0);
        println!("   evaluated {} timelines at frame 0", scene.nodes.len());
        let pack_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft");
        if let Ok(pack) = mi_assets::AssetPack::open(&pack_dir, "1.20.2") {
            let bindings = mi_project::ModelBindings::bind(&project, &pack);
            let body_parts = project.timelines().iter().filter(|t| t.kind == mi_core::TlType::Bodypart).count();
            println!("   {} of {} body parts bound to a model", bindings.len(), body_parts);
        }

        // Compare what we would write with what is on disk, line by line.
        let original = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                println!("   could not re-read: {error}");
                continue;
            }
        };
        let written = file.save();

        // Whatever the bytes look like, loading what we wrote must give the
        // same project. The first save may normalise things (an older format
        // is upgraded, values equal to their default are dropped), so the
        // comparison is between two generations of our own output.
        let reload = |text: &str| {
            let mut counter = 0u32;
            let mut new_id = || {
                counter += 1;
                mi_core::SaveId::new(format!("CHECK{counter:011}"))
            };
            let options = LoadOptions { ground_slot: 1.0, seed: file.defaults.number(mi_core::ValueId::Seed), new_id: &mut new_id };
            ProjectFile::load(text.as_bytes(), options).map(|loaded| loaded.file)
        };
        match reload(&written) {
            Ok(second) => {
                let stable = second.save() == written;
                let same = second.objects == file.objects && second.background == file.background && second.render == file.render;
                println!("   reload of our output: {}", if stable && same { "same data, stable bytes" } else { "DIFFERS" });
                if !(stable && same) {
                    println!("     bytes stable: {stable}, background same: {}, render same: {}", second.background == file.background, second.render == file.render);
                    for (a, b) in second.objects.templates.iter().zip(&file.objects.templates) {
                        if a != b {
                            println!("     template differs: {} ({})", b.name, b.kind.name());
                        }
                    }
                    for (a, b) in second.objects.timelines.iter().zip(&file.objects.timelines).filter(|(a, b)| a != b).take(5) {
                        println!("     timeline differs: {} ({})", b.name, b.kind.name());
                        for (id, value) in a.default_values.iter() {
                            if *value != b.default_values[id] {
                                println!("       default {}: {:?} -> {:?}", id.name(), b.default_values[id], value);
                            }
                        }
                        for (ka, kb) in a.keyframes.iter().zip(&b.keyframes) {
                            for (id, value) in ka.values.iter() {
                                if *value != kb.values[id] {
                                    println!("       keyframe {} {}: {:?} -> {:?}", kb.position, id.name(), kb.values[id], value);
                                }
                            }
                        }
                    }
                    for (a, b) in second.objects.resources.iter().zip(&file.objects.resources) {
                        if a != b {
                            println!("     resource differs: {}", b.filename);
                        }
                    }
                }
                failed |= !(stable && same);
            }
            Err(error) => {
                println!("   reload of our output FAILED: {error}");
                failed = true;
            }
        }

        if written == original {
            println!("   round trip: identical");
            continue;
        }
        let (a, b): (Vec<&str>, Vec<&str>) = (original.lines().collect(), written.lines().collect());
        let different = a.iter().zip(&b).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len());
        println!("   round trip: {} of {} lines differ ({} lines written)", different, a.len(), b.len());
        for (index, (x, y)) in a.iter().zip(&b).enumerate().filter(|(_, (x, y))| x != y).take(8) {
            println!("     line {}: {:?}", index + 1, x.trim());
            println!("       wrote: {:?}", y.trim());
        }
    }
    if failed {
        std::process::exit(1);
    }
}
