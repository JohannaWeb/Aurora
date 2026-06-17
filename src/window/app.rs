use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{Renderer, RendererOptions, Scene, wgpu};
use winit::window::Window;

use super::BROWSER_CHROME_HEIGHT;
use super::chrome::ChromeRenderer;
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
    pub(super) chrome: ChromeRenderer,
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
            chrome: ChromeRenderer::default(),
        }
    }

    pub(super) fn reflow(&mut self, width: u32, height: u32) {
        self.input.reflow(width, height);
    }

    pub(super) fn run_frame_tasks(&mut self) -> bool {
        let now = Instant::now();
        let mut needs_reflow = self.input.needs_reflow;
        let mut runtime_dirtied_blitz = false;

        if let Some(runtime) = self.input.runtime.as_mut() {
            let runtime_needs_reflow =
                runtime.tick(now) | runtime.drain_animation_frame_callbacks(now);
            if runtime_needs_reflow {
                runtime_dirtied_blitz = true;
                needs_reflow = true;
            }
            if runtime.take_needs_reflow() {
                runtime_dirtied_blitz = true;
                needs_reflow = true;
            }
        }
        if runtime_dirtied_blitz && self.input.blitz_doc.is_none() {
            self.input.mark_blitz_snapshot_dirty();
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
        let url = self
            .input
            .base_url
            .clone()
            .unwrap_or_else(|| "aurora://local".to_string());
        self.chrome.paint(
            &mut scene,
            width,
            &url,
            &self.input.dom,
            &self.input.identity,
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
    // The clip only needs to keep content from painting up into the chrome, so
    // it matters vertically (top = content_top). Pulling the left/right edges a
    // hair outside the viewport keeps the content's x=0 column off the clip's
    // antialiased boundary, which was shaving the left edge of the page.
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &vello::kurbo::Rect::new(-2.0, content_top, width as f64 + 2.0, height as f64),
    );
    let mut content_scene = Scene::new();
    if let Some(blitz_doc) = app.input.blitz_doc.as_ref().cloned() {
        if !blitz_doc
            .borrow_mut()
            .paint_to_scene(&mut content_scene, width, content_height)
        {
            app.input.mark_blitz_snapshot_dirty();
            app.input.needs_reflow = true;
            content_scene = Scene::new();
        }
    }
    scene.append(
        &content_scene,
        Some(Affine::translate((0.0, content_top - app.scroll_y))),
    );
    scene.pop_layer();
}
