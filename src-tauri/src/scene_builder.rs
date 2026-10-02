//! Turns an evaluated project into what the renderer draws.
//!
//! This is the only place that knows both the project model and the
//! renderer's scene description.

use mi_anim::scene::Inherited;
use mi_assets::{AssetPack, BendStyle, Rgba};
use mi_core::{Color, ObjRef, TempType, TlType, ValueId};
use mi_format::project::{Background, Template};
use mi_mesh::MeshData;
use mi_project::{ModelBindings, ModelTextures, Project};
use mi_render::camera::CLIP_NEAR;
use mi_render::scene::{ColorTransform, Fog, PointLight, RenderObject, RenderScene, Tonemapper};
use mi_render::{ground_mesh, shape_mesh, Camera, MeshId, Shape, ShapeSettings, SkySettings, TextureId, WorkCamera};

/// Meshes and textures the scene needs. The caller keeps what it uploaded
/// and builds or loads something only the first time its key is asked for.
pub trait SceneResources {
    fn mesh(&mut self, key: String, build: &dyn Fn() -> MeshData) -> MeshId;
    fn texture(&mut self, key: String, load: &dyn Fn() -> Option<Rgba>) -> Option<TextureId>;
}

/// Everything besides the project that the scene is built from.
#[derive(Clone, Copy, Default)]
pub struct SceneInputs<'a> {
    pub pack: Option<&'a AssetPack>,
    pub bindings: Option<&'a ModelBindings>,
}

/// How a viewport shows the scene (`e_view_mode`, without the high quality
/// mode, which is not implemented yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// No lighting.
    Flat,
    /// Sun and lights, per vertex.
    Shaded,
}

/// Which camera a viewport looks through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewCamera {
    Work(WorkCamera),
    /// The camera timeline that is active at the current frame; the work
    /// camera is used while there is none.
    Active(WorkCamera),
}

fn rgb(color: Color) -> [f32; 3] {
    [color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0]
}

fn shape_of(kind: TempType) -> Option<Shape> {
    Some(match kind {
        TempType::Cube => Shape::Cube,
        TempType::Cone => Shape::Cone,
        TempType::Cylinder => Shape::Cylinder,
        TempType::Sphere => Shape::Sphere,
        TempType::Surface => Shape::Surface,
        _ => return None,
    })
}

fn shape_settings(template: &Template) -> ShapeSettings {
    let s = &template.shape;
    ShapeSettings {
        tex_mapped: s.tex_mapped,
        tex_hoffset: s.tex_hoffset as f32,
        tex_voffset: s.tex_voffset as f32,
        tex_hrepeat: s.tex_hrepeat as f32,
        tex_vrepeat: s.tex_vrepeat as f32,
        tex_hmirror: s.tex_hmirror,
        tex_vmirror: s.tex_vmirror,
        closed: s.closed,
        invert: s.invert,
        detail: s.detail.clamp(3.0, 256.0) as u32,
    }
}

fn sky_settings(background: &Background, render_distance: f64) -> SkySettings {
    SkySettings {
        sky_time: background.sky_time as f32,
        sky_rotation: background.sky_rotation as f32,
        sunlight_strength: background.sunlight_strength as f32,
        sunlight_color: rgb(background.sunlight_color),
        ambient_color: rgb(background.ambient_color),
        night_color: rgb(background.night_color),
        sky_color: rgb(background.sky_color),
        twilight: background.twilight,
        render_distance: render_distance as f32,
    }
}

/// The colour a block texture is tinted with (`block_texture_get_blend`).
fn texture_tint(pack: Option<&AssetPack>, name: &str, background: &Background) -> Color {
    let kind = pack.and_then(|p| p.manifest().object("block_textures_color")).and_then(|m| m.string(name));
    match kind {
        Some("grass") => background.grass_color,
        Some("foliage") => background.foliage_color,
        Some("water") => background.water_color,
        Some("oak_leaves") => background.leaves_oak_color,
        Some("spruce_leaves") => background.leaves_spruce_color,
        Some("birch_leaves") => background.leaves_birch_color,
        Some("jungle_leaves") => background.leaves_jungle_color,
        Some("acacia_leaves") => background.leaves_acacia_color,
        Some("dark_oak_leaves") => background.leaves_dark_oak_color,
        Some("mangrove_leaves") => background.leaves_mangrove_color,
        Some(hex) if hex.starts_with('#') => Color::from_hex(hex),
        _ => Color::WHITE,
    }
}

