//! Builds a project that uses every kind of object, writes it and reads it
//! back, and checks that upgrades of older formats are applied.

use mi_core::types::ValueType;
use mi_core::version::project as fmt;
use mi_core::{Color, ObjRef, ResType, SaveId, TempType, TlType, Value, ValueId};
use mi_format::project::{
    Keyframe, LoadOptions, Loaded, Marker, ParticleSource, ParticleType, Pattern, ProjectFile, Resource, Template,
    Timeline, ViewCamera,
};
use mi_format::{FormatError, StateValue};

fn id(n: usize) -> SaveId {
    SaveId::new(format!("ID{n:014}"))
}

fn load(text: &str) -> Loaded<ProjectFile> {
    let mut counter = 1000;
    let mut new_id = || {
        counter += 1;
        id(counter)
    };
    ProjectFile::load(text.as_bytes(), LoadOptions { ground_slot: 1.0, seed: 77.0, new_id: &mut new_id }).unwrap()
}

fn text_state(name: &str, value: &str) -> (String, StateValue) {
    (name.to_owned(), StateValue::Str(value.to_owned()))
}

fn sample_project() -> ProjectFile {
    let mut p = ProjectFile::new(1.0, 77.0);
    p.info.name = "Round trip".into();
    p.info.author = "Tester \"T\"".into();
    p.info.description = "Line one\nLine two".into();
    p.info.tempo = 30.0;
    p.info.render_settings = String::new();
    p.info.view_second_camera = ViewCamera::Timeline(id(20));
    p.info.timeline.region_start = Some(5.0);
    p.info.timeline.region_end = Some(60.0);
    p.render.samples = 48.0;
    p.render.shadows_transparent = true;
    p.render.ssao_color = Color::rgb(10, 20, 30);
    p.background.sky_time = 30.0;
    p.background.image = ObjRef::Id(id(40));
    p.background.biome = "desert".into();
    p.background.fog_color = Color::rgb(1, 2, 3);

    // One template of every kind.
    for (n, &kind) in TempType::ALL.iter().enumerate() {
        let mut t = Template::new(id(n), kind);
        t.name = format!("Template {}", kind.name());
        match kind {
            TempType::Character => {
                t.model_name = "armor".into();
                t.model_state = vec![text_state("variant", "steve")];
                t.model_tex = ObjRef::default_resource();
                t.model_version = 3.0;
                t.armor[2].dye = Color::rgb(9, 8, 7);
                t.armor[2].trim_pattern = "coast".into();
            }
            TempType::SpecialBlock => {
                t.model_name = "banner".into();
                t.pattern = Some(Pattern {
                    base_color: "red".into(),
                    patterns: vec!["stripe_top".into(), "cross".into()],
                    colors: vec!["white".into(), "black".into()],
                });
            }
            TempType::Bodypart => t.model_part_name = "head".into(),
            TempType::Item => {
                t.item_tex = ObjRef::default_resource();
                t.item_name = Some("item/diamond_sword".into());
                t.item.spin = true;
            }
            TempType::Block => {
                t.block_name = "oak_stairs".into();
                t.block_state = vec![text_state("facing", "north"), text_state("half", "top")];
                t.block_repeat_enable = true;
                t.block_repeat = [2.0, 3.0, 4.0];
            }
            TempType::Scenery => {
                t.scenery = ObjRef::Id(id(41));
                t.block_tex = ObjRef::default_resource();
                t.block_tex_material = ObjRef::default_resource();
                t.block_tex_normal = ObjRef::default_resource();
            }
            TempType::ParticleSpawner => {
                let spawner = t.particles.as_mut().unwrap();
                spawner.settings.spawn_amount = 42.0;
                spawner.settings.spawn_region_path = ObjRef::Id(id(27));
                let mut sheet = ParticleType::new(id(60));
                sheet.name = "sparks".into();
                sheet.source = ParticleSource::Sheet;
                sheet.settings.spd_israndom = [true, false, true];
                sheet.settings.spd_random_max = [1.0, 2.0, 3.0];
                let mut object = ParticleType::new(id(61));
                object.source = ParticleSource::Object(ObjRef::Id(id(8)));
                object.settings.angle_speed_israndom = false;
                spawner.types = vec![sheet, object];
            }
            TempType::Text => {
                t.text.font = ObjRef::Id(id(42));
                t.text.is_3d = true;
            }
            TempType::Model => t.model = ObjRef::Id(id(43)),
            TempType::Cube => {
                t.shape.tex = ObjRef::Id(id(40));
                t.shape.tex_mapped = true;
                t.shape.tex_hmirror = true;
            }
            _ => t.shape.detail = 16.0,
        }
        p.objects.templates.push(t);
    }

    // One timeline of every kind that can exist in a project.
    let kinds = TlType::ALL.iter().copied().filter(|k| !matches!(k, TlType::Shape | TlType::LightSource));
    for (n, kind) in kinds.enumerate() {
        let mut tl = Timeline::new(id(20 + n), kind, &p.defaults);
        tl.name = format!("Timeline {}", kind.name());
        if let Some(temp_kind) = kind.temp_type() {
            tl.temp = ObjRef::Id(id(temp_kind.index()));
        }
        tl.parent_tree_index = Some(n as i64);
        tl.depth = n as f64 - 3.0;
        tl.color_tag = (n % 3 == 0).then_some(2.0);
        tl.rot_point = [1.0, 2.0, 3.0];
        tl.inherit.alpha = true;
        tl.appearance.glow = true;
        tl.blend_mode = "add".into();
        tl.path.detail = 9.0;
        tl.wind = true;

        tl.default_values[ValueId::PosZ] = Value::Number(16.0);
        let mut first = tl.default_values.clone();
        first[ValueId::RotZ] = Value::Number(90.0);
        first[ValueId::Transition] = Value::Str("easeinoutquad".into());
        let mut second = tl.default_values.clone();
        second[ValueId::Alpha] = Value::Number(0.25);
        second[ValueId::TextureObj] = Value::Ref(ObjRef::None);
        second[ValueId::RgbMul] = Value::Color(Color::rgb(200, 100, 50));
        tl.keyframes = vec![Keyframe { position: 0, values: first }, Keyframe { position: 24, values: second }];

        match kind {
            TlType::Bodypart => {
                tl.model_part_name = "head".into();
                tl.part_of = ObjRef::Id(id(20));
                tl.parent = id(20);
            }
            TlType::Character => tl.parts = Some(vec![ObjRef::Id(id(25))]),
            TlType::Text => tl.text = "Hello\tworld".into(),
            TlType::SpecialBlock => {
                tl.part_of = ObjRef::Id(id(22));
                tl.part_root = ObjRef::Id(id(22));
                tl.part_model = Some(("banner".into(), vec![text_state("wall", "false")]));
                tl.pattern_type = "banner".into();
                tl.pattern =
                    Some(Pattern { base_color: "blue".into(), patterns: vec!["base".into()], colors: vec!["red".into()] });
            }
            TlType::Block => {
                tl.part_of = ObjRef::Id(id(22));
                tl.part_block = Some(("chest".into(), vec![text_state("facing", "east")]));
            }
            _ => {}
        }
        p.objects.timelines.push(tl);
    }

    for (n, &kind) in ResType::ALL.iter().enumerate() {
        let mut r = Resource::new(id(40 + n), kind);
        r.filename = format!("file {n}.png");
        match kind {
            ResType::Scenery => {
                r.scenery_tl_add = Some(true);
                r.scenery_download_skins = false;
                r.scenery_integrity = 0.5;
            }
            ResType::FromWorld => {
                r.scenery_tl_add = Some(false);
                r.world_regions_dir = "C:\\saves\\World\\region".into();
                r.world_box_start = Some([1.0, 2.0, 3.0]);
                r.world_box_end = Some([4.0, 5.0, 6.0]);
                r.world_filter_array = vec!["minecraft:stone".into()];
            }
            ResType::ItemSheet => r.item_sheet_size = [16.0, 8.0],
            ResType::Skin => r.player_skin = true,
            _ => {}
        }
        p.objects.resources.push(r);
    }

    p.markers = vec![
        Marker { id: id(90), position: 12.0, name: "Start".into(), color: 1.0 },
        Marker { id: id(91), position: 48.0, name: "End".into(), color: 4.0 },
    ];
    p
}

