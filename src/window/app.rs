use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{Renderer, RendererOptions, Scene, wgpu};
use winit::window::Window;

use super::BROWSER_CHROME_HEIGHT;
use super::chrome::ChromeRenderer;
use super::input::{SnapshotRebuildReason, WindowInput};
use crate::blitz_document::PaintResult;

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
    content_frame_cache: LastGoodSceneState,
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
            content_frame_cache: LastGoodSceneState::default(),
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
            self.input
                .mark_blitz_snapshot_dirty(SnapshotRebuildReason::MissingMapping);
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

    fn handle_content_paint_failure(
        &mut self,
        paint_result: PaintResult,
        content_scene: &mut Scene,
        width: u32,
        content_height: u32,
    ) -> PaintResult {
        self.input
            .mark_blitz_snapshot_dirty(SnapshotRebuildReason::PaintFailure);
        self.input.needs_reflow = true;
        let effective_result = self.content_frame_cache.finish_failed_paint(
            paint_result,
            content_scene,
            width,
            content_height,
        );
        if matches!(effective_result, PaintResult::PreservedLastGoodFrame) {
            log::warn!(
                "Preserving last successful Blitz content frame after paint failure: consecutive_failures={} last_successful_paint_time={:?}",
                self.content_frame_cache.consecutive_paint_failures,
                self.content_frame_cache.last_successful_paint_time
            );
        }
        effective_result
    }
}

#[derive(Default)]
struct LastGoodSceneState {
    last_good_scene: Option<Scene>,
    last_good_scene_size: Option<(u32, u32)>,
    last_successful_paint_time: Option<Instant>,
    consecutive_paint_failures: u32,
}

impl LastGoodSceneState {
    fn record_successful_paint(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        painted_at: Instant,
    ) {
        self.last_good_scene = Some(scene.clone());
        self.last_good_scene_size = Some((width, height));
        self.last_successful_paint_time = Some(painted_at);
        self.consecutive_paint_failures = 0;
    }

    fn finish_failed_paint(
        &mut self,
        paint_result: PaintResult,
        scene: &mut Scene,
        width: u32,
        height: u32,
    ) -> PaintResult {
        debug_assert!(matches!(
            paint_result,
            PaintResult::FailedRecoverable | PaintResult::FailedUnhealthy
        ));
        self.consecutive_paint_failures += 1;

        if matches!(paint_result, PaintResult::FailedRecoverable)
            && self.last_good_scene_size == Some((width, height))
            && let Some(last_good_scene) = self.last_good_scene.clone()
        {
            *scene = last_good_scene;
            return PaintResult::PreservedLastGoodFrame;
        }

        *scene = Scene::new();
        paint_result
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
        let paint_result =
            blitz_doc
                .borrow_mut()
                .paint_to_scene(&mut content_scene, width, content_height);
        match paint_result {
            PaintResult::PaintedCurrentFrame => {
                app.content_frame_cache.record_successful_paint(
                    &content_scene,
                    width,
                    content_height,
                    Instant::now(),
                );
            }
            PaintResult::PreservedLastGoodFrame => {}
            PaintResult::FailedRecoverable | PaintResult::FailedUnhealthy => {
                let _effective_result = app.handle_content_paint_failure(
                    paint_result,
                    &mut content_scene,
                    width,
                    content_height,
                );
            }
        }
    }
    scene.append(
        &content_scene,
        Some(Affine::translate((0.0, content_top - app.scroll_y))),
    );
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Stylesheet;
    use crate::identity::{Capability, Identity, IdentityKind};
    use crate::layout::{LayoutTree, ViewportSize};
    use crate::media::MediaCache;
    use crate::style::StyleTree;
    use std::cell::RefCell;
    use std::rc::Rc;
    use vello::kurbo::Rect;

    fn scene_with_rect() -> Scene {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        scene
    }

