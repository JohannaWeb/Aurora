use super::app::AuroraApp;
use std::sync::Arc;
use vello::wgpu::PresentMode;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

impl winit::application::ApplicationHandler for AuroraApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let initial_width = self.input.viewport.width.max(1.0) as u32;
        let initial_height = self.input.viewport.height.max(1.0) as u32;
        let window_attr = Window::default_attributes()
            .with_title("Aurora Browser (GPU Accelerated)")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.input.viewport.width as f64,
                self.input.viewport.height as f64,
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
            WindowEvent::RedrawRequested => {
                if self.surface.is_some() {
                    self.render();
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
            _ => {}
        }
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

    fn handle_key(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, key: Key) {
        match key {
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_y += 20.0;
                self.request_redraw();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_y = (self.scroll_y - 20.0).max(0.0);
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