/// Settings that the file format does not store for a given kind of object
/// come back as defaults; this rebuilds what a load is expected to return.
fn as_stored(project: &ProjectFile) -> ProjectFile {
    let mut expected = project.clone();
    for tl in &mut expected.objects.timelines {
        let fresh = Timeline::new(tl.id.clone(), tl.kind, &project.defaults);
        let types = tl.kind.value_types(false);
        if !types.has(ValueType::Hierarchy) {
            tl.inherit = fresh.inherit.clone();
        }
        if !types.has(ValueType::RotPoint) {
            tl.rot_point = fresh.rot_point;
        }
        if !types.has(ValueType::Appearance) {
            tl.appearance = fresh.appearance.clone();
            tl.blend_mode = fresh.blend_mode.clone();
        }
        if !types.has(ValueType::Appearance) || !tl.kind.has_wind_settings() {
            tl.wind = fresh.wind;
        }
        if !types.has(ValueType::Path) {
            tl.path = fresh.path.clone();
        }
    }
    for resource in &mut expected.objects.resources {
        let fresh = Resource::new(resource.id.clone(), resource.kind);
        if !matches!(resource.kind, ResType::Scenery | ResType::FromWorld) {
            resource.scenery_download_skins = fresh.scenery_download_skins;
        }
    }
    expected
}

