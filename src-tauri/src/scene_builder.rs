//! Turns an evaluated project into what the renderer draws.
//!
//! This is the only place that knows both the project model and the
//! renderer's scene description.

use mi_core::{Color, ObjRef, TempType, TlType, ValueId};
use mi_format::project::{Background, Template};
use mi_project::Project;
use mi_render::camera::CLIP_NEAR;
use mi_render::scene::{ColorTransform, Fog, PointLight, RenderObject, RenderScene, Tonemapper};
use mi_render::{Camera, MeshId, Shape, ShapeSettings, SkySettings, WorkCamera};

/// Meshes the scene refers to. The caller owns the uploaded meshes and
/// resolves keys to them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeshKey {
    Shape(Shape, ShapeSettings),
    /// The ground plane for a given render distance.
    Ground(f32),
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

/// Builds the scene of `project` at frame `marker`.
///
/// Drawn so far: shapes, the ground (in the biome's grass colour, since
/// textures are not loaded yet), the sun, point and spot lights (as point
/// lights, which is what the low quality modes do) and fog. Models, blocks,
/// items, text and particles need the Minecraft assets.
pub fn build_scene(
    project: &Project,
    marker: f64,
    view_camera: ViewCamera,
    mode: ViewMode,
    resolve_mesh: &mut dyn FnMut(MeshKey) -> MeshId,
) -> RenderScene {
    let file = project.file();
    let background = &file.background;
    let render = &file.render;
    let (state, order) = project.evaluate(marker);
    let timelines = project.timelines();

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
        let mesh = resolve_mesh(MeshKey::Ground(far));
        // The ground follows the camera in steps of one block.
        let snap = |v: f32| (v / 16.0).trunc() * 16.0;
        let model = glam::Mat4::from_translation(glam::Vec3::new(snap(camera.from.x), snap(camera.from.y), 0.0));
        let mut ground = RenderObject::new(mesh, model.to_cols_array());
        let grass = rgb(background.grass_color);
        ground.blend_color = [grass[0], grass[1], grass[2], 1.0];
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

                let mesh = resolve_mesh(MeshKey::Shape(shape, shape_settings(template)));
                let mut object = RenderObject::new(mesh, node.matrix_render.to_f32());
                let tint = rgb(inherited.rgb_mul);
                object.blend_color = [tint[0], tint[1], tint[2], inherited.alpha as f32];

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
                object.unlit = unlit;
                object.fog = timeline.appearance.fog;
                object.backfaces = timeline.appearance.backfaces;
                objects.push(object);
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

    fn build(project: &Project, camera: ViewCamera, mode: ViewMode) -> (RenderScene, Vec<MeshKey>) {
        let mut keys = Vec::new();
        let scene = build_scene(project, 0.0, camera, mode, &mut |key| {
            keys.push(key);
            MeshId::from_raw(keys.len() - 1)
        });
        (scene, keys)
    }

    #[test]
    fn shapes_ground_and_lights_are_collected() {
        let project = project();
        let (scene, keys) = build(&project, ViewCamera::Work(WorkCamera::default()), ViewMode::Shaded);

        // Ground plus the one visible sphere with a template.
        assert_eq!(scene.objects.len(), 2);
        assert!(matches!(keys[0], MeshKey::Ground(_)));
        assert!(matches!(keys[1], MeshKey::Shape(Shape::Sphere, settings) if settings.detail == 12));
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
}
