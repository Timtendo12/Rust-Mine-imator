//! The 3D viewport: a wgpu surface on the application window, underneath
//! the webview. The webview is transparent where the frontend places its
//! viewport element, so the scene shows through there.
//!
//! Rendering happens on its own thread, which redraws when asked to.

use crate::scene_builder::{build_scene, SceneInputs, SceneResources, ViewCamera, ViewMode};
use crate::state::AppState;
use mi_assets::Rgba;
use mi_mesh::MeshData;
use mi_render::{wgpu, GpuError, MeshId, Renderer, TextureFilter, TextureId, Viewport, WorkCamera};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use tauri::{Manager, WebviewWindow};

/// What a viewport shows; kept in the application state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewState {
    /// Frame the scene is shown at.
    pub marker: f64,
    pub work_camera: WorkCamera,
    /// Look through the active camera timeline instead of the work camera.
    pub use_timeline_camera: bool,
    pub mode: ViewMode,
    /// Where on the window the viewport is, in physical pixels.
    pub rect: Option<Viewport>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            marker: 0.0,
            work_camera: WorkCamera::default(),
            use_timeline_camera: false,
            mode: ViewMode::Shaded,
            rect: None,
        }
    }
}

enum Message {
    Redraw,
    Resize(u32, u32),
    /// What is under a pixel of the viewport, leaving out some timelines.
    Pick { x: u32, y: u32, exclude: Vec<usize>, reply: Sender<Option<usize>> },
    /// The scene as an image of the given size, as RGBA rows.
    Image { width: u32, height: u32, timeline_camera: bool, reply: Sender<Option<Vec<u8>>> },
}

/// Handle for asking the render thread to do something. Cheap to clone.
#[derive(Clone)]
pub struct ViewportHandle {
    sender: Sender<Message>,
}

impl ViewportHandle {
    /// Asks for the scene to be drawn again. Requests made while a frame is
    /// being drawn are merged into one.
    pub fn redraw(&self) {
        // The render thread only goes away when the application exits.
        let _ = self.sender.send(Message::Redraw);
    }

    pub fn resize(&self, width: u32, height: u32) {
        let _ = self.sender.send(Message::Resize(width, height));
    }

    /// Renders the current frame into an image: RGBA, top row first.
    /// `timeline_camera` looks through the active camera timeline if there
    /// is one, otherwise the work camera is used. The selection outline is
    /// left out.
    pub fn render_image(&self, width: u32, height: u32, timeline_camera: bool) -> Option<Vec<u8>> {
        let (reply, answer) = mpsc::channel();
        self.sender.send(Message::Image { width, height, timeline_camera, reply }).ok()?;
        answer.recv_timeout(std::time::Duration::from_secs(60)).ok().flatten()
    }