#[test]
fn full_project_round_trip() {
    let project = sample_project();
    let text = project.save();
    let loaded = load(&text);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);

    let expected = as_stored(&project);
    assert_eq!(loaded.file.info, expected.info);
    assert_eq!(loaded.file.render, expected.render);
    assert_eq!(loaded.file.background, expected.background);
    for (a, b) in loaded.file.objects.templates.iter().zip(&expected.objects.templates) {
        assert_eq!(a, b, "template {}", b.name);
    }
    for (a, b) in loaded.file.objects.timelines.iter().zip(&expected.objects.timelines) {
        assert_eq!(a, b, "timeline {}", b.name);
    }
    for (a, b) in loaded.file.objects.resources.iter().zip(&expected.objects.resources) {
        assert_eq!(a, b, "resource {}", b.filename);
    }
    assert_eq!(loaded.file, expected);

    // Writing the loaded project gives the same bytes again.
    assert_eq!(loaded.file.save(), text);
}

#[test]
fn layout_of_a_small_project() {
    let mut p = ProjectFile::new(1.0, 77.0);
    p.created_in = "2.0.2".into();
    let mut tl = Timeline::new(SaveId::new("AAAAAAAAAAAAAAAA"), TlType::Folder, &p.defaults);
    tl.name = "Folder".into();
    tl.parent_tree_index = Some(0);
    p.objects.timelines.push(tl);
    let text = p.save();

    assert!(text.starts_with(
        "{\r\n\t\"format\": 34,\r\n\t\"created_in\": \"2.0.2\",\r\n\t\"project\": {\r\n\t\t\"name\": \"\","
    ));
    assert!(text.contains(
        "\t\"templates\": [\r\n\t],\r\n\t\"timelines\": [\r\n\t\t{\r\n\t\t\t\"id\": \"AAAAAAAAAAAAAAAA\",\r\n\t\t\t\"type\": \"folder\","
    ));
    assert!(text.contains(
        "\t\t\t\"default_values\": {\r\n\t\t\t},\r\n\t\t\t\"keyframes\": {\r\n\t\t\t},\r\n\t\t\t\"parent\": \"root\",\r\n\t\t\t\"parent_tree_index\": 0,"
    ));
    assert!(text.ends_with("\t\"resources\": [\r\n\t]\r\n}"));
    assert!(!text.contains("markers"));
}

