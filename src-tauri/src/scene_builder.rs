//! Turns an evaluated project into what the renderer draws.
//!
//! This is the only place that knows both the project model and the
//! renderer's scene description.

use mi_anim::scene::Inherited;
use mi_assets::{AssetPack, BendStyle, Rgba};
use mi_core::{Color, ObjRef, TempType, TlType, ValueId};
use mi_format::project::{Background, Template};
use mi_mesh::MeshData;
use mi_project::{ModelBindings, ModelTextures, Project, SceneryStore};
use mi_render::camera::CLIP_NEAR;
use mi_render::scene::{ColorTransform, Fog, PointLight, RenderObject, RenderScene, Tonemapper};
use mi_render::{ground_mesh, shape_mesh, Camera, MeshId, Shape, ShapeSettings, SkySettings, TextureId, WorkCamera};

/// Meshes and textures the scene needs. The caller keeps what it uploaded
/// and builds or loads something only the first time its key is asked for.
pub trait SceneResources {
    fn mesh(&mut self, key: String, build: &dyn Fn() -> MeshData) -> MeshId;
    /// Several meshes built together, each with a name (blocks give one
    /// mesh per texture).
    fn meshes(&mut self, key: String, build: &dyn Fn() -> Vec<(String, MeshData)>) -> Vec<(String, MeshId)>;
    fn texture(&mut self, key: String, load: &dyn Fn() -> Option<Rgba>) -> Option<TextureId>;
}

