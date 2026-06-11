use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{Renderer, RendererOptions, Scene, wgpu};
use winit::window::Window;

use super::BROWSER_CHROME_HEIGHT;
use super::chrome::paint_browser_chrome_scene;
use super::input::WindowInput;

pub(super) struct AuroraApp {
    pub(super) input: WindowInput,
    pub(super) context: RenderContext,
    pub(super) renderers: Vec<Option<Renderer>>,
    pub(super) surface: Option<RenderSurface<'static>>,
    pub(super) window: Option<Arc<Window>>,
    pub(super) scroll_y: f64,
    pub(super) mouse_x: f64,
    pub(super) mouse_y: f64,
}

impl AuroraApp {
    pub(super) fn new(input: WindowInput) -> Self {
        Self {
            input,
            context: RenderContext::new(),
            renderers: Vec::new(),
            surface: None,
            window: None,
            scroll_y: 0.0,
            mouse_x: 0.0,
            mouse_y: 0.0,
        }
    }

    pub(super) fn reflow(&mut self, width: u32, height: u32) {
        self.input.reflow(width, height);
    }

    pub(super) fn run_frame_tasks(&mut self) -> bool {
        let now = Instant::now();
        let mut needs_reflow = self.input.needs_reflow;

        if let Some(runtime) = self.input.runtime.as_mut() {
            needs_reflow |= runtime.tick(now);
            needs_reflow |= runtime.drain_animation_frame_callbacks(now);
            needs_reflow |= runtime.take_needs_reflow();
        }

        let needs_redraw = self.input.media.update();
        if needs_reflow {
            self.perform_sync_reflow();
        }
        needs_reflow || needs_redraw
    }

    /// Forces a synchronous reflow of both supported workflows.
    ///
    /// This is intentionally dual-path: the live renderer paints through Blitz DOM
    /// and Blitz Paint, while the legacy LayoutTree remains the source for tests,
    /// screenshots, JS layout accessors, and current hit testing.
    pub(super) fn perform_sync_reflow(&mut self) {
        let viewport = *self.input.viewport.borrow();
        self.reflow(viewport.width as u32, viewport.height as u32);
    }

    pub(super) fn next_runtime_deadline(&self) -> Option<Instant> {
        self.input
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.next_deadline())
    }

    pub(super) fn has_animation_frame_callbacks(&self) -> bool {
        self.input
            .runtime
            .as_ref()
            .map(|runtime| runtime.has_animation_frame_callbacks())
            .unwrap_or(false)
    }

    pub(super) fn render(&mut self) {
        // Extract what we need from surface before any mutable borrows of self.
        let (width, height, dev_id) = {
            let s = self.surface.as_ref().unwrap();
            (s.config.width, s.config.height, s.dev_id)
        };

        let mut scene = Scene::new();
        paint_content_layer(self, &mut scene, width, height);
        paint_browser_chrome_scene(
            &mut scene,
            width,
            self.input.base_url.as_deref().unwrap_or("aurora://local"),
        );

        let surface = self.surface.as_ref().unwrap();
        let device_handle = &self.context.devices[dev_id];
        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return, // timeout / occluded / outdated / lost — skip frame
        };
        let render_params = vello::RenderParams {
            base_color: Color::WHITE,
            antialiasing_method: vello::AaConfig::Msaa16,
            width,
            height,
        };

        let renderer = renderer_for_surface(&mut self.renderers, dev_id, &device_handle.device);
        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &scene,
                &surface.target_view,
                &render_params,
            )
            .expect("failed to render to texture");

        let mut encoder = device_handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &surface.target_view,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device_handle
            .queue
            .submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }
}

fn renderer_for_surface<'a>(
    renderers: &'a mut [Option<Renderer>],
    dev_id: usize,
    device: &wgpu::Device,
) -> &'a mut Renderer {
    if renderers[dev_id].is_none() {
        renderers[dev_id] = Some(
            Renderer::new(
                device,
                RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::all(),
                    num_init_threads: None,
                    pipeline_cache: None,
                },
            )
            .expect("failed to create vello renderer"),
        );
    }
    renderers[dev_id].as_mut().unwrap()
}

fn paint_content_layer(app: &mut AuroraApp, scene: &mut Scene, width: u32, height: u32) {
    let content_top = BROWSER_CHROME_HEIGHT as f64;
    let content_height = (height as f32 - BROWSER_CHROME_HEIGHT).max(1.0) as u32;
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
    );
    let mut content_scene = Scene::new();
    if let Some(blitz_doc) = &mut app.input.blitz_doc {
        if !blitz_doc.paint_to_scene(&mut content_scene, width, content_height) {
            app.input.blitz_doc = None;
            content_scene = Scene::new();
        }
    }
    scene.append(
        &content_scene,
        Some(Affine::translate((0.0, content_top - app.scroll_y))),
    );
    scene.pop_layer();
}
