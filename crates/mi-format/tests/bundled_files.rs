//! Round-trips the files that ship with the program. They were written by
//! the original, so reproducing them byte for byte shows that both the reader
//! and the writer agree with it.

use mi_core::SaveId;
use mi_format::project::{Background, KeyframesFile, ParticlesFile, RenderFile, RenderSettings};
use mi_format::ValueSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn defaults() -> (ValueSet, Background) {
    let background = Background::default();
    (ValueSet::project_defaults(&background, 1.0, 1.0), background)
}

/// Compares a file written by the original with our output for the same
/// content. The bundled files predate a few settings, so lines for those
/// keys may be extra in our output; everything else must match exactly,
/// including order, indentation and line endings.
fn assert_same_document(original: &str, written: &str, added_later: &[&str], name: &str) {
    let mut expected = original.split("
").peekable();
    for line in written.split("
") {
        if expected.peek() == Some(&line) {
            expected.next();
            continue;
        }
        let key = line.trim().split('"').nth(1).unwrap_or("");
        assert!(
            added_later.contains(&key),
            "{name}: wrote {line:?} where the original has {:?}",
            expected.peek()
        );
    }
    assert_eq!(expected.next(), None, "{name}: output ends early");
}

#[test]
fn render_presets_round_trip_exactly() {
    let dir = assets().join("Data/Render");
    let mut count = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let original = fs::read_to_string(&path).unwrap();
        let file = RenderFile::load(original.as_bytes(), &RenderSettings::default()).unwrap();
        let written = file.save();
        assert_same_document(&original, &written, &["glint_speed", "glint_strength"], &path.display().to_string());
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn particle_presets_round_trip_exactly() {
    let (values, background) = defaults();
    let dir = assets().join("Particles");
    let mut count = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let original = fs::read_to_string(&path).unwrap();
        let mut new_id = || SaveId::new("GENERATED0000000");
        let loaded = ParticlesFile::load(original.as_bytes(), &values, &background, &mut new_id).unwrap();
        assert!(loaded.warnings.is_empty(), "{}: {:?}", path.display(), loaded.warnings);
        assert!(!loaded.file.particles.types.is_empty(), "{}", path.display());
        let written = loaded.file.save(&values);
        assert_same_document(&original, &written, &["randomize"], &path.display().to_string());
        count += 1;
    }
    assert_eq!(count, 14);
}

#[test]
fn bundled_animation_loops_load_and_survive_a_round_trip() {
    let (values, background) = defaults();
    let zip_path = assets().join("Data/Minecraft/1.20.2.zip");
    let mut archive = zip::ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
    let mut count = 0;
    let mut exact = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if !entry.name().ends_with(".miframes") {
            continue;
        }
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();

        let mut new_id = || SaveId::new("GENERATED0000000");
        let loaded = KeyframesFile::load(&bytes, 24.0, &values, &background, &mut new_id)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let file = loaded.file;
        assert!(!file.keyframes.is_empty(), "{name}");
        assert!(file.tempo > 0.0, "{name}");

        // Every keyframe must resolve against a timeline's defaults.
        for keyframe in &file.keyframes {
            let resolved = keyframe.resolve(&values, file.loaded_format);
            assert!(resolved.iter().all(|(id, v)| v.matches_kind(id.kind())), "{name}");
        }

        // Writing and reading again must give the same content.
        let written = file.save(&values);
        let mut new_id = || SaveId::new("GENERATED0000000");
        let again = KeyframesFile::load(written.as_bytes(), 24.0, &values, &background, &mut new_id).unwrap().file;
        assert_eq!(again.keyframes, file.keyframes, "{name}");
        assert_eq!((again.is_model, again.tempo, again.length), (file.is_model, file.tempo, file.length), "{name}");

        if written.as_bytes() == bytes.as_slice() {
            exact += 1;
        }
        count += 1;
    }
    assert!(count >= 90, "only {count} loops found");
    println!("{exact} of {count} animation loops round-trip byte for byte");
}
