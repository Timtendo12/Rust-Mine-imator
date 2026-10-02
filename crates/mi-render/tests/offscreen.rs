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
