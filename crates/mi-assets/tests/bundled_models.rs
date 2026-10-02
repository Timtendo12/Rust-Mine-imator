//! Loads every model that ships with the program and builds its meshes.

use mi_assets::{shape_mesh, BendStyle, ModelFile, ModelPart, ShapeKind};
use mi_mesh::MeshData;
use std::io::Read;
use std::path::PathBuf;

fn models() -> Vec<(String, Vec<u8>)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Data/Minecraft/1.20.2.zip");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if entry.name().ends_with(".mimodel") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            out.push((entry.name().to_owned(), bytes));
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn check_mesh(mesh: &MeshData, name: &str) {
    assert_eq!(mesh.vertices.len() % 3, 0, "{name}");
    for v in &mesh.vertices {
        assert!(v.position.iter().chain(&v.normal).chain(&v.uv).all(|c| c.is_finite()), "{name}: not finite");
    }
}

#[test]
fn every_bundled_model_loads_and_meshes() {
    let models = models();
    assert!(models.len() > 120, "only {} models found", models.len());

    let (mut parts, mut shapes, mut bends) = (0, 0, 0);
    for (name, bytes) in &models {
        let model = ModelFile::load(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!model.name.is_empty(), "{name}");

        for part in model.all_parts() {
            parts += 1;
            for (index, shape) in part.shapes.iter().enumerate() {
                shapes += 1;
                let label = format!("{name} / {} / shape {index}", part.name);

                // Resting mesh.
                let rest = shape_mesh(shape, part.bend.as_ref(), [0.0; 3], BendStyle::Blocky);
                check_mesh(&rest, &label);
                if shape.kind == ShapeKind::Block {
                    // A box with no height is a single face, as in the
                    // original.
                    // (One bundled fish fin has its top below its bottom,
                    // which the original also draws as a single face.)
                    let flat = shape.to[2] - shape.from[2] < 0.0001;
                    assert_eq!(rest.triangle_count(), if flat { 2 } else { 12 }, "{label}");
                }

                // Bent meshes in both styles, when the part can bend.
                if let Some(bend) = &part.bend {
                    bends += 1;
                    for style in [BendStyle::Blocky, BendStyle::Realistic] {
                        for angle in [35.0, 90.0, -120.0, 180.0] {
                            let angles = [0, 1, 2].map(|i| if bend.axis[i] { angle } else { 0.0 });
                            let bent = shape_mesh(shape, Some(bend), angles, style);
                            check_mesh(&bent, &format!("{label} bent {angle} {style:?}"));
                            assert!(bent.triangle_count() >= rest.triangle_count().min(2), "{label}");
                        }
                    }
                }
            }
        }
    }
    assert!(parts > 800 && shapes > 1000 && bends > 100, "{parts} parts, {shapes} shapes, {bends} bendable");
}

