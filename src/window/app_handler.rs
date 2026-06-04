use super::app::AuroraApp;
use std::sync::Arc;
use std::time::Instant;
use vello::wgpu::PresentMode;
use winit::event::StartCause;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

impl winit::application::ApplicationHandler for AuroraApp {
    fn new_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.request_redraw();
        }
        self.schedule_next_frame(event_loop);
    }

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let viewport = *self.input.viewport.borrow();
        let initial_width = viewport.width.max(1.0) as u32;
        let initial_height = viewport.height.max(1.0) as u32;
        let window_attr = Window::default_attributes()
            .with_title("Aurora Browser (GPU Accelerated)")
            .with_inner_size(winit::dpi::LogicalSize::new(
                viewport.width as f64,
                viewport.height as f64,
            ));

        let window = Arc::new(
            event_loop
                .create_window(window_attr)
                .expect("failed to create window"),
        );
        self.window = Some(window.clone());

        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            initial_width,
            initial_height,
            PresentMode::Fifo,
        ))
        .expect("failed to create surface");
        self.surface = Some(surface);
        self.renderers
            .resize_with(self.context.devices.len(), || None);
        window.request_redraw();
        self.schedule_next_frame(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.handle_resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x;
                self.mouse_y = position.y;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => self.handle_click(),
            WindowEvent::RedrawRequested => {
                if self.surface.is_some() {
                    if self.run_frame_tasks() {
                        self.request_redraw();
                    }
                    self.render();
                    self.schedule_next_frame(event_loop);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => self.handle_key(event_loop, logical_key),
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, dy) => dy * 30.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_by(-scroll_amount);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let runtime_dirty = self.input.runtime.as_ref()
            .map(|r| r.has_dirty_bits())
            .unwrap_or(false);
        if self.has_animation_frame_callbacks()
            || self.timer_is_due()
            || self.has_active_media()
            || self.input.needs_reflow
            || runtime_dirty
        {
            self.request_redraw();
        }
        self.schedule_next_frame(event_loop);
    }
}

impl AuroraApp {
    fn handle_resize(&mut self, width: u32, height: u32) {
        if let Some(surface) = self.surface.as_mut() {
            self.context.resize_surface(surface, width, height);
        }
        self.reflow(width, height);
        self.request_redraw();
    }

    fn scroll_by(&mut self, amount: f32) {
        let viewport = *self.input.viewport.borrow();
        let content_height = (viewport.height - super::BROWSER_CHROME_HEIGHT).max(1.0);
        let doc_height = self.input.layout.borrow().root().rect().height;
        let max_scroll = (doc_height - content_height).max(0.0);
        self.scroll_y = (self.scroll_y + amount as f64).clamp(0.0, max_scroll as f64);
        self.request_redraw();
    }

    fn handle_key(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, key: Key) {
        match key {
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_by(30.0);
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_by(-30.0);
            }
            Key::Named(NamedKey::PageDown) => {
                let viewport = *self.input.viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(page);
            }
            Key::Named(NamedKey::PageUp) => {
                let viewport = *self.input.viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(-page);
            }
            Key::Named(NamedKey::Space) => {
                let viewport = *self.input.viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(page);
            }
            _ => {}
        }
    }

    fn handle_click(&mut self) {
        let content_x = self.mouse_x as f32;
        let content_y = (self.mouse_y - super::BROWSER_CHROME_HEIGHT as f64 + self.scroll_y) as f32;

        // Navigation: use blitz-dom hit test so coordinates match what is rendered.
        if let Some(href) = self
            .input
            .blitz_doc
            .as_ref()
            .and_then(|doc| doc.hit_test_anchor(content_x, content_y))
        {
            let full_url = match &self.input.base_url {
                Some(base) => crate::fetch::resolve_relative_url(base, &href)
                    .unwrap_or(href),
                None => href,
            };
            self.input.navigate_to(&full_url);
            self.scroll_y = 0.0;
            self.request_redraw();
            return;
        }

        // JS event dispatch: legacy LayoutTree hit test for DOM node identity.
        let hit_node = {
            let layout = self.input.layout.borrow();
            layout.hit_test(content_x, content_y)
        };
        if let Some(node) = hit_node {
            if let Some(runtime) = self.input.runtime.as_mut() {
                if runtime.dispatch_event(&node, "click") {
                    self.request_redraw();
                }
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn timer_is_due(&self) -> bool {
        self.next_runtime_deadline()
            .map(|deadline| deadline <= Instant::now())
            .unwrap_or(false)
    }

    fn schedule_next_frame(&self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.has_animation_frame_callbacks() || self.has_active_media() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if let Some(deadline) = self.next_runtime_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn has_active_media(&self) -> bool {
        self.input.media.has_active_media()
    }
}