#[test]
fn newer_and_invalid_formats_are_rejected() {
    let mut new_id = || id(1);
    let newer = br#"{"format": 99, "created_in": "9.9"}"#;
    let err = ProjectFile::load(newer, LoadOptions { ground_slot: 0.0, seed: 1.0, new_id: &mut new_id }).unwrap_err();
    assert!(matches!(err, FormatError::TooNew { format: 99, .. }));

    let bad_files: [&[u8]; 4] = [br#"{"created_in": "x"}"#, br#"{"format": 5}"#, b"[1]", b"not json"];
    for bad in bad_files {
        let mut new_id = || id(1);
        assert!(ProjectFile::load(bad, LoadOptions { ground_slot: 0.0, seed: 1.0, new_id: &mut new_id }).is_err());
    }
}

#[test]
fn project_from_1_2_x_is_upgraded() {
    let text = r##"{
        "format": 30,
        "created_in": "1.2.2",
        "project": { "name": "Old", "tempo": 24, "timeline": { "show_seconds": true } },
        "background": { "sunlight_strength": 0, "foliage_color": "#112233", "sky_clouds_story_mode": true, "biome": "taiga" },
        "templates": [
            { "id": "T1", "type": "char", "name": "Steve", "model_tex": "default",
              "model": { "name": "human", "state": { "model": "steve" } } },
            { "id": "T2", "type": "particles", "name": "P",
              "particles": { "types": [ { "id": "P1", "name": "a", "temp": "null", "angle_speed": 5 } ] } }
        ],
        "timelines": [
            { "id": "L1", "type": "camera", "name": "Cam", "parent": "root", "parent_tree_index": 0,
              "keyframes": { "10": { "CAM_SHAKE": true, "CAM_SHAKE_HORIZONTAL_SPEED": 2, "BRIGHTNESS": 0.5 }, "0": {} } },
            { "id": "L2", "type": "background", "name": "Bg", "parent": "root", "parent_tree_index": 1,
              "keyframes": { "0": { "BG_SUNLIGHT_STRENGTH": 0.5 } } }
        ],
        "resources": [ { "id": "R1", "type": "schematic", "filename": "house.schematic" } ]
    }"##;
    let loaded = load(text);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let p = loaded.file;
    assert_eq!(p.loaded_format, fmt::FORMAT_122);

    assert!(p.info.timeline.intervals_show);
    assert_eq!(p.background.sunlight_strength, 1.0);
    assert_eq!(p.background.sky_clouds_mode, "faded");
    assert_eq!(p.background.leaves_oak_color, Color::from_hex("#112233"));

    let steve = &p.objects.templates[0];
    assert_eq!(steve.model_tex_material, ObjRef::default_resource());
    assert_eq!(steve.model_state, vec![text_state("model", "steve")]);
    let ptype = &p.objects.templates[1].particles.as_ref().unwrap().types[0];
    assert_eq!(ptype.source, ParticleSource::Sheet);
    assert_eq!(ptype.settings.angle_speed, 0.0);

    let cam = &p.objects.timelines[0];
    assert_eq!(cam.keyframes.iter().map(|k| k.position).collect::<Vec<_>>(), vec![0, 10]);
    let kf = &cam.keyframes[1].values;
    assert_eq!(kf[ValueId::CamShakeSpeedX], Value::Number(20.0));
    assert_eq!(kf[ValueId::CamShakeMode], Value::Number(1.0));
    assert_eq!(kf[ValueId::Emissive], Value::Number(0.5));
    assert_eq!(cam.alpha_mode, 0.0);

    let bg = &p.objects.timelines[1].keyframes[0].values;
    assert_eq!(bg[ValueId::BgSunlightStrength], Value::Number(1.5));
    assert_eq!(bg[ValueId::BgBiome], Value::Str("taiga".into()));
    assert_eq!(bg[ValueId::BgLeavesOakColor], p.defaults[ValueId::BgFoliageColor]);

    assert_eq!(p.objects.resources[0].kind, ResType::Scenery);
    assert_eq!(p.objects.resources[0].scenery_tl_add, Some(true));
}