    /// The timeline (by index) drawn at pixel (`x`, `y`) of the viewport,
    /// ignoring those in `exclude`. Waits for the render thread.
    pub fn pick(&self, x: u32, y: u32, exclude: Vec<usize>) -> Option<usize> {
        let (reply, answer) = mpsc::channel();
        self.sender.send(Message::Pick { x, y, exclude, reply }).ok()?;
        answer.recv_timeout(std::time::Duration::from_secs(5)).ok().flatten()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ViewportError {
    #[error("could not create a drawing surface on the window: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error(transparent)]
    Gpu(#[from] GpuError),
    #[error("the window surface supports no usable format")]
    NoFormat,
    #[error("could not read the window size: {0}")]
    Window(#[from] tauri::Error),
}

/// Meshes and textures uploaded for the scene, by key.
#[derive(Default)]
struct Cache {
    meshes: HashMap<String, MeshId>,
    groups: HashMap<String, Vec<(String, MeshId)>>,
    /// `None` remembers textures that could not be loaded.
    textures: HashMap<String, Option<TextureId>>,
    /// Meshes of bent body parts, which change during animation.
    bent: Vec<String>,
}

/// Bent meshes kept before the oldest are dropped.
const MAX_BENT_MESHES: usize = 4096;

struct Resources<'a> {
    renderer: &'a mut Renderer,
    cache: &'a mut Cache,
}

impl SceneResources for Resources<'_> {
    fn mesh(&mut self, key: String, build: &dyn Fn() -> MeshData) -> MeshId {
        if let Some(id) = self.cache.meshes.get(&key) {
            return *id;
        }
        let id = self.renderer.add_mesh(&build());
        // Body part meshes are keyed by their bend angles; keep the number
        // of different ones bounded.
        if key.starts_with("model:") {
            self.cache.bent.push(key.clone());
            if self.cache.bent.len() > MAX_BENT_MESHES {
                for old in self.cache.bent.drain(..MAX_BENT_MESHES / 2) {
                    if let Some(old_id) = self.cache.meshes.remove(&old) {
                        self.renderer.remove_mesh(old_id);
                    }
                }
            }
        }
        self.cache.meshes.insert(key, id);
        id
    }

    fn meshes(&mut self, key: String, build: &dyn Fn() -> Vec<(String, MeshData)>) -> Vec<(String, MeshId)> {
        if let Some(group) = self.cache.groups.get(&key) {
            return group.clone();
        }
        let group: Vec<(String, MeshId)> =
            build().into_iter().map(|(name, mesh)| (name, self.renderer.add_mesh(&mesh))).collect();
        self.cache.groups.insert(key, group.clone());
        group
    }

    fn texture(&mut self, key: String, load: &dyn Fn() -> Option<Rgba>) -> Option<TextureId> {
        if let Some(id) = self.cache.textures.get(&key) {
            return *id;
        }
        let id = load().map(|image| self.renderer.add_texture(&image.pixels, image.width, image.height, TextureFilter::Nearest));
        self.cache.textures.insert(key, id);
        id
    }
}

/// Resources for building a scene outside the viewport, in tests.
#[cfg(test)]
pub(crate) fn render_resources<'a>(renderer: &'a mut Renderer, cache: &'a mut RenderCache) -> impl SceneResources + 'a {
    Resources { renderer, cache: &mut cache.0 }
}

/// The viewport's cache of uploaded meshes and textures.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct RenderCache(Cache);

struct RenderThread {
    window: WebviewWindow,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    depth: wgpu::TextureView,
    cache: Cache,
}

impl RenderThread {
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width == self.config.width && height == self.config.height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = self.renderer.create_depth_view(width, height);
    }

    fn draw(&mut self) {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(error) => return eprintln!("viewport: no frame after reconfiguring: {error}"),
                }
            }
            // A timeout or a minimised window: try again on the next redraw.
            Err(error) => return eprintln!("viewport: {error}"),
        };
        let color = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (scene, rect) = self.scene(None);

        match scene {
            Some(scene) => self.renderer.render(&color, &self.depth, rect, &scene, true),
            None => self.clear(&color),
        }
        frame.present();
    }

    /// Fills the target with the interface background while no project is
    /// open.
    fn clear(&self, color: &wgpu::TextureView) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("clear") });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.118, g: 0.122, b: 0.133, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        self.renderer.submit(encoder);
    }

    /// The scene as the view shows it now, and where on the window. For an
    /// exported image, `export_camera` says whether to look through the
    /// timeline camera, and the selection is not outlined.
    fn scene(&mut self, export_camera: Option<bool>) -> (Option<mi_render::RenderScene>, Viewport) {
        let state = self.window.state::<AppState>();
        let view = state.view();
        let full = Viewport { x: 0, y: 0, width: self.config.width, height: self.config.height };
        let rect = view.rect.map(|r| clamp_rect(r, full)).unwrap_or(full);
        let guard = state.project();
        let scene = guard.as_ref().map(|project| {
            let camera = if export_camera.unwrap_or(view.use_timeline_camera) {
                ViewCamera::Active(view.work_camera)
            } else {
                ViewCamera::Work(view.work_camera)
            };
            let bindings = state.bindings();
            let scenery = state.scenery();
            let selected = if export_camera.is_some() { Vec::new() } else { state.selection() };
            let inputs =
                SceneInputs {
                    pack: state.pack(),
                    bindings: bindings.as_ref(),
                    scenery: scenery.as_ref(),
                    selected: &selected,
                    font: state.font(),
                    backdrop: true,
                    particles: Some(state.particles()),
                };
            let mut resources = Resources { renderer: &mut self.renderer, cache: &mut self.cache };
            build_scene(project, inputs, view.marker, camera, view.mode, &mut resources)
        });
        (scene, rect)
    }

    fn image(&mut self, width: u32, height: u32, timeline_camera: bool) -> Option<Vec<u8>> {
        let (scene, _) = self.scene(Some(timeline_camera));
        let scene = scene?;
        let target = self.renderer.offscreen(width, height);
        self.renderer.render(&target.color, &target.depth, target.viewport(), &scene, true);
        match target.read_rgba() {
            Ok(pixels) => Some(pixels),
            Err(error) => {
                eprintln!("viewport: rendering an image failed: {error}");
                None
            }
        }
    }

    fn pick(&mut self, x: u32, y: u32, exclude: &[usize]) -> Option<usize> {
        let (scene, rect) = self.scene(None);
        let mut scene = scene?;
        for object in &mut scene.objects {
            if object.pick != 0 && exclude.contains(&(object.pick as usize - 1)) {
                object.pick = 0;
            }
        }
        match self.renderer.pick(&scene, rect.width, rect.height, x, y) {
            Ok(id) => id.map(|id| id as usize - 1),
            Err(error) => {
                eprintln!("viewport: picking failed: {error}");
                None
            }
        }
    }

    fn run(mut self, receiver: Receiver<Message>) {
        while let Ok(first) = receiver.recv() {
            // Handle everything that queued up, then draw once.
            let mut redraw = false;
            for message in std::iter::once(first).chain(receiver.try_iter()) {
                match message {
                    Message::Redraw => redraw = true,
                    Message::Resize(width, height) => {
                        self.resize(width, height);
                        redraw = true;
                    }
                    Message::Pick { x, y, exclude, reply } => {
                        let _ = reply.send(self.pick(x, y, &exclude));
                    }
                    Message::Image { width, height, timeline_camera, reply } => {
                        let _ = reply.send(self.image(width, height, timeline_camera));
                    }
                }
            }
            if redraw {
                self.draw();
            }
        }
    }
}

