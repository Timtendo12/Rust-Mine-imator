//! Renders small scenes into memory and checks the pixels.
//!
//! These tests need a graphics adapter. On machines without one (some CI
//! runners) they report that and pass without checking anything.

use glam::{Mat4, Vec3};
use mi_render::{
    request_device, shape_mesh, wgpu, Camera, ColorTransform, Fog, OffscreenTarget, PointLight, RenderObject,
    RenderScene, Renderer, Shape, ShapeSettings, SkySettings, TextureFilter, Tonemapper,
};

const SIZE: u32 = 128;
const SKY: [u8; 4] = [120, 167, 255, 255];

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    match pollster::block_on(request_device(&instance, None)) {
        Ok((_, device, queue)) => Some(Gpu { device, queue }),
        Err(error) => {
            eprintln!("skipping: {error}");
            None
        }
    }
}

/// A scene at noon without fog, seen from -Y looking at the origin.
fn scene(objects: Vec<RenderObject>) -> RenderScene {
    RenderScene {
        camera: Camera {
            from: Vec3::new(0.0, -60.0, 0.0),
            to: Vec3::ZERO,
            up: Vec3::Z,
            fov: 45.0,
            near: 1.0,
            far: 1000.0,
        },
        lighting: SkySettings { sky_time: 0.0, ..Default::default() }.lighting(),
        background: SkySettings::default().sky_color,
        wind: Default::default(),
        fog: Fog { show: false, color: [1.0; 3], distance: 1000.0, size: 100.0, height: 1000.0 },
        tonemapper: Tonemapper::None,
        exposure: 1.0,
        gamma: 2.2,
        lights: Vec::new(),
        objects,
    }
}

fn render(gpu: &Gpu, renderer: &mut Renderer, scene: &RenderScene) -> Vec<u8> {
    let target = OffscreenTarget::new(&gpu.device, &gpu.queue, renderer, SIZE, SIZE);
    renderer.render(&target.color, &target.depth, target.viewport(), scene, true);
    target.read_rgba().unwrap()
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [image[i], image[i + 1], image[i + 2], image[i + 3]]
}

fn centre(image: &[u8]) -> [u8; 4] {
    pixel(image, SIZE / 2, SIZE / 2)
}

fn brightness(p: [u8; 4]) -> u32 {
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

fn translation(x: f32, y: f32, z: f32) -> [f32; 16] {
    Mat4::from_translation(Vec3::new(x, y, z)).to_cols_array()
}

fn cube_renderer(gpu: &Gpu) -> (Renderer, mi_render::MeshId) {
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, OffscreenTarget::FORMAT);
    let cube = renderer.add_mesh(&shape_mesh(Shape::Cube, &ShapeSettings::default()));
    (renderer, cube)
}

fn flat(mesh: mi_render::MeshId, model: [f32; 16], color: [f32; 4]) -> RenderObject {
    let mut object = RenderObject::new(mesh, model);
    object.unlit = true;
    object.blend_color = color;
    object
}

#[test]
fn empty_scene_is_sky() {
    let Some(gpu) = gpu() else { return };
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, OffscreenTarget::FORMAT);
    let image = render(&gpu, &mut renderer, &scene(Vec::new()));
    assert_eq!(pixel(&image, 0, 0), SKY);
    assert_eq!(centre(&image), SKY);
}

#[test]
fn cube_is_drawn_in_the_middle_and_lit_from_above() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);

    // Looking down at an angle so that the top and the front are both visible.
    let mut s = scene(vec![RenderObject::new(cube, translation(0.0, 0.0, 0.0))]);
    s.camera.from = Vec3::new(0.0, -60.0, 40.0);
    let image = render(&gpu, &mut renderer, &s);

    assert_eq!(pixel(&image, 2, 2), SKY);
    assert_ne!(centre(&image), SKY, "the cube covers the centre");

    // With the sun overhead the top face is lit and the front face only
    // receives ambient light.
    let top = pixel(&image, SIZE / 2, SIZE / 2 - 12);
    let front = pixel(&image, SIZE / 2, SIZE / 2 + 12);
    assert_ne!(top, SKY);
    assert_ne!(front, SKY);
    assert!(brightness(top) > brightness(front) + 100, "top {top:?}, front {front:?}");
}

