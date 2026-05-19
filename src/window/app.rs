use crate::gpu_paint::GpuPainter;
use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{wgpu, Renderer, RendererOptions, Scene};
use winit::window::Window;

use super::chrome::paint_browser_chrome_scene;
use super::input::WindowInput;
use super::BROWSER_CHROME_HEIGHT;

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
        let mut needs_reflow = false;
        if let Some(runtime) = self.input.runtime.as_mut() {
            needs_reflow |= runtime.tick(now);
            needs_reflow |= runtime.drain_animation_frame_callbacks(now);
        }
        if needs_reflow {
            let viewport = *self.input.viewport.borrow();
            self.reflow(viewport.width as u32, viewport.height as u32);
        }
        needs_reflow
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
        let surface = self.surface.as_ref().unwrap();
        let width = surface.config.width;
        let height = surface.config.height;
        let device_handle = &self.context.devices[surface.dev_id];
        let mut scene = Scene::new();

        paint_content_layer(self, &mut scene, width, height);
        paint_scrollbar_layer(self, &mut scene, width, height);
        paint_browser_chrome_scene(
            &mut scene,
            width,
            self.input.base_url.as_deref().unwrap_or("aurora://local"),
        );

        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return, // timeout / occluded / outdated / lost — skip frame
        };
        let render_params = vello::RenderParams {
            base_color: Color::WHITE,
            antialiasing_method: vello::AaConfig::Msaa16,
            width,
            height,
        };

        let renderer =
            renderer_for_surface(&mut self.renderers, surface.dev_id, &device_handle.device);
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

fn paint_content_layer(app: &AuroraApp, scene: &mut Scene, width: u32, height: u32) {
    let content_top = BROWSER_CHROME_HEIGHT as f64;
    // Clip to the area below the chrome; use IDENTITY so the clip rect is in screen coords.
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
    );
    // Paint into a sub-scene, then append it with the scroll+chrome offset applied.
    // scene.append is the correct way to transform content in vello — push_layer's
    // transform only applies to the clip shape, not to draw calls inside the layer.
    let mut content_scene = Scene::new();
    GpuPainter::paint(
        app.input.layout.borrow().root(),
        &mut content_scene,
        &app.input.images,
        &app.input.svgs,
    );
    scene.append(
        &content_scene,
        Some(Affine::translate((0.0, content_top - app.scroll_y))),
    );
    scene.pop_layer();
}

fn paint_scrollbar_layer(app: &AuroraApp, scene: &mut Scene, width: u32, height: u32) {
    let content_top = BROWSER_CHROME_HEIGHT as f64;
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
    );
    let mut scrollbar_scene = Scene::new();
    GpuPainter::paint_scrollbars(
        app.input.layout.borrow().root(),
        &mut scrollbar_scene,
        (height as f32 - BROWSER_CHROME_HEIGHT).max(1.0),
    );
    scene.append(
        &scrollbar_scene,
        Some(Affine::translate((0.0, content_top))),
    );
    scene.pop_layer();
}