fn pack_texture(resources: &mut dyn SceneResources, pack: Option<&AssetPack>, name: &str) -> Option<TextureId> {
    let pack = pack?;
    resources.texture(format!("pack:{name}"), &|| pack.texture(name))
}

/// Colour, transparency and surface values of an object, from what its
/// timeline inherited and the colour of the shape itself.
fn apply_material(object: &mut RenderObject, inherited: &Inherited, shape_color: Color, shape_alpha: f64) {
    let tint = rgb(inherited.rgb_mul);
    let own = rgb(shape_color);
    object.blend_color = [tint[0] * own[0], tint[1] * own[1], tint[2] * own[2], (inherited.alpha * shape_alpha) as f32];

    let black = Color::BLACK;
    let extended = inherited.rgb_add != black
        || inherited.rgb_sub != black
        || inherited.hsb_add != black
        || inherited.hsb_sub != black
        || inherited.hsb_mul != Color::WHITE
        || inherited.mix_percent > 0.0;
    if extended {
        object.colors = Some(ColorTransform {
            rgb_add: rgb(inherited.rgb_add),
            rgb_sub: rgb(inherited.rgb_sub),
            hsb_add: rgb(inherited.hsb_add),
            hsb_sub: rgb(inherited.hsb_sub),
            hsb_mul: rgb(inherited.hsb_mul),
            mix_color: rgb(inherited.mix_color),
            mix_percent: inherited.mix_percent as f32,
        });
    }
    object.metallic = inherited.metallic as f32;
    object.roughness = inherited.roughness as f32;
    object.emissive = inherited.emissive as f32;
}