/// Everything besides the project that the scene is built from.
#[derive(Clone, Copy, Default)]
pub struct SceneInputs<'a> {
    pub pack: Option<&'a AssetPack>,
    pub bindings: Option<&'a ModelBindings>,
    pub scenery: Option<&'a SceneryStore>,
    /// Selected timelines; they and everything below them get an outline.
    pub selected: &'a [mi_core::SaveId],
    /// The font of text objects.
    pub font: Option<&'a mi_assets::SpriteFont>,
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
/// Drawn so far: shapes, blocks, body parts of characters and special
/// blocks, the textured ground, the sun, point and spot lights (as point lights, which
/// is what the low quality modes do), fog, scenery and text in the
/// Minecraft font, and items. Particles are not drawn yet.
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
    let scenery_size = |resource: &mi_core::SaveId| inputs.scenery.and_then(|s| s.get(resource)).map(|s| s.size());
    let (state, order) =
        project.evaluate_with(marker, &|i| bindings.and_then(|b| b.part_info(i)), &scenery_size);
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
        // Everything drawn for this timeline picks it.
        let first_object = objects.len();

        match timeline.kind {
            TlType::PointLight | TlType::SpotLight | TlType::Camera => {
                // Lights and cameras are clicked by a box around them.
                let mesh = resources.mesh("clickbox".to_owned(), &|| shape_mesh(Shape::Cube, &ShapeSettings::default()));
                let size = CLICK_BOX_SIZE / 16.0;
                let p = node.world_pos.map(|v| v as f32);
                let model = glam::Mat4::from_scale_rotation_translation(
                    glam::Vec3::splat(size),
                    glam::Quat::IDENTITY,
                    glam::Vec3::new(p[0], p[1], p[2]),
                );
                let mut click = RenderObject::new(mesh, model.to_cols_array());
                click.pick_only = true;
                click.backfaces = true;
                click.unlit = unlit;
                objects.push(click);
                if timeline.kind != TlType::Camera {
                    let color = rgb(node.values[ValueId::LightColor].as_color().unwrap_or(Color::WHITE));
                    let strength = node.values.number(ValueId::LightStrength) as f32;
                    lights.push(PointLight {
                        position: node.world_pos.map(|v| v as f32),
                        range: node.values.number(ValueId::LightRange) as f32,
                        color: color.map(|c| c * strength),
                    });
                }
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
            TlType::Block => {
                let Some(pack) = inputs.pack else { continue };
                if inherited.alpha <= 0.0 {
                    continue;
                }
                let template = timeline.temp.as_id().and_then(|id| project.template(id));
                // Blocks that are part of scenery name their block themselves.
                let (name, state, repeat, randomize) = match (&timeline.part_block, template) {
                    (Some((name, state)), template) => {
                        (name, state, [1.0; 3], template.is_none_or(|t| t.block_randomize))
                    }
                    (None, Some(t)) if t.kind == TempType::Block => {
                        let repeat = if t.block_repeat_enable { t.block_repeat } else { [1.0; 3] };
                        (&t.block_name, &t.block_state, repeat, t.block_randomize)
                    }
                    _ => continue,
                };
                let blocks = pack.blocks();
                let Some(block) = blocks.def(name) else { continue };
                let full_state = block.full_state(state);
                let repeat = repeat.map(|r| r.round().clamp(1.0, MAX_BLOCK_REPEAT));
                // The builder runs along Y first; the mesh is turned to
                // match, as the original does "for legacy support".
                let size = [repeat[1] as usize, repeat[0] as usize, repeat[2] as usize];
                let state_key: Vec<String> = full_state.iter().map(|(k, v)| format!("{k}={v}")).collect();
                let key = format!("block:{name}:{}:{size:?}:{randomize}", state_key.join(","));
                let meshes = resources.meshes(key, &|| blocks.grid_meshes(pack, block, &full_state, size, randomize));

                let matrix = block_turn(repeat[1]).then(&node.matrix_render).to_f32();
                let look = BlockLook { pack, background, inherited, timeline, unlit };
                push_block_objects(&mut objects, resources, &meshes, matrix, &look);
            }
            TlType::Item => {
                let Some(pack) = inputs.pack else { continue };
                let Some(template) = timeline.temp.as_id().and_then(|id| project.template(id)) else { continue };
                if inherited.alpha <= 0.0 {
                    continue;
                }
                // A keyframe can swap the item; otherwise it is the template's.
                let slot_name = |slot: f64| {
                    let list = pack.manifest().array("item_textures")?;
                    list.get(slot.max(0.0) as usize)?.as_str().map(str::to_owned)
                };
                let name = if node.values.flag(ValueId::CustomItemSlot) {
                    match node.values[ValueId::ItemName].as_str() {
                        Some(name) if !name.is_empty() => Some(name.to_owned()),
                        _ => slot_name(node.values.number(ValueId::ItemSlot)),
                    }
                } else {
                    template.item_name.clone().or_else(|| slot_name(template.item_slot))
                };
                let Some(name) = name else { continue };
                let is_3d = template.item.is_3d;
                let mesh = resources.mesh(format!("item:{name}:{is_3d}"), &|| {
                    pack.block_texture(&name).map(|image| mi_assets::item_mesh(&image, is_3d)).unwrap_or_default()
                });
                let mut object = RenderObject::new(mesh, node.matrix_render.to_f32());
                object.texture = resources.texture(format!("block:{name}"), &|| pack.block_texture(&name));
                apply_material(&mut object, inherited, Color::WHITE, 1.0);
                object.unlit = unlit;
                object.fog = timeline.appearance.fog;
                object.backfaces = timeline.appearance.backfaces;
                objects.push(object);
            }
            TlType::Text => {
                let Some(font) = inputs.font else { continue };
                if inherited.alpha <= 0.0 {
                    continue;
                }
                let text = match node.values[ValueId::Text].as_str() {
                    Some(text) if !text.is_empty() => text,
                    // Projects from before the text became a keyframe value.
                    _ => timeline.text.as_str(),
                };
                if text.is_empty() {
                    continue;
                }
                let align = |id: ValueId| mi_assets::Align::from_name(node.values[id].as_str().unwrap_or("center"));
                let (halign, valign) = (align(ValueId::TextHalign), align(ValueId::TextValign));
                let key = format!("text:{halign:?}:{valign:?}:{text}");
                let image = || mi_assets::text_image(font, text, halign, valign);
                let mesh = resources.mesh(key.clone(), &|| image().map(|t| mi_assets::text_mesh(&t)).unwrap_or_default());
                let mut object = RenderObject::new(mesh, node.matrix_render.to_f32());
                object.texture = resources.texture(key, &|| image().map(|t| t.image));
                apply_material(&mut object, inherited, Color::WHITE, 1.0);
                object.unlit = unlit;
                object.fog = timeline.appearance.fog;
                objects.push(object);
            }
            TlType::Scenery => {
                let Some(pack) = inputs.pack else { continue };
                let Some(template) = timeline.temp.as_id().and_then(|id| project.template(id)) else { continue };
                let Some(resource) = template.scenery.as_id() else { continue };
                let Some(loaded) = inputs.scenery.and_then(|s| s.get(resource)) else { continue };
                if inherited.alpha <= 0.0 {
                    continue;
                }
                let scenery = &loaded.scenery;
                let waves = render.liquid_animation;
                let key = format!("scenery:{resource}:{}:{}:{waves}", loaded.timelines, loaded.randomize);
                let meshes = resources.meshes(key, &|| scenery.meshes(pack, loaded.timelines, loaded.randomize, waves));

                let size = loaded.size();
                let repeat = if template.block_repeat_enable {
                    template.block_repeat.map(|r| r.round().clamp(1.0, MAX_BLOCK_REPEAT))
                } else {
                    [1.0; 3]
                };
                let copies = (repeat[0] * repeat[1] * repeat[2]) as usize;
                if copies > MAX_SCENERY_COPIES {
                    continue;
                }
                let look = BlockLook { pack, background, inherited, timeline, unlit };
                for rx in 0..repeat[0] as usize {
                    for ry in 0..repeat[1] as usize {
                        for rz in 0..repeat[2] as usize {
                            let offset =
                                [rx as f64 * size[0] * 16.0, ry as f64 * size[1] * 16.0, rz as f64 * size[2] * 16.0];
                            let matrix = block_turn(size[1])
                                .then(&mi_anim::Mat4::translation(offset))
                                .then(&node.matrix_render)
                                .to_f32();
                            push_block_objects(&mut objects, resources, &meshes, matrix, &look);
                        }
                    }
                }
            }
            _ => {}
        }
        // Children of a selected timeline are outlined with it
        // (`parent_is_selected`).
        let selected = !inputs.selected.is_empty() && {
            let mut current = Some(order[node_index]);
            let mut found = false;
            while let Some(index) = current {
                if inputs.selected.contains(&timelines[index].id) {
                    found = true;
                    break;
                }
                current = project.tree().parent(index);
            }
            found
        };
        for object in &mut objects[first_object..] {
            object.pick = order[node_index] as u32 + 1;
            object.selected = selected && !object.pick_only;
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

/// Size of the box lights and cameras are clicked by (`view_3d_box_size`).
const CLICK_BOX_SIZE: f32 = 12.0;

/// Largest repeat count along one axis of a block template.
const MAX_BLOCK_REPEAT: f64 = 256.0;

/// Largest number of copies of a repeated scenery that are drawn.
const MAX_SCENERY_COPIES: usize = 4096;

/// The turn every block mesh gets, "for legacy support" in the original
/// (`render_world_block`): the builder's X runs along -Y, shifted back by
/// the extent along Y (`size_y`, in blocks).
fn block_turn(size_y: f64) -> mi_anim::Mat4 {
    mi_anim::Mat4::build([0.0, size_y * 16.0, 0.0], [0.0, 0.0, 90.0], [1.0; 3])
}

/// What block objects of a timeline share.
struct BlockLook<'a> {
    pack: &'a AssetPack,
    background: &'a Background,
    inherited: &'a Inherited,
    timeline: &'a mi_format::project::Timeline,
    unlit: bool,
}

/// Adds an object per texture of a block mesh.
fn push_block_objects(
    objects: &mut Vec<RenderObject>,
    resources: &mut dyn SceneResources,
    meshes: &[(String, MeshId)],
    matrix: [f32; 16],
    look: &BlockLook,
) {
    let pack = look.pack;
    for (texture_name, mesh) in meshes {
        let mut object = RenderObject::new(*mesh, matrix);
        object.texture = resources.texture(format!("block:{texture_name}"), &|| pack.block_texture(texture_name));
        apply_material(&mut object, look.inherited, texture_tint(Some(pack), texture_name, look.background), 1.0);
        object.unlit = look.unlit;
        object.fog = look.timeline.appearance.fog;
        object.backfaces = look.timeline.appearance.backfaces;
        objects.push(object);
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
        fn meshes(&mut self, key: String, build: &dyn Fn() -> Vec<(String, MeshData)>) -> Vec<(String, MeshId)> {
            let built = build();
            self.keys.push(key);
            built.into_iter().enumerate().map(|(i, (name, _))| (name, MeshId::from_raw(1000 + i))).collect()
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

        // Ground plus the one visible sphere with a template; the lamp and
        // the camera are only there to be clicked.
        let drawn: Vec<&RenderObject> = scene.objects.iter().filter(|o| !o.pick_only).collect();
        assert_eq!(drawn.len(), 2);
        assert_eq!(scene.objects.iter().filter(|o| o.pick_only).count(), 2);
        assert!(keys[0].starts_with("ground:"));
        assert!(keys[1].starts_with("shape:Sphere") && keys[1].contains("detail: 12"), "{}", keys[1]);
        assert!(drawn[0].sun_only);
        let ball = drawn[1];
        // Clicking it picks its timeline (index + 1); the ground picks nothing.
        assert_eq!((drawn[0].pick, ball.pick), (0, 1));
        let lamp_box = scene.objects.iter().find(|o| o.pick == 5).unwrap();
        assert!(lamp_box.pick_only);
        assert_eq!(lamp_box.model[12], 50.0);
        assert_eq!(ball.blend_color, [1.0, 0.0, 0.0, 0.5]);
        // Shapes are lifted by their rotation point so they stand on
        // their position.
        assert_eq!(ball.model[14], 16.0);
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
        assert_eq!(scene.objects.iter().filter(|o| !o.pick_only).count(), 1);

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

        let inputs = SceneInputs { pack: Some(&pack), bindings: Some(&bindings), scenery: None, selected: &[], font: None };
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

    #[test]
    fn blocks_are_drawn_per_texture_and_repeated() {
        let pack = pack();
        let mut file = ProjectFile::new(1.0, 1.0);
        file.background.ground_show = false;
        let mut grass = Template::new(SaveId::new("GRASS"), TempType::Block);
        grass.block_name = "grass_block".into();
        grass.block_repeat_enable = true;
        grass.block_repeat = [3.0, 2.0, 1.0];
        file.objects.templates.push(grass);
        let mut tl = Timeline::new(SaveId::new("BLOCK"), TlType::Block, &file.defaults);
        tl.temp = ObjRef::id("GRASS");
        tl.parent_tree_index = Some(0);
        file.objects.timelines.push(tl);
        let project = Project::from_file(file, IdGenerator::new(3)).0;

        let inputs = SceneInputs { pack: Some(&pack), bindings: None, scenery: None, selected: &[], font: None };
        let (scene, recorder) = build_with(&project, inputs, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);
        assert_eq!(recorder.keys.len(), 1);
        assert!(recorder.keys[0].starts_with("block:grass_block:snowy=false:[2, 3, 1]"), "{}", recorder.keys[0]);
        // Top, side, side overlay and dirt bottom.
        assert!(scene.objects.len() >= 3, "{}", scene.objects.len());
        assert!(scene.objects.iter().all(|o| o.texture.is_some()));
        // The top is tinted with the grass colour, the bottom is not.
        let tints: Vec<[f32; 4]> = scene.objects.iter().map(|o| o.blend_color).collect();
        assert!(tints.iter().any(|t| *t != [1.0; 4]) && tints.contains(&[1.0; 4]), "{tints:?}");
        // Turned so the repeat runs 3 along X and 2 along Y, centred on the
        // timeline's position (the rotation point of block templates).
        let m = glam::Mat4::from_cols_array(&scene.objects[0].model);
        let corner = m.transform_point3(glam::Vec3::new(2.0 * 16.0, 0.0, 0.0));
        assert!((corner - glam::Vec3::new(-24.0, -16.0, 0.0)).length() < 1e-3, "{corner}");
        let corner = m.transform_point3(glam::Vec3::new(0.0, 3.0 * 16.0, 0.0));
        assert!((corner - glam::Vec3::new(24.0, 16.0, 0.0)).length() < 1e-3, "{corner}");
    }

    #[test]
    fn scenery_is_drawn_turned_and_repeated() {
        let pack = pack();
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/Data/legacy.midata");
        let legacy = mi_assets::LegacyBlocks::load(&std::fs::read(data).unwrap(), pack.blocks());
        // A .blocks file: two planks along X.
        let mut bytes = vec![0, 0];
        for size in [1u16, 2, 1] {
            bytes.extend(size.to_be_bytes());
        }
        bytes.extend([5, 0, 5, 0]);
        let scenery = mi_assets::Scenery::read(&bytes, "blocks", pack.blocks(), &legacy, Default::default()).unwrap();
        let mut store = SceneryStore::default();
        let loaded = mi_project::LoadedScenery { scenery: std::sync::Arc::new(scenery), timelines: true, randomize: false };
        store.insert(SaveId::new("RES"), loaded);

        let mut file = ProjectFile::new(1.0, 1.0);
        file.background.ground_show = false;
        let mut template = Template::new(SaveId::new("SCENERY"), TempType::Scenery);
        template.scenery = ObjRef::id("RES");
        template.block_repeat_enable = true;
        template.block_repeat = [1.0, 2.0, 1.0];
        file.objects.templates.push(template);
        let mut tl = Timeline::new(SaveId::new("TL"), TlType::Scenery, &file.defaults);
        tl.temp = ObjRef::id("SCENERY");
        tl.parent_tree_index = Some(0);
        file.objects.timelines.push(tl);
        let project = Project::from_file(file, IdGenerator::new(3)).0;

        let inputs = SceneInputs { pack: Some(&pack), bindings: None, scenery: Some(&store), selected: &[], font: None };
        let (scene, recorder) = build_with(&project, inputs, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);
        assert_eq!(recorder.keys, ["scenery:RES:true:false:true"]);
        // One texture, two copies along Y.
        assert_eq!(scene.objects.len(), 2);
        let m = glam::Mat4::from_cols_array(&scene.objects[0].model);
        // The builder's X (two blocks) runs along -Y, its Y along +X; the
        // whole repeat (1 x 4 blocks) is centred on the timeline.
        let centre = glam::Vec3::new(8.0, 32.0, 0.0);
        let p = m.transform_point3(glam::Vec3::new(32.0, 0.0, 0.0)) + centre;
        assert!(p.length() < 1e-3, "{p}");
        let p = m.transform_point3(glam::Vec3::new(0.0, 16.0, 0.0)) + centre;
        assert!((p - glam::Vec3::new(16.0, 32.0, 0.0)).length() < 1e-3, "{p}");
        // The copy starts where the first ends: the scenery is 2 blocks
        // along Y.
        let copy = glam::Mat4::from_cols_array(&scene.objects[1].model);
        let p = copy.transform_point3(glam::Vec3::new(32.0, 0.0, 0.0)) + centre;
        assert!((p - glam::Vec3::new(0.0, 32.0, 0.0)).length() < 1e-3, "{p}");
    }

    #[test]
    fn items_and_text_are_drawn() {
        let pack = pack();
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/Data/Fonts/minecraft.png");
        let font = mi_assets::SpriteFont::minecraft(&std::fs::read(data).unwrap()).unwrap();
        let mut file = ProjectFile::new(1.0, 1.0);
        file.background.ground_show = false;
        let mut sword = Template::new(SaveId::new("SWORD"), TempType::Item);
        sword.item_name = Some("item/diamond_sword".into());
        file.objects.templates.push(sword);
        file.objects.templates.push(Template::new(SaveId::new("LABEL"), TempType::Text));
        let mut item = Timeline::new(SaveId::new("ITEM"), TlType::Item, &file.defaults);
        item.temp = ObjRef::id("SWORD");
        item.parent_tree_index = Some(0);
        file.objects.timelines.push(item);
        let mut text = Timeline::new(SaveId::new("TEXT"), TlType::Text, &file.defaults);
        text.temp = ObjRef::id("LABEL");
        text.parent_tree_index = Some(1);
        text.default_values[ValueId::Text] = Value::Str("Hello".into());
        file.objects.timelines.push(text);
        let project = Project::from_file(file, IdGenerator::new(3)).0;

        let inputs = SceneInputs { pack: Some(&pack), font: Some(&font), ..Default::default() };
        let (scene, recorder) = build_with(&project, inputs, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);
        assert_eq!(recorder.keys, ["item:item/diamond_sword:true", "text:Center:Center:Hello"]);
        assert_eq!(recorder.textures, ["block:item/diamond_sword", "text:Center:Center:Hello"]);
        assert_eq!(scene.objects.len(), 2);
        assert!(scene.objects.iter().all(|o| o.texture.is_some()));
        // The item turns around the middle of its bottom edge.
        assert_eq!(scene.objects[0].model[12], -8.0);
    }
}