/// The corners of an unrotated, uninverted box must face away from its
/// centre and wind accordingly.
#[test]
fn boxes_face_outwards() {
    let mut checked = 0;
    for (name, bytes) in models() {
        let model = ModelFile::load(&bytes).unwrap();
        for part in model.all_parts() {
            for shape in &part.shapes {
                let flat = (0..3).any(|i| (shape.to[i] - shape.from[i]).abs() < 1e-6);
                if shape.kind != ShapeKind::Block || shape.invert || shape.rotation != [0.0; 3] || flat {
                    continue;
                }
                // Negative scale mirrors the box and turns it inside out.
                if shape.scale.iter().any(|s| *s <= 0.0) || (0..3).any(|i| shape.to[i] < shape.from[i]) {
                    continue;
                }
                let mesh = shape_mesh(shape, None, [0.0; 3], BendStyle::Blocky);
                let centre = [0, 1, 2].map(|i| ((shape.from[i] + shape.to[i]) / 2.0) as f32);
                for tri in mesh.vertices.chunks(3) {
                    let face = cross(sub(tri[1].position, tri[0].position), sub(tri[2].position, tri[0].position));
                    let middle = [0, 1, 2].map(|i| (tri[0].position[i] + tri[1].position[i] + tri[2].position[i]) / 3.0);
                    assert!(dot(face, sub(middle, centre)) > 0.0, "{name} / {}: a face points inwards", part.name);
                    for v in tri {
                        assert!(dot(face, v.normal) > 0.0, "{name} / {}: normal against winding", part.name);
                    }
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 500, "{checked} boxes checked");
}

fn find<'a>(model: &'a ModelFile, name: &str) -> &'a ModelPart {
    model.find_part(name).unwrap_or_else(|| panic!("no part {name}"))
}

#[test]
fn steve_has_the_expected_parts() {
    let (_, bytes) = models().into_iter().find(|(name, _)| name.ends_with("character/steve.mimodel")).unwrap();
    let steve = ModelFile::load(&bytes).unwrap();
    assert!(steve.player_skin);
    assert_eq!(steve.texture_size, [64.0, 64.0]);

    let head = find(&steve, "head");
    assert!(head.bend.is_none());
    // The head is an 8×8×8 box sitting on the neck.
    let skull = &head.shapes[0];
    assert_eq!((skull.to[0] - skull.from[0], skull.to[1] - skull.from[1], skull.to[2] - skull.from[2]), (8.0, 8.0, 8.0));

    // Arms and legs bend at the middle and support inverse kinematics.
    for limb in ["left_arm", "right_arm", "left_leg", "right_leg"] {
        let part = find(&steve, limb);
        let bend = part.bend.unwrap_or_else(|| panic!("{limb} does not bend"));
        assert!(bend.info().supports_ik(), "{limb}");
        assert!(bend.offset < 0.0, "{limb}: the joint is below the shoulder or hip");
    }

    // The UVs of the head stay inside the texture.
    let mesh = shape_mesh(skull, None, [0.0; 3], BendStyle::Blocky);
    assert!(mesh.vertices.iter().all(|v| (0.0..=1.0).contains(&v.uv[0]) && (0.0..=1.0).contains(&v.uv[1])));
}

#[test]
fn bending_moves_the_lower_half_only() {
    let (_, bytes) = models().into_iter().find(|(name, _)| name.ends_with("character/steve.mimodel")).unwrap();
    let steve = ModelFile::load(&bytes).unwrap();
    let leg = find(&steve, "right_leg");
    let bend = leg.bend.unwrap();
    let shape = &leg.shapes[0];

    let rest = shape_mesh(shape, Some(&bend), [0.0; 3], BendStyle::Blocky);
    let bent = shape_mesh(shape, Some(&bend), [90.0, 0.0, 0.0], BendStyle::Blocky);
    assert!(bent.triangle_count() > rest.triangle_count(), "a bent limb is cut into segments");

    let extent = |mesh: &MeshData, axis: usize| {
        let values = mesh.vertices.iter().map(|v| v.position[axis]);
        (values.clone().fold(f32::MAX, f32::min), values.fold(f32::MIN, f32::max))
    };
    // The top of the leg stays where it was; the foot swings out along Y and
    // no longer reaches as far down.
    assert!((extent(&rest, 2).1 - extent(&bent, 2).1).abs() < 0.05);
    assert!(extent(&bent, 2).0 > extent(&rest, 2).0 + 2.0);
    let (rest_y, bent_y) = (extent(&rest, 1), extent(&bent, 1));
    assert!((bent_y.1 - bent_y.0) > (rest_y.1 - rest_y.0) + 2.0);

    // The realistic style spreads the fold over more segments.
    let realistic = shape_mesh(shape, Some(&bend), [90.0, 0.0, 0.0], BendStyle::Realistic);
    assert!(realistic.triangle_count() > bent.triangle_count());

    // Angles outside the part's range are limited: 400 degrees is the same
    // as the maximum.
    let max = bend.direction_max[0];
    let over = shape_mesh(shape, Some(&bend), [400.0, 0.0, 0.0], BendStyle::Blocky);
    let at_max = shape_mesh(shape, Some(&bend), [max, 0.0, 0.0], BendStyle::Blocky);
    assert_eq!(over, at_max);
}
