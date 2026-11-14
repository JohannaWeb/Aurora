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
        let viewport = *self.input().viewport.borrow();
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x;
                self.mouse_y = position.y;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => self.handle_click(event_loop),
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
        let runtime_dirty = self
            .input()
            .runtime
            .as_ref()
            .map(|r| r.has_dirty_bits())
            .unwrap_or(false);
        if self.has_animation_frame_callbacks()
            || self.timer_is_due()
            || self.has_active_media()
            || self.input().needs_reflow
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
        let viewport = *self.input().viewport.borrow();
        let content_height = (viewport.height - super::BROWSER_CHROME_HEIGHT).max(1.0);
        let doc_height = self
            .input()
            .blitz_doc
            .as_ref()
            .map(|doc| doc.borrow().document_height())
            .unwrap_or_else(|| self.input().layout.borrow().root().rect().height);
        let max_scroll = (doc_height - content_height).max(0.0);
        let tab = self.tab_mut();
        tab.scroll_y = (tab.scroll_y + amount as f64).clamp(0.0, max_scroll as f64);
        self.request_redraw();
    }

    fn handle_key(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, key: Key) {
        let ctrl = self.modifiers.control_key();
        // A focused address bar captures typing before the page shortcuts do.
        // Ctrl-chords deliberately fall through so tab management keeps working.
        if self.url_edit.is_some() && self.handle_url_edit_key(&key) {
            self.request_redraw();
            return;
        }
        match key {
            // Focus the address bar, following the usual browser binding.
            Key::Character(ref c) if ctrl && c.eq_ignore_ascii_case("l") => {
                self.url_edit = Some(String::new());
                self.request_redraw();
            }
            // Tab management, following the usual browser bindings.
            Key::Character(ref c) if ctrl && c.eq_ignore_ascii_case("t") => {
                self.open_new_tab();
                self.request_redraw();
            }
            Key::Character(ref c) if ctrl && c.eq_ignore_ascii_case("w") => {
                if !self.close_tab(self.active) {
                    event_loop.exit();
                    return;
                }
                self.request_redraw();
            }
            Key::Named(NamedKey::Tab) if ctrl => {
                self.cycle_tab(self.modifiers.shift_key());
                self.request_redraw();
            }
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_by(30.0);
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_by(-30.0);
            }
            Key::Named(NamedKey::PageDown) => {
                let viewport = *self.input().viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(page);
            }
            Key::Named(NamedKey::PageUp) => {
                let viewport = *self.input().viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(-page);
            }
            Key::Named(NamedKey::Space) => {
                let viewport = *self.input().viewport.borrow();
                let page = (viewport.height - super::BROWSER_CHROME_HEIGHT - 40.0).max(20.0);
                self.scroll_by(page);
            }
            _ => {}
        }
    }

    /// Keys while the address bar is focused. Returns true when the key was
    /// consumed by the editor.
    fn handle_url_edit_key(&mut self, key: &Key) -> bool {
        if self.modifiers.control_key() {
            return false;
        }
        match key {
            Key::Character(c) => {
                if let Some(buffer) = self.url_edit.as_mut() {
                    buffer.push_str(c);
                }
                true
            }
            Key::Named(NamedKey::Space) => {
                if let Some(buffer) = self.url_edit.as_mut() {
                    buffer.push(' ');
                }
                true
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(buffer) = self.url_edit.as_mut() {
                    buffer.pop();
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.url_edit = None;
                true
            }
            Key::Named(NamedKey::Enter) => {
                let typed = self.url_edit.take().unwrap_or_default();
                if let Some(url) = normalize_typed_url(&typed) {
                    self.input_mut().navigate_to(&url);
                    self.tab_mut().scroll_y = 0.0;
                }
                true
            }
            // No caret movement yet; swallow these so they don't scroll the
            // page out from under the user mid-edit.
            Key::Named(
                NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::PageUp
                | NamedKey::PageDown,
            ) => true,
            _ => false,
        }
    }

    fn handle_click(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Clicks in the chrome band go to the tab strip or address bar, not
        // the page. Any click that isn't on the bar drops bar focus.
        if self.mouse_y < super::BROWSER_CHROME_HEIGHT as f64 {
            let width = self
                .surface
                .as_ref()
                .map(|s| s.config.width as f64)
                .unwrap_or_else(|| self.input().viewport.borrow().width as f64);
            let hit =
                super::chrome::chrome_hit_test(self.mouse_x, self.mouse_y, self.tabs.len(), width);
            if !matches!(hit, Some(super::chrome::ChromeHit::UrlBar))
                && self.url_edit.take().is_some()
            {
                self.request_redraw();
            }
            match hit {
                Some(super::chrome::ChromeHit::UrlBar) => {
                    if self.url_edit.is_none() {
                        self.url_edit = Some(String::new());
                    }
                    self.request_redraw();
                }
                Some(super::chrome::ChromeHit::ActivateTab(i)) => {
                    self.activate_tab(i);
                    self.request_redraw();
                }
                Some(super::chrome::ChromeHit::CloseTab(i)) => {
                    if !self.close_tab(i) {
                        event_loop.exit();
                        return;
                    }
                    self.request_redraw();
                }
                Some(super::chrome::ChromeHit::NewTab) => {
                    self.open_new_tab();
                    self.request_redraw();
                }
                None => {}
            }
            return;
        }

        // A click into the page drops address-bar focus.
        if self.url_edit.take().is_some() {
            self.request_redraw();
        }

        let content_x = self.mouse_x as f32;
        let content_y =
            (self.mouse_y - super::BROWSER_CHROME_HEIGHT as f64 + self.tab().scroll_y) as f32;

        // Navigation: use blitz-dom hit test so coordinates match what is rendered.
        if let Some(href) = self
            .input()
            .blitz_doc
            .as_ref()
            .and_then(|doc| doc.borrow().hit_test_anchor(content_x, content_y))
        {
            let full_url = match &self.input().base_url {
                Some(base) => crate::fetch::resolve_relative_url(base, &href).unwrap_or(href),
                None => href,
            };
            self.input_mut().navigate_to(&full_url);
            self.tab_mut().scroll_y = 0.0;
            self.request_redraw();
            return;
        }

        // JS event dispatch follows the Blitz hit test first so events target
        // the node that was actually rendered. The legacy LayoutTree remains a
        // compatibility fallback for documents without a live Blitz renderer.
        let hit_node = self
            .input()
            .blitz_doc
            .as_ref()
            .and_then(|doc| doc.borrow().hit_test_dom_node(content_x, content_y))
            .or_else(|| {
                let layout = self.input().layout.borrow();
                layout.hit_test(content_x, content_y)
            });
        if let Some(node) = hit_node {
            if let Some(runtime) = self.input_mut().runtime.as_mut() {
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
            .map(|deadline| deadline <= crate::clock::now())
            .unwrap_or(false)
    }

    fn schedule_next_frame(&self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.has_animation_frame_callbacks() || self.has_active_media() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if let Some(deadline) = self.next_runtime_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(crate::clock::to_real(deadline)));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn has_active_media(&self) -> bool {
        self.input().media.has_active_media()
    }
}

/// Turn what was typed in the address bar into a navigable URL: schemed input
/// passes through, anything else is assumed to be a host and gets `https://`.
/// Returns None when there is nothing to navigate to.
fn normalize_typed_url(typed: &str) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    if typed.contains("://") {
        return Some(typed.to_string());
    }
    Some(format!("https://{typed}"))
}

#[cfg(test)]
mod tests {
    use super::normalize_typed_url;

    #[test]
    fn normalize_typed_url_handles_schemes_hosts_and_junk() {
        assert_eq!(
            normalize_typed_url("https://example.com/a"),
            Some("https://example.com/a".to_string())
        );
        assert_eq!(
            normalize_typed_url("http://localhost:8000"),
            Some("http://localhost:8000".to_string())
        );
        assert_eq!(
            normalize_typed_url("example.com/path?q=1"),
            Some("https://example.com/path?q=1".to_string())
        );
        assert_eq!(
            normalize_typed_url("  example.com  "),
            Some("https://example.com".to_string())
        );
        assert_eq!(normalize_typed_url(""), None);
        assert_eq!(normalize_typed_url("   "), None);
    }
}