#[test]
fn unlit_objects_show_their_plain_colour() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let object = flat(cube, translation(0.0, 0.0, 0.0), [1.0, 0.5, 0.25, 1.0]);
    let c = centre(&render(&gpu, &mut renderer, &scene(vec![object])));
    let near = |a: u8, b: i32| (a as i32 - b).abs() <= 1;
    assert!(near(c[0], 255) && near(c[1], 128) && near(c[2], 64), "{c:?}");
}

#[test]
fn nearer_objects_hide_farther_ones_regardless_of_order() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let red_near = flat(cube, translation(0.0, -20.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
    let blue_far = flat(cube, translation(0.0, 20.0, 0.0), [0.0, 0.0, 1.0, 1.0]);

    for objects in [vec![red_near.clone(), blue_far.clone()], vec![blue_far, red_near]] {
        let image = render(&gpu, &mut renderer, &scene(objects));
        assert_eq!(centre(&image), [255, 0, 0, 255]);
    }
}

#[test]
fn positive_x_is_on_the_left() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let object = flat(cube, translation(16.0, 0.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
    let image = render(&gpu, &mut renderer, &scene(vec![object]));
    assert_eq!(pixel(&image, SIZE / 2 - 40, SIZE / 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, SIZE / 2 + 40, SIZE / 2), SKY);
}

#[test]
fn back_faces_are_culled_unless_requested() {
    let Some(gpu) = gpu() else { return };
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, OffscreenTarget::FORMAT);
    // A surface faces +Y, away from the camera at -Y.
    let surface = renderer.add_mesh(&shape_mesh(Shape::Surface, &ShapeSettings::default()));
    let mut object = flat(surface, translation(0.0, 0.0, 0.0), [0.0, 1.0, 0.0, 1.0]);

    let image = render(&gpu, &mut renderer, &scene(vec![object.clone()]));
    assert_eq!(centre(&image), SKY);

    object.backfaces = true;
    let image = render(&gpu, &mut renderer, &scene(vec![object.clone()]));
    assert_eq!(centre(&image), [0, 255, 0, 255]);

    // Seen from the other side it is visible without that.
    object.backfaces = false;
    let mut s = scene(vec![object]);
    s.camera.from = Vec3::new(0.0, 60.0, 0.0);
    let image = render(&gpu, &mut renderer, &s);
    assert_eq!(centre(&image), [0, 255, 0, 255]);
}

#[test]
fn point_lights_brighten_what_they_reach() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let mut s = scene(vec![RenderObject::new(cube, translation(0.0, 0.0, 0.0))]);
    // Midnight: no sun, dark ambient.
    s.lighting = SkySettings { sky_time: 180.0, ..Default::default() }.lighting();
    let dark = centre(&render(&gpu, &mut renderer, &s));

    s.lights.push(PointLight { position: [0.0, -30.0, 0.0], range: 100.0, color: [1.0, 1.0, 1.0] });
    let lit = centre(&render(&gpu, &mut renderer, &s));
    assert!(brightness(lit) > brightness(dark) + 200, "dark {dark:?}, lit {lit:?}");

    // Out of range it has no effect.
    s.lights[0].range = 5.0;
    let out_of_range = centre(&render(&gpu, &mut renderer, &s));
    assert_eq!(out_of_range, dark);
}

