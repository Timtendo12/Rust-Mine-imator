//! Renders a project to an image without the window, for checking by eye
//! what the viewport would show. It only runs when asked to:
//!
//! ```text
//! MI_RENDER_PROJECT=<file.miproject> MI_RENDER_OUT=<file.png> cargo test -p mine-imator --lib render_check
//! ```
//!
//! Optional: `MI_RENDER_FRAME`, `MI_RENDER_FOCUS=x,y,z` or
//! `MI_RENDER_LOOK_AT=<save id>`, `MI_RENDER_ANGLE`,
//! `MI_RENDER_ELEV`, `MI_RENDER_ZOOM` (otherwise the saved work camera is
//! used), `MI_RENDER_SELECT=<save id or display type>` to outline timelines
//! and `MI_RENDER_PICK=x,y` to print what a click there picks.

use crate::scene_builder::{build_scene, saved_work_camera, SceneInputs, ViewCamera, ViewMode};
use crate::viewport::render_resources;
use mi_render::{wgpu, OffscreenTarget, Renderer, WorkCamera};

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

#[test]
fn render_check() {
    let Ok(path) = std::env::var("MI_RENDER_PROJECT") else { return };
    let out = std::env::var("MI_RENDER_OUT").expect("MI_RENDER_OUT names the image to write");
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/Data");
    let pack = mi_assets::AssetPack::open(&data.join("Minecraft"), mi_core::version::MINECRAFT_VERSION).unwrap();
    let legacy = mi_assets::LegacyBlocks::load(&std::fs::read(data.join("legacy.midata")).unwrap(), pack.blocks());
    let (project, _) = mi_project::Project::open(std::path::Path::new(&path), Default::default()).unwrap();
    let bindings = mi_project::ModelBindings::bind(&project, &pack);
    let scenery = mi_project::SceneryStore::load(&project, &pack, &legacy);

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let (_, device, queue) = pollster::block_on(mi_render::request_device(&instance, None)).unwrap();
    let mut renderer = Renderer::new(&device, &queue, OffscreenTarget::FORMAT);

    let saved = saved_work_camera(&project);
    let orbit = ["MI_RENDER_FOCUS", "MI_RENDER_LOOK_AT", "MI_RENDER_ANGLE", "MI_RENDER_ELEV", "MI_RENDER_ZOOM"];
    let camera = if orbit.iter().any(|name| std::env::var(name).is_ok()) {
        let look_at = std::env::var("MI_RENDER_LOOK_AT").ok().and_then(|id| {
            let index = project.timeline_index(&mi_core::SaveId::new(&id))?;
            let (state, order) = project.evaluate_with(
                env("MI_RENDER_FRAME").unwrap_or(0.0),
                &|i| bindings.part_info(i),
                &|resource| scenery.get(resource).map(|s| s.size()),
            );
            let node = order.iter().position(|&i| i == index)?;
            let p = state.nodes[node].world_pos;
            Some(glam::Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
        });
        let focus = look_at.or_else(|| {
            let v: Vec<f32> = std::env::var("MI_RENDER_FOCUS").ok()?.split(',').filter_map(|c| c.trim().parse().ok()).collect();
            Some(glam::Vec3::new(v[0], v[1], v[2]))
        });
        let focus = focus.unwrap_or(saved.focus);
        WorkCamera::orbiting(
            focus,
            env("MI_RENDER_ANGLE").unwrap_or(saved.angle_xy),
            env("MI_RENDER_ELEV").unwrap_or(saved.angle_z),
            0.0,
            env("MI_RENDER_ZOOM").unwrap_or(saved.zoom),
        )
    } else {
        saved
    };

    // Timelines to outline, by save id or by type name ("char", "scenery").
    let selected: Vec<mi_core::SaveId> = std::env::var("MI_RENDER_SELECT")
        .ok()
        .map(|wanted| {
            project
                .timelines()
                .iter()
                .filter(|t| t.id.as_str() == wanted || t.kind.name() == wanted)
                .map(|t| t.id.clone())
                .collect()
        })
        .unwrap_or_default();

    let font = mi_assets::SpriteFont::minecraft(&std::fs::read(data.join("Fonts/minecraft.png")).unwrap()).unwrap();
    let inputs = SceneInputs {
        pack: Some(&pack),
        bindings: Some(&bindings),
        scenery: Some(&scenery),
        selected: &selected,
        font: Some(&font),
        backdrop: true,
    };
    let mut cache = Default::default();
    let frame = env("MI_RENDER_FRAME").unwrap_or(0.0);
    let scene = {
        let mut resources = render_resources(&mut renderer, &mut cache);
        build_scene(&project, inputs, frame, ViewCamera::Work(camera), ViewMode::Shaded, &mut resources)
    };

    let (width, height) = (960, 540);
    let target = OffscreenTarget::new(&device, &queue, &renderer, width, height);
    renderer.render(&target.color, &target.depth, target.viewport(), &scene, true);
    let pixels = target.read_rgba().unwrap();
    image::save_buffer(&out, &pixels, width, height, image::ColorType::Rgba8).unwrap();
    eprintln!("{} objects, {} outlined", scene.objects.len(), scene.objects.iter().filter(|o| o.selected).count());

    if let Ok(at) = std::env::var("MI_RENDER_PICK") {
        let v: Vec<u32> = at.split(',').filter_map(|c| c.trim().parse().ok()).collect();
        let picked = renderer.pick(&scene, width, height, v[0], v[1]).unwrap();
        let name = picked.map(|id| {
            let timeline = &project.timelines()[id as usize - 1];
            format!("{} {} {:?}", timeline.kind.name(), timeline.id, timeline.name)
        });
        eprintln!("pick at {at}: {name:?}");
    }
}