    fn test_identity() -> Identity {
        Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace, Capability::NetworkAccess],
        )
    }

    fn test_input() -> WindowInput {
        let dom = crate::html::Parser::new("<html><body><p id='item'>hello</p></body></html>")
            .parse_document();
        crate::dom::reparent_subtree(&dom);
        let identity = test_identity();
        let mut stylesheet = Stylesheet::from_dom(&dom, None, &identity);
        stylesheet.merge(Stylesheet::user_agent_stylesheet());
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let viewport = ViewportSize {
            width: 800.0,
            height: 600.0,
        };
        let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, viewport);

        WindowInput {
            dom,
            stylesheet: Rc::new(RefCell::new(stylesheet)),
            base_url: None,
            identity,
            viewport: Rc::new(RefCell::new(viewport)),
            layout: Rc::new(RefCell::new(layout)),
            images: crate::ImageCache::default(),
            svgs: crate::SvgCache::default(),
            media: MediaCache::default(),
            runtime: None,
            blitz_doc: None,
            needs_reflow: false,
            blitz_snapshot_dirty: false,
            pending_snapshot_rebuild_reason: None,
            pending_snapshot_rebuild_source: None,
            snapshot_rebuild_count: 0,
            consecutive_snapshot_rebuilds: 0,
            last_snapshot_rebuild_reason: None,
            last_snapshot_rebuild_source: None,
            last_snapshot_rebuild_op_id: None,
            #[cfg(debug_assertions)]
            snapshot_rebuild_events: std::collections::VecDeque::new(),
        }
    }

    #[test]
    fn last_good_scene_records_successful_paint() {
        let mut state = LastGoodSceneState::default();
        let scene = scene_with_rect();
        let painted_at = Instant::now();

        state.record_successful_paint(&scene, 800, 540, painted_at);

        assert!(state.last_good_scene.is_some());
        assert_eq!(state.last_good_scene_size, Some((800, 540)));
        assert_eq!(state.last_successful_paint_time, Some(painted_at));
        assert_eq!(state.consecutive_paint_failures, 0);
    }

    #[test]
    fn recoverable_failure_preserves_matching_last_good_scene() {
        let mut state = LastGoodSceneState::default();
        let scene = scene_with_rect();
        state.record_successful_paint(&scene, 800, 540, Instant::now());
        let mut failed_scene = Scene::new();

        let result =
            state.finish_failed_paint(PaintResult::FailedRecoverable, &mut failed_scene, 800, 540);

        assert_eq!(result, PaintResult::PreservedLastGoodFrame);
        assert_eq!(state.consecutive_paint_failures, 1);
        assert_eq!(
            failed_scene.encoding().n_paths,
            scene.encoding().n_paths,
            "preserved scene should replace the failed frame"
        );
    }

    #[test]
    fn recoverable_failure_without_matching_size_clears_failed_scene() {
        let mut state = LastGoodSceneState::default();
        state.record_successful_paint(&scene_with_rect(), 800, 540, Instant::now());
        let mut failed_scene = scene_with_rect();

        let result =
            state.finish_failed_paint(PaintResult::FailedRecoverable, &mut failed_scene, 1024, 700);

        assert_eq!(result, PaintResult::FailedRecoverable);
        assert_eq!(state.consecutive_paint_failures, 1);
        assert_eq!(failed_scene.encoding().n_paths, 0);
    }

    #[test]
    fn unhealthy_failure_does_not_preserve_last_good_scene() {
        let mut state = LastGoodSceneState::default();
        state.record_successful_paint(&scene_with_rect(), 800, 540, Instant::now());
        let mut failed_scene = scene_with_rect();

        let result =
            state.finish_failed_paint(PaintResult::FailedUnhealthy, &mut failed_scene, 800, 540);

        assert_eq!(result, PaintResult::FailedUnhealthy);
        assert_eq!(state.consecutive_paint_failures, 1);
        assert_eq!(failed_scene.encoding().n_paths, 0);
    }

    #[test]
    fn recoverable_content_paint_failure_preserves_scene_and_schedules_recovery() {
        let mut app = AuroraApp::new(test_input());
        let scene = scene_with_rect();
        app.content_frame_cache
            .record_successful_paint(&scene, 800, 540, Instant::now());
        let mut failed_scene = Scene::new();

        let result = app.handle_content_paint_failure(
            PaintResult::FailedRecoverable,
            &mut failed_scene,
            800,
            540,
        );

        assert_eq!(result, PaintResult::PreservedLastGoodFrame);
        assert_eq!(failed_scene.encoding().n_paths, scene.encoding().n_paths);
        assert!(app.input.blitz_snapshot_dirty);
        assert!(app.input.needs_reflow);
        assert_eq!(
            app.input.pending_snapshot_rebuild_reason,
            Some(SnapshotRebuildReason::PaintFailure)
        );
    }
}
