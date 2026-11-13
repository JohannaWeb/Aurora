use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{Renderer, RendererOptions, Scene, wgpu};
use winit::window::Window;

use super::BROWSER_CHROME_HEIGHT;
use super::chrome::{ChromeProps, ChromeRenderer};
use super::input::{SnapshotRebuildReason, WindowInput};
use crate::blitz_document::PaintResult;

/// One browser tab: its page state plus per-tab view state. Each tab owns its
/// own JS runtime (and thus its own V8 isolate); `V8Runtime` keeps isolates
/// un-entered at rest, so tabs can be closed (dropped) in any order.
pub(super) struct Tab {
    pub(super) input: WindowInput,
    pub(super) scroll_y: f64,
    frame_cache: LastGoodSceneState,
}

impl Tab {
    pub(super) fn new(input: WindowInput) -> Self {
        Self {
            input,
            scroll_y: 0.0,
            frame_cache: LastGoodSceneState::default(),
        }
    }
}

pub(super) struct AuroraApp {
    /// Open tabs; never empty. Only the active tab is ticked and painted —
    /// background tabs are effectively suspended until re-activated.
    pub(super) tabs: Vec<Tab>,
    pub(super) active: usize,
    /// URL of the initial page, offered as a link on new-tab pages.
    home_url: Option<String>,
    pub(super) context: RenderContext,
    pub(super) renderers: Vec<Option<Renderer>>,
    pub(super) surface: Option<RenderSurface<'static>>,
    pub(super) window: Option<Arc<Window>>,
    pub(super) mouse_x: f64,
    pub(super) mouse_y: f64,
    pub(super) modifiers: winit::keyboard::ModifiersState,
    pub(super) chrome: ChromeRenderer,
    /// `Some(buffer)` while the address bar has keyboard focus. Window-level
    /// rather than per-tab: switching tabs drops an in-progress edit, like
    /// mainstream browsers.
    pub(super) url_edit: Option<String>,
}

impl AuroraApp {
    pub(super) fn new(input: WindowInput) -> Self {
        let home_url = input.base_url.clone();
        Self {
            tabs: vec![Tab::new(input)],
            active: 0,
            home_url,
            context: RenderContext::new(),
            renderers: Vec::new(),
            surface: None,
            window: None,
            mouse_x: 0.0,
            mouse_y: 0.0,
            modifiers: winit::keyboard::ModifiersState::default(),
            chrome: ChromeRenderer::default(),
            url_edit: None,
        }
    }

    pub(super) fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub(super) fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub(super) fn input(&self) -> &WindowInput {
        &self.tabs[self.active].input
    }

    pub(super) fn input_mut(&mut self) -> &mut WindowInput {
        &mut self.tabs[self.active].input
    }

    /// Open a new tab showing the built-in new-tab page and make it active.
    /// The address bar starts focused so a URL can be typed immediately.
    pub(super) fn open_new_tab(&mut self) {
        let viewport = *self.input().viewport.borrow();
        let input = WindowInput::blank(
            self.input().identity.clone(),
            viewport,
            self.home_url.as_deref(),
        );
        self.tabs.push(Tab::new(input));
        self.activate_tab(self.tabs.len() - 1);
        self.url_edit = Some(String::new());
    }