/// Limits a rectangle to the target.
fn clamp_rect(rect: Viewport, target: Viewport) -> Viewport {
    let x = rect.x.min(target.width);
    let y = rect.y.min(target.height);
    Viewport { x, y, width: rect.width.min(target.width - x), height: rect.height.min(target.height - y) }
}

/// Creates the surface on `window` and starts the render thread.
pub fn start(window: WebviewWindow) -> Result<ViewportHandle, ViewportError> {
    let size = window.inner_size()?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let surface = instance.create_surface(Arc::new(window.clone()))?;
    let (adapter, device, queue) = pollster::block_on(mi_render::request_device(&instance, Some(&surface)))?;

    let capabilities = surface.get_capabilities(&adapter);
    // The renderer does its own gamma, so the surface must not convert.
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or(capabilities.formats.first().copied())
        .ok_or(ViewportError::NoFormat)?;
    let alpha_mode = capabilities.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: Vec::new(),
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    let renderer = Renderer::new(&device, &queue, format);
    let depth = renderer.create_depth_view(config.width, config.height);
    let thread = RenderThread { window, surface, device, config, renderer, depth, cache: Cache::default() };

    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("viewport".to_owned())
        .spawn(move || thread.run(receiver))
        .expect("the operating system can start a thread");

    let handle = ViewportHandle { sender };
    handle.redraw();
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_are_kept_inside_the_target() {
        let target = Viewport { x: 0, y: 0, width: 800, height: 600 };
        let inside = Viewport { x: 100, y: 50, width: 300, height: 200 };
        assert_eq!(clamp_rect(inside, target), inside);
        let overhanging = Viewport { x: 700, y: 500, width: 300, height: 300 };
        assert_eq!(clamp_rect(overhanging, target), Viewport { x: 700, y: 500, width: 100, height: 100 });
        let outside = Viewport { x: 900, y: 900, width: 10, height: 10 };
        assert_eq!(clamp_rect(outside, target), Viewport { x: 800, y: 600, width: 0, height: 0 });
    }
}