/// Builds the scene of `project` at frame `marker`.
///
/// Drawn so far: shapes, body parts of characters and special blocks, the
/// textured ground, the sun, point and spot lights (as point lights, which
/// is what the low quality modes do) and fog. Blocks, scenery, items, text
/// and particles are not drawn yet.
pub fn build_scene(
    project: &Project,
    inputs: SceneInputs,
    marker: f64,
    view_camera: ViewCamera,
    mode: ViewMode,
    resources: &mut dyn SceneResources,
) -> RenderScene {
    let file = project.file();
    let background = &file.background;
    let render = &file.render;
    let bindings = inputs.bindings;
    let (state, order) = project.evaluate_with(marker, &|i| bindings.and_then(|b| b.part_info(i)));
    let timelines = project.timelines();
    let bend_style = BendStyle::from_name(&render.bend_style);

    // Camera
    let far = render.distance as f32;
    let (ViewCamera::Work(work) | ViewCamera::Active(work)) = view_camera;
    let mut camera = work.camera(far);
    if matches!(view_camera, ViewCamera::Active(_)) {
        if let Some(active) = project.active_camera(&state, &order) {
            let node = order.iter().position(|&i| i == active).expect("the active camera is part of the scene");
            let fov = state.nodes[node].values.number(ValueId::CamFov) as f32;
            camera = Camera::from_matrix(&state.nodes[node].matrix.to_f32(), fov, far);
            camera.near = CLIP_NEAR;
        }
    }

    // Draw order: by depth, then tree order (`tl_update_depth`).
    let mut draw_order: Vec<usize> = (0..order.len()).collect();
    draw_order.sort_by(|&a, &b| timelines[order[a]].depth.total_cmp(&timelines[order[b]].depth));

    let mut objects = Vec::new();
    let mut lights = Vec::new();
    let unlit = mode == ViewMode::Flat;

    // Ground
    if background.ground_show {
        let mesh = resources.mesh(format!("ground:{far}"), &|| ground_mesh(far));
        // The ground follows the camera in steps of one block.
        let snap = |v: f32| (v / 16.0).trunc() * 16.0;
        let model = glam::Mat4::from_translation(glam::Vec3::new(snap(camera.from.x), snap(camera.from.y), 0.0));
        let mut ground = RenderObject::new(mesh, model.to_cols_array());
        ground.texture = pack_texture(resources, inputs.pack, &background.ground_name);
        // Without the texture the ground shows the grass colour.
        let tint = if ground.texture.is_some() {
            rgb(texture_tint(inputs.pack, &background.ground_name, background))
        } else {
            rgb(background.grass_color)
        };
        ground.blend_color = [tint[0], tint[1], tint[2], 1.0];
        ground.sun_only = true;
        ground.unlit = unlit;
        objects.push(ground);
    }

    for node_index in draw_order {
        let timeline = &timelines[order[node_index]];
        let node = &state.nodes[node_index];
        let inherited = &node.inherited;
        if timeline.hide || !inherited.visible || timeline.lq_hiding {
            continue;
        }

        match timeline.kind {
            TlType::PointLight | TlType::SpotLight => {
                let color = rgb(node.values[ValueId::LightColor].as_color().unwrap_or(Color::WHITE));
                let strength = node.values.number(ValueId::LightStrength) as f32;
                lights.push(PointLight {
                    position: node.world_pos.map(|v| v as f32),
                    range: node.values.number(ValueId::LightRange) as f32,
                    color: color.map(|c| c * strength),
                });
            }
            kind if kind.is_shape() => {
                let template = match &timeline.temp {
                    ObjRef::Id(id) => project.template(id),
                    _ => None,
                };
                let Some(template) = template else { continue };
                let Some(shape) = shape_of(template.kind) else { continue };
                if inherited.alpha <= 0.0 {
                    continue;
                }

                let settings = shape_settings(template);
                let mesh = resources.mesh(format!("shape:{shape:?}:{settings:?}"), &|| shape_mesh(shape, &settings));
                let mut object = RenderObject::new(mesh, node.matrix_render.to_f32());
                apply_material(&mut object, inherited, Color::WHITE, 1.0);
                object.unlit = unlit;
                object.fog = timeline.appearance.fog;
                object.backfaces = timeline.appearance.backfaces;
                objects.push(object);
            }
            TlType::Bodypart => {
                let Some(binding) = bindings.and_then(|b| b.part(order[node_index])) else { continue };
                let Some(part) = binding.part() else { continue };
                if binding.hidden() || inherited.alpha <= 0.0 {
                    continue;
                }
                // The mesh depends on the bend, limited to the part's range.
                let mut bend = inherited.bend;
                if let Some(b) = &part.bend {
                    for (i, angle) in bend.iter_mut().enumerate() {
                        *angle = angle.clamp(b.direction_min[i], b.direction_max[i]);
                    }
                }
                let bend_key = bend.map(|a| (a * 100.0).round() as i64);

                for (index, shape) in part.shapes.iter().enumerate() {
                    if binding.model.shape_hide.contains(&shape.description) || shape.color.alpha <= 0.0 {
                        continue;
                    }
                    let key = format!("model:{}:{}:{index}:{bend_key:?}:{bend_style:?}", binding.model_key, part.name);
                    let mesh =
                        resources.mesh(key, &|| mi_assets::shape_mesh(shape, part.bend.as_ref(), bend, bend_style));
                    let matrix = mi_anim::Mat4::translation(shape.position).then(&node.matrix_render);
                    let mut object = RenderObject::new(mesh, matrix.to_f32());

                    let texture_name =
                        binding.model.shape_texture(&part.name, &shape.description, shape.texture_name.as_deref());
                    object.texture = match &binding.textures {
                        ModelTextures::Pack => pack_texture(resources, inputs.pack, &texture_name),
                        ModelTextures::Image(path) => resources.texture(format!("file:{}", path.display()), &|| {
                            std::fs::read(path).ok().and_then(|bytes| mi_assets::decode_square(&bytes))
                        }),
                    };
                    apply_material(&mut object, inherited, shape.color.blend, shape.color.alpha);
                    object.emissive = object.emissive.max(shape.color.emissive as f32);
                    object.unlit = unlit;
                    object.fog = timeline.appearance.fog;
                    object.backfaces = timeline.appearance.backfaces || part.backfaces;
                    objects.push(object);
                }
            }
            _ => {}
        }
    }

    let lighting = sky_settings(background, render.distance).lighting();
    let fog_color = if background.fog_color_custom { rgb(background.fog_color) } else { lighting.sky_color };
    RenderScene {
        camera,
        fog: Fog {
            show: background.fog_show,
            color: fog_color,
            distance: background.fog_distance as f32,
            size: background.fog_size as f32,
            height: background.fog_height as f32,
        },
        lighting,
        tonemapper: match render.tonemapper as i64 {
            1 => Tonemapper::Reinhard,
            2 => Tonemapper::Aces,
            _ => Tonemapper::None,
        },
        exposure: render.exposure as f32,
        gamma: render.gamma as f32,
        lights,
        objects,
    }
}