#[test]
fn fog_alpha_textures_and_colour_transforms() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);

    // Fog swallows distant objects.
    let mut object = flat(cube, translation(0.0, 0.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
    let mut s = scene(vec![object.clone()]);
    s.fog = Fog { show: true, color: [0.0, 1.0, 0.0], distance: 10.0, size: 5.0, height: 1000.0 };
    assert_eq!(centre(&render(&gpu, &mut renderer, &s)), [0, 255, 0, 255]);
    s.objects[0].fog = false;
    assert_eq!(centre(&render(&gpu, &mut renderer, &s)), [255, 0, 0, 255]);

    // Half transparent red over the sky.
    object.blend_color = [1.0, 0.0, 0.0, 0.5];
    let blended = centre(&render(&gpu, &mut renderer, &scene(vec![object.clone()])));
    assert!(blended[0] > 150 && blended[2] > 100 && blended[2] < 160, "{blended:?}");

    // A texture replaces the white default.
    let texture = renderer.add_texture(&[0, 0, 255, 255], 1, 1, TextureFilter::Nearest);
    object.blend_color = [1.0; 4];
    object.texture = Some(texture);
    assert_eq!(centre(&render(&gpu, &mut renderer, &scene(vec![object.clone()]))), [0, 0, 255, 255]);

    // Mixing fully with a colour replaces it again.
    object.colors = Some(ColorTransform {
        rgb_add: [0.0; 3],
        rgb_sub: [0.0; 3],
        hsb_add: [0.0; 3],
        hsb_sub: [0.0; 3],
        hsb_mul: [1.0; 3],
        mix_color: [1.0, 1.0, 0.0],
        mix_percent: 1.0,
    });
    assert_eq!(centre(&render(&gpu, &mut renderer, &scene(vec![object]))), [255, 255, 0, 255]);
}

#[test]
fn many_objects_grow_the_uniform_buffer() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let empty = renderer.add_mesh(&Default::default());
    let mut objects: Vec<RenderObject> =
        (0..600).map(|i| flat(cube, translation(0.0, 500.0, 0.0), [0.0, 0.0, (i % 2) as f32, 1.0])).collect();
    // Empty meshes are skipped and must not shift the others.
    objects.insert(0, RenderObject::new(empty, translation(0.0, 0.0, 0.0)));
    objects.push(flat(cube, translation(0.0, 0.0, 0.0), [1.0, 0.0, 0.0, 1.0]));
    let image = render(&gpu, &mut renderer, &scene(objects));
    assert_eq!(centre(&image), [255, 0, 0, 255]);
}

#[test]
fn picking_finds_the_nearest_object_under_a_pixel() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    // The world is left-handed: looking along +Y, +X is on the left.
    let mut left = flat(cube, translation(12.0, 0.0, 0.0), [1.0; 4]);
    left.pick = 7;
    let mut right = flat(cube, translation(-12.0, 0.0, 0.0), [1.0; 4]);
    right.pick = 9;
    // The ground and other unselectable objects are not picked.
    let mut behind = flat(cube, translation(0.0, 40.0, 0.0), [1.0; 4]);
    behind.pick = 0;
    let s = scene(vec![left, right, behind]);

    let pick = |renderer: &mut Renderer, x, y| renderer.pick(&s, SIZE, SIZE, x, y).unwrap();
    assert_eq!(pick(&mut renderer, SIZE / 2 - 31, SIZE / 2), Some(7));
    assert_eq!(pick(&mut renderer, SIZE / 2 + 31, SIZE / 2), Some(9));
    assert_eq!(pick(&mut renderer, SIZE / 2, SIZE / 2), None);
    assert_eq!(pick(&mut renderer, 0, 0), None);
    assert_eq!(pick(&mut renderer, SIZE, 0), None);

    // Of two objects under the same pixel the nearer wins, in any order.
    let mut near = flat(cube, translation(0.0, -20.0, 0.0), [1.0; 4]);
    near.pick = 1;
    let mut far = flat(cube, translation(0.0, 20.0, 0.0), [1.0; 4]);
    far.pick = 2;
    for objects in [vec![near.clone(), far.clone()], vec![far, near]] {
        let s = scene(objects);
        assert_eq!(renderer.pick(&s, SIZE, SIZE, SIZE / 2, SIZE / 2).unwrap(), Some(1));
    }
}