    /// Close a tab. Returns false when the last tab was closed, i.e. the
    /// caller should exit the app. Tabs drop in click order, not creation
    /// order — the V8 isolate lifecycle explicitly supports this.
    pub(super) fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return true;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            return false;
        }
        // Closing a tab left of the active one shifts the active tab down by
        // one; closing the active tab itself falls through to its right
        // neighbour (or the new last tab).
        let next = if index < self.active {
            self.active - 1
        } else {
            self.active
        };
        self.activate_tab(next.min(self.tabs.len() - 1));
        true
    }

    pub(super) fn activate_tab(&mut self, index: usize) {
        self.url_edit = None;
        self.active = index.min(self.tabs.len() - 1);
        // The window may have been resized while this tab was inactive;
        // reflow against the real surface size, not the tab's stale viewport.
        if let Some(surface) = self.surface.as_ref() {
            let (w, h) = (surface.config.width, surface.config.height);
            self.reflow(w, h);
        }
    }

    pub(super) fn cycle_tab(&mut self, backwards: bool) {
        let len = self.tabs.len();
        if len < 2 {
            return;
        }
        let next = if backwards {
            (self.active + len - 1) % len
        } else {
            (self.active + 1) % len
        };
        self.activate_tab(next);
    }

    pub(super) fn reflow(&mut self, width: u32, height: u32) {
        self.input_mut().reflow(width, height);
    }

    pub(super) fn run_frame_tasks(&mut self) -> bool {
        let now = crate::clock::now();
        let input = self.input_mut();
        let mut needs_reflow = input.needs_reflow;
        let mut runtime_dirtied_blitz = false;

        if let Some(runtime) = input.runtime.as_mut() {
            let runtime_needs_reflow = runtime.tick(now)
                | runtime.drain_animation_frame_callbacks(now)
                | runtime.deliver_mutation_records()
                | runtime.perform_style_and_layout()
                | runtime.take_needs_reflow();
            if runtime_needs_reflow {
                runtime_dirtied_blitz = true;
                needs_reflow = true;
            }
        }
        if runtime_dirtied_blitz && input.blitz_doc.is_none() {
            input.mark_blitz_snapshot_dirty(SnapshotRebuildReason::MissingMapping);
        }

        let needs_redraw = self.input_mut().media.update();
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
        let viewport = *self.input().viewport.borrow();
        self.reflow(viewport.width as u32, viewport.height as u32);
    }

    pub(super) fn next_runtime_deadline(&self) -> Option<Instant> {
        self.input()
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.next_deadline())
    }

    pub(super) fn has_animation_frame_callbacks(&self) -> bool {
        self.input()
            .runtime
            .as_ref()
            .map(|runtime| runtime.has_animation_frame_callbacks())
            .unwrap_or(false)
    }

    pub(super) fn render(&mut self) {
        // Extract what we need from surface before any mutable borrows of self.
        let (width, height, dev_id) = {
            let Some(s) = self.surface.as_ref() else {
                return; // nothing to render before the surface is configured
            };
            (s.config.width, s.config.height, s.dev_id)
        };

        let mut scene = Scene::new();
        paint_content_layer(self, &mut scene, width, height);
        let chrome_props = self.chrome_props();
        let identity = self.input().identity.clone();
        self.chrome
            .paint(&mut scene, width, chrome_props, &identity);

        let Some(surface) = self.surface.as_ref() else {
            return;
        };
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
        device_handle.queue.present(surface_texture);
    }

    /// Chrome props for the live window: every tab's label plus the active
    /// tab's URL, telemetry, and identity.
    fn chrome_props(&self) -> ChromeProps {
        let url = |input: &WindowInput| {
            input
                .base_url
                .clone()
                .unwrap_or_else(|| "aurora://local".to_string())
        };
        let labels = self
            .tabs
            .iter()
            .map(|tab| super::chrome::tab_label(&url(&tab.input), &tab.input.dom))
            .collect();
        let active_input = self.input();
        let mut props = ChromeProps::from_tabs(
            &url(active_input),
            &active_input.dom,
            &active_input.identity,
            labels,
            self.active,
        );
        props.url_edit = self.url_edit.clone();
        props
    }

    fn handle_content_paint_failure(
        &mut self,
        paint_result: PaintResult,
        content_scene: &mut Scene,
        width: u32,
        content_height: u32,
    ) -> PaintResult {
        let tab = &mut self.tabs[self.active];
        tab.input
            .mark_blitz_snapshot_dirty(SnapshotRebuildReason::PaintFailure);
        tab.input.needs_reflow = true;
        let effective_result =
            tab.frame_cache
                .finish_failed_paint(paint_result, content_scene, width, content_height);
        if matches!(effective_result, PaintResult::PreservedLastGoodFrame) {
            log::warn!(
                "Preserving last successful Blitz content frame after paint failure: consecutive_failures={} last_successful_paint_time={:?}",
                tab.frame_cache.consecutive_paint_failures,
                tab.frame_cache.last_successful_paint_time
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
    renderers[dev_id].get_or_insert_with(|| {
        Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .expect("failed to create vello renderer")
    })
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
    if let Some(blitz_doc) = app.input().blitz_doc.as_ref().cloned() {
        let paint_result =
            blitz_doc
                .borrow_mut()
                .paint_to_scene(&mut content_scene, width, content_height);
        match paint_result {
            PaintResult::PaintedCurrentFrame => {
                app.tab_mut().frame_cache.record_successful_paint(
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
        Some(Affine::translate((0.0, content_top - app.tab().scroll_y))),
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
    fn open_new_tab_appends_and_activates() {
        let mut app = AuroraApp::new(test_input());
        assert_eq!(app.tabs.len(), 1);

        app.open_new_tab();

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1);
        // The new tab shows the built-in new-tab page, not the first tab's DOM.
        assert!(app.input().base_url.is_none());
        // ...with the address bar focused and empty, ready for typing.
        assert_eq!(app.url_edit.as_deref(), Some(""));
    }

    #[test]
    fn switching_tabs_drops_an_in_progress_url_edit() {
        let mut app = AuroraApp::new(test_input());
        app.open_new_tab();
        app.url_edit = Some("example.co".to_string());

        app.activate_tab(0);

        assert!(app.url_edit.is_none());
    }

    #[test]
    fn close_tab_left_of_active_keeps_active_page() {
        let mut app = AuroraApp::new(test_input());
        app.open_new_tab();
        app.open_new_tab();
        assert_eq!(app.active, 2);

        assert!(app.close_tab(0));

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1, "active index shifts down with the removal");
    }

    #[test]
    fn close_active_tab_falls_through_to_right_neighbour_or_last() {
        let mut app = AuroraApp::new(test_input());
        app.open_new_tab();
        app.open_new_tab();

        // Close the middle tab while it is active.
        app.activate_tab(1);
        assert!(app.close_tab(1));
        assert_eq!(app.active, 1, "right neighbour takes the slot");

        // Close the last tab while it is active.
        assert!(app.close_tab(1));
        assert_eq!(app.active, 0);
    }

    #[test]
    fn closing_the_only_tab_signals_exit() {
        let mut app = AuroraApp::new(test_input());
        assert!(!app.close_tab(0));
        assert!(app.tabs.is_empty());
    }

    #[test]
    fn cycle_tab_wraps_in_both_directions() {
        let mut app = AuroraApp::new(test_input());
        app.open_new_tab();
        app.open_new_tab();
        app.activate_tab(2);

        app.cycle_tab(false);
        assert_eq!(app.active, 0, "forward cycle wraps to the first tab");
        app.cycle_tab(true);
        assert_eq!(app.active, 2, "backward cycle wraps to the last tab");
    }

    #[test]
    fn recoverable_content_paint_failure_preserves_scene_and_schedules_recovery() {
        let mut app = AuroraApp::new(test_input());
        let scene = scene_with_rect();
        app.tab_mut()
            .frame_cache
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
        assert!(app.input().blitz_snapshot_dirty);
        assert!(app.input().needs_reflow);
        assert_eq!(
            app.input().pending_snapshot_rebuild_reason,
            Some(SnapshotRebuildReason::PaintFailure)
        );
    }
}
