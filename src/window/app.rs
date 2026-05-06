use crate::gpu_paint::GpuPainter;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
use std::sync::Arc;
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
        }
    }

    pub(super) fn reflow(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.input.viewport = ViewportSize {
            width: width as f32,
            height: height as f32,
        };
        let content_viewport = ViewportSize {
            width: width as f32,
            height: ((height as f32) - BROWSER_CHROME_HEIGHT).max(1.0),
        };
        let style_tree = StyleTree::from_dom(&self.input.dom, &self.input.stylesheet);
        self.input.layout =
            LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);
        self.input.images = crate::load_images(
            self.input.layout.root(),
            self.input.base_url.as_deref(),
            &self.input.identity,
        );
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

        let surface_texture = surface
            .surface
            .get_current_texture()
            .expect("failed to get surface texture");
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
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::translate((0.0, content_top - app.scroll_y)),
        &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
    );
    GpuPainter::paint(app.input.layout.root(), scene, &app.input.images);
    scene.pop_layer();
}

fn paint_scrollbar_layer(app: &AuroraApp, scene: &mut Scene, width: u32, height: u32) {
    let content_top = BROWSER_CHROME_HEIGHT as f64;
    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode::default(),
        1.0,
        Affine::translate((0.0, content_top)),
        &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
    );
    GpuPainter::paint_scrollbars(
        app.input.layout.root(),
        scene,
        (height as f32 - BROWSER_CHROME_HEIGHT).max(1.0),
    );
    scene.pop_layer();
}