#[test]
fn selected_objects_get_a_white_border() {
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let red = [1.0, 0.0, 0.0, 1.0];
    let white = [255, 255, 255, 255];
    let count_white = |image: &[u8]| image.chunks_exact(4).filter(|p| *p == white).count();

    let plain = render(&gpu, &mut renderer, &scene(vec![flat(cube, translation(0.0, 0.0, 0.0), red)]));
    assert_eq!(count_white(&plain), 0);

    let mut selected = flat(cube, translation(0.0, 0.0, 0.0), red);
    selected.selected = true;
    let image = render(&gpu, &mut renderer, &scene(vec![selected]));
    // The object itself is untouched and the border is around it.
    assert_eq!(centre(&image), [255, 0, 0, 255]);
    assert!(count_white(&image) > 50, "{}", count_white(&image));
    // Walking left from the centre: red, then the border, then sky.
    let row: Vec<[u8; 4]> = (0..SIZE / 2).rev().map(|x| pixel(&image, x, SIZE / 2)).collect();
    let first_other = row.iter().position(|p| *p != [255, 0, 0, 255]).unwrap();
    assert_eq!(row[first_other], white);
    assert_eq!(*row.last().unwrap(), SKY);
}

#[test]
fn sky_layers_stay_behind_the_world_and_can_add_light() {
    use mi_render::Layer;
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let near = |a: [u8; 4], b: [i32; 3]| (0..3).all(|i| (a[i] as i32 - b[i]).abs() <= 1);

    // A backdrop object nearer to the camera does not hide a world
    // object behind it, even when it is drawn first: it has no depth.
    let mut backdrop = flat(cube, translation(0.0, -20.0, 0.0), [0.0, 1.0, 0.0, 1.0]);
    backdrop.layer = Layer::Sky;
    let world = flat(cube, translation(0.0, 20.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
    let mut s = scene(vec![backdrop.clone(), world]);
    s.background = [0.0, 0.0, 0.25];
    let image = render(&gpu, &mut renderer, &s);
    assert!(near(centre(&image), [255, 0, 0]), "{:?}", centre(&image));
    // Around the world object the backdrop shows; in the corner the clear colour.
    assert!(near(pixel(&image, SIZE / 2 + 30, SIZE / 2), [0, 255, 0]));
    assert!(near(pixel(&image, 1, 1), [0, 0, 64]));

    // An additive object adds its colour times its alpha; of the cube both
    // the front and the back are drawn, as backdrops are not culled.
    let mut glow = flat(cube, translation(0.0, 0.0, 0.0), [1.0, 0.5, 0.0, 0.25]);
    glow.layer = Layer::SkyAdd;
    s.objects = vec![glow];
    let image = render(&gpu, &mut renderer, &s);
    assert!(near(centre(&image), [128, 64, 64]), "{:?}", centre(&image));
}

#[test]
fn wind_moves_what_is_set_to_sway() {
    use mi_render::{ObjectWind, Wind};
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);
    let windy = |time: f32, wind: ObjectWind| {
        let mut object = flat(cube, translation(0.0, 0.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
        object.wind = wind;
        let mut s = scene(vec![object]);
        s.wind = Wind { time, speed: 1.0, direction: [1.0, 0.0], gust_phase: 0.0 };
        s
    };
    let covered = |image: &[u8]| (0..SIZE * SIZE).filter(|i| pixel(image, i % SIZE, i / SIZE)[0] == 255).count();
    let whole = ObjectWind { whole: true, marked: true, strength: 6.0, directional_strength: 0.0 };

    // A swaying object is drawn differently as time passes.
    let early = render(&gpu, &mut renderer, &windy(1.0, whole));
    let late = render(&gpu, &mut renderer, &windy(9.0, whole));
    assert!(covered(&early) > 0 && early != late);
    // Without strength, or when only marked vertices sway (the cube has
    // none), nothing moves.
    let still = ObjectWind { strength: 0.0, ..whole };
    assert!(render(&gpu, &mut renderer, &windy(1.0, still)) == render(&gpu, &mut renderer, &windy(9.0, still)));
    let marked = ObjectWind { whole: false, ..whole };
    assert!(render(&gpu, &mut renderer, &windy(1.0, marked)) == render(&gpu, &mut renderer, &windy(9.0, marked)));
    assert!(render(&gpu, &mut renderer, &windy(1.0, marked)) == render(&gpu, &mut renderer, &windy(1.0, ObjectWind::default())));
    // Gusts push it along the wind.
    let gusty = ObjectWind { directional_strength: 20.0, ..whole };
    assert!(render(&gpu, &mut renderer, &windy(1.0, gusty)) != early);
}

#[test]
fn high_quality_gathers_samples_and_casts_sun_shadows() {
    use mi_render::HighSettings;
    let Some(gpu) = gpu() else { return };
    let (mut renderer, cube) = cube_renderer(&gpu);

    // A cube floating over a wide, thin floor, seen from above and aside,
    // with the sun overhead.
    let floor_model = Mat4::from_translation(Vec3::new(0.0, 0.0, -20.0)) * Mat4::from_scale(Vec3::new(20.0, 20.0, 0.1));
    let floor = RenderObject::new(cube, floor_model.to_cols_array());
    let floating = RenderObject::new(cube, translation(0.0, 0.0, 0.0));
    let mut s = scene(vec![floor, floating]);
    s.camera.from = Vec3::new(0.0, -150.0, 120.0);
    s.camera.to = Vec3::new(0.0, 0.0, -20.0);
    let pixel_of = |world: Vec3| {
        let clip = s.camera.view_projection(1.0) * world.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        (((ndc.x * 0.5 + 0.5) * SIZE as f32) as u32, ((0.5 - ndc.y * 0.5) * SIZE as f32) as u32)
    };
    let under = pixel_of(Vec3::new(0.0, 0.0, -19.2));
    let beside = pixel_of(Vec3::new(60.0, 0.0, -19.2));

    let target = OffscreenTarget::new(&gpu.device, &gpu.queue, &renderer, SIZE, SIZE);
    let render = |renderer: &mut Renderer, settings: &HighSettings, scene: &RenderScene| {
        let mut calls = 0;
        while renderer.render_high(&target.color, target.viewport(), scene, settings) {
            calls += 1;
            assert!(calls < 100);
        }
        (target.read_rgba().unwrap(), calls + 1)
    };

    // Samples are gathered one per call until there are enough.
    let settings = HighSettings { samples: 6, sun_buffer_size: 512, ..Default::default() };
    let (image, calls) = render(&mut renderer, &settings, &s);
    assert_eq!(calls, 6);
    assert_eq!(renderer.high_samples(SIZE, SIZE), 6);
    // Asking again changes nothing and wants no more.
    assert!(!renderer.render_high(&target.color, target.viewport(), &s, &settings));
    assert_eq!(target.read_rgba().unwrap(), image);

    // The floor under the cube is in its shadow; beside it, in the sun.
    let lit = brightness(pixel(&image, beside.0, beside.1));
    let shaded = brightness(pixel(&image, under.0, under.1));
    assert!(lit > shaded + 150, "lit {lit}, shaded {shaded}");
    assert_eq!(pixel(&image, 1, 1), SKY);

    // Without shadows both are lit alike. A change of settings starts over.
    let no_shadows = HighSettings { shadows: false, ..settings };
    let (plain, calls) = render(&mut renderer, &no_shadows, &s);
    assert_eq!(calls, 6);
    let a = brightness(pixel(&plain, beside.0, beside.1));
    let b = brightness(pixel(&plain, under.0, under.1));
    assert!((a as i32 - b as i32).abs() < 12, "{a} {b}");

    // An object that casts no shadow leaves the floor lit.
    let mut ghost = s.clone();
    ghost.objects[1].shadows = false;
    let (image, _) = render(&mut renderer, &settings, &ghost);
    let shaded = brightness(pixel(&image, under.0, under.1));
    assert!((lit as i32 - shaded as i32).abs() < 12, "{lit} {shaded}");

    // Edges are smoothed by the shifted samples: along the cube's outline
    // there are in-between colours, which one sample does not give.
    let single = HighSettings { samples: 1, ..settings };
    let (hard, _) = render(&mut renderer, &single, &s);
    let (smooth, _) = render(&mut renderer, &HighSettings { samples: 16, ..settings }, &s);
    let colours = |image: &[u8]| {
        let mut seen = std::collections::HashSet::new();
        for i in 0..SIZE * SIZE {
            seen.insert(pixel(image, i % SIZE, i / SIZE));
        }
        seen.len()
    };
    assert!(colours(&smooth) > colours(&hard) * 2, "{} {}", colours(&smooth), colours(&hard));
}