/// The work camera a project was saved with.
pub fn saved_work_camera(project: &Project) -> WorkCamera {
    let saved = &project.file().info.work_camera;
    WorkCamera::orbiting(
        glam::Vec3::new(saved.focus[0] as f32, saved.focus[1] as f32, saved.focus[2] as f32),
        saved.angle_xy as f32,
        saved.angle_z as f32,
        saved.roll as f32,
        saved.zoom as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::{IdGenerator, SaveId, Value};
    use mi_format::project::{ProjectFile, Timeline};
    use mi_format::StateValue;

    #[derive(Default)]
    struct Recorder {
        keys: Vec<String>,
        textures: Vec<String>,
    }

    impl SceneResources for Recorder {
        fn mesh(&mut self, key: String, build: &dyn Fn() -> MeshData) -> MeshId {
            assert!(build().vertices.len() % 3 == 0);
            self.keys.push(key);
            MeshId::from_raw(self.keys.len() - 1)
        }
        fn texture(&mut self, key: String, load: &dyn Fn() -> Option<Rgba>) -> Option<TextureId> {
            let loaded = load().is_some();
            self.textures.push(key);
            loaded.then(|| TextureId::from_raw(self.textures.len() - 1))
        }
    }

    fn project() -> Project {
        let mut file = ProjectFile::new(0.0, 1.0);
        let mut template = Template::new(SaveId::new("TEMPLATE"), TempType::Sphere);
        template.shape.detail = 12.0;
        file.objects.templates.push(template);

        let mut add = |id: &str, kind: TlType, index: i64, edit: &dyn Fn(&mut Timeline)| {
            let mut tl = Timeline::new(SaveId::new(id), kind, &file.defaults);
            tl.parent_tree_index = Some(index);
            if kind.is_shape() {
                tl.temp = ObjRef::id("TEMPLATE");
            }
            edit(&mut tl);
            file.objects.timelines.push(tl);
        };
        add("BALL", TlType::Sphere, 0, &|tl| {
            tl.default_values[ValueId::PosZ] = Value::Number(8.0);
            tl.default_values[ValueId::RgbMul] = Value::Color(Color::rgb(255, 0, 0));
            tl.default_values[ValueId::Alpha] = Value::Number(0.5);
        });
        add("HIDDEN", TlType::Sphere, 1, &|tl| tl.hide = true);
        add("INVISIBLE", TlType::Sphere, 2, &|tl| tl.default_values[ValueId::Visible] = Value::Bool(false));
        add("NO_TEMPLATE", TlType::Cube, 3, &|tl| tl.temp = ObjRef::Null);
        add("LAMP", TlType::PointLight, 4, &|tl| {
            tl.default_values[ValueId::PosX] = Value::Number(50.0);
            tl.default_values[ValueId::LightStrength] = Value::Number(2.0);
        });
        add("CAM", TlType::Camera, 5, &|tl| {
            tl.default_values[ValueId::PosY] = Value::Number(-200.0);
            tl.default_values[ValueId::CamFov] = Value::Number(70.0);
        });
        Project::from_file(file, IdGenerator::new(3)).0
    }

    fn build_with(project: &Project, inputs: SceneInputs, camera: ViewCamera, mode: ViewMode) -> (RenderScene, Recorder) {
        let mut recorder = Recorder::default();
        let scene = build_scene(project, inputs, 0.0, camera, mode, &mut recorder);
        (scene, recorder)
    }

    fn build(project: &Project, camera: ViewCamera, mode: ViewMode) -> (RenderScene, Vec<String>) {
        let (scene, recorder) = build_with(project, SceneInputs::default(), camera, mode);
        (scene, recorder.keys)
    }

    #[test]
    fn shapes_ground_and_lights_are_collected() {
        let project = project();
        let (scene, keys) = build(&project, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);

        // Ground plus the one visible sphere with a template.
        assert_eq!(scene.objects.len(), 2);
        assert!(keys[0].starts_with("ground:"));
        assert!(keys[1].starts_with("shape:Sphere") && keys[1].contains("detail: 12"), "{}", keys[1]);
        assert!(scene.objects[0].sun_only);
        let ball = &scene.objects[1];
        assert_eq!(ball.blend_color, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(ball.model[14], 8.0);
        assert!(!ball.unlit);

        assert_eq!(scene.lights.len(), 1);
        assert_eq!(scene.lights[0].position, [50.0, 0.0, 0.0]);
        assert_eq!(scene.lights[0].color, [2.0, 2.0, 2.0]);
        assert_eq!(scene.lights[0].range, 250.0);

        assert_eq!(scene.gamma, 2.2);
        assert!(scene.fog.show);
        assert_eq!(scene.camera.fov, WorkCamera::FOV);
    }

    #[test]
    fn flat_mode_turns_lighting_off() {
        let project = project();
        let (scene, _) = build(&project, ViewCamera::Work(WorkCamera::default()), ViewMode::Flat);
        assert!(scene.objects.iter().all(|o| o.unlit));
    }

    #[test]
    fn active_camera_is_used_when_asked_for() {
        let project = project();
        let (scene, _) = build(&project, ViewCamera::Active(WorkCamera::default()), ViewMode::Shaded);
        assert_eq!(scene.camera.from, glam::Vec3::new(0.0, -200.0, 0.0));
        assert_eq!(scene.camera.to, glam::Vec3::new(0.0, -199.0, 0.0));
        assert_eq!(scene.camera.fov, 70.0);
    }

    #[test]
    fn ground_can_be_hidden_and_follows_the_camera() {
        let mut file = project().file().clone();
        file.background.ground_show = false;
        let hidden = Project::from_file(file, IdGenerator::new(3)).0;
        let (scene, _) = build(&hidden, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);
        assert_eq!(scene.objects.len(), 1);

        let far_away = WorkCamera::orbiting(glam::Vec3::new(1000.0, -500.0, 0.0), 0.0, 0.0, 0.0, 1.0);
        let (scene, _) = build(&project(), ViewCamera::Work(far_away), ViewMode::Shaded);
        let ground = &scene.objects[0];
        assert_eq!(ground.model[12] % 16.0, 0.0);
        assert!((ground.model[12] - 1000.0).abs() <= 16.0 && (ground.model[13] + 500.0).abs() <= 16.0);
    }

    fn pack() -> AssetPack {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/Data/Minecraft");
        AssetPack::open(&folder, "1.20.2").unwrap()
    }

    #[test]
    fn characters_are_drawn_from_the_asset_pack() {
        let pack = pack();
        let mut file = ProjectFile::new(1.0, 1.0);
        let mut steve = Template::new(SaveId::new("STEVE"), TempType::Character);
        steve.model_state =
            vec![("type".into(), StateValue::Str("wide".into())), ("variant".into(), StateValue::Str("steve".into()))];
        file.objects.templates.push(steve);

        let mut character = Timeline::new(SaveId::new("CHAR"), TlType::Character, &file.defaults);
        character.temp = ObjRef::id("STEVE");
        character.parent_tree_index = Some(0);
        file.objects.timelines.push(character);
        for (i, part) in ["body", "head", "left_arm"].iter().enumerate() {
            let mut tl = Timeline::new(SaveId::new(format!("PART{i}")), TlType::Bodypart, &file.defaults);
            tl.temp = ObjRef::id("STEVE");
            tl.part_of = ObjRef::id("CHAR");
            // As in real projects, parts hang below their parent part.
            tl.parent = SaveId::new(if i == 0 { "CHAR" } else { "PART0" });
            tl.parent_tree_index = Some(i as i64);
            tl.model_part_name = (*part).into();
            if *part == "left_arm" {
                tl.default_values[ValueId::BendAngleX] = Value::Number(45.0);
            }
            file.objects.timelines.push(tl);
        }
        let project = Project::from_file(file, IdGenerator::new(3)).0;
        let bindings = ModelBindings::bind(&project, &pack);
        assert_eq!(bindings.len(), 3);

        let inputs = SceneInputs { pack: Some(&pack), bindings: Some(&bindings) };
        let (scene, recorder) = build_with(&project, inputs, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);
        // Ground plus the shapes of three parts, all textured.
        assert!(scene.objects.len() > 4, "{}", scene.objects.len());
        assert!(scene.objects.iter().all(|o| o.texture.is_some()));
        assert!(recorder.textures.iter().any(|t| t == "pack:entity/player/wide/steve"), "{:?}", recorder.textures);
        assert!(recorder.textures.iter().any(|t| t == "pack:block/grass_block_top"));
        // The head sits on top of the body: its shapes are higher.
        let heights: Vec<f32> = scene.objects.iter().skip(1).map(|o| o.model[14]).collect();
        assert!(heights.iter().any(|&z| z >= 24.0), "{heights:?}");
        // The bent arm has its own mesh.
        assert!(recorder.keys.iter().any(|k| k.contains("left_arm") && k.contains("4500")), "{:?}", recorder.keys);
    }
}
