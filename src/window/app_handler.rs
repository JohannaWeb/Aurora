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
                self.update_hover_and_cursor();
            }
            WindowEvent::CursorLeft { .. } => {
                self.clear_page_hover();
                self.set_cursor_icon(winit::window::CursorIcon::Default);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => self.handle_mouse_press(event_loop),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: winit::event::MouseButton::Left,
                ..
            } => self.handle_mouse_release(),
            WindowEvent::RedrawRequested => {
                if self.surface.is_some() {
                    if self.run_frame_tasks() {
                        self.request_redraw();
                    }
                    self.render();
                    self.schedule_next_frame(event_loop);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_keyboard(event_loop, event),
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
        // Timers and network callbacks can navigate too (location, forms).
        self.follow_pending_navigation();
        let runtime_dirty = self
            .input()
            .runtime
            .as_ref()
            .map(|r| r.has_dirty_bits())
            .unwrap_or(false);
        let blitz_delivered = self
            .input()
            .blitz_doc
            .as_ref()
            .is_some_and(|doc| doc.borrow().has_delivered_resources());
        if self.has_animation_frame_callbacks()
            || self.timer_is_due()
            || self.has_active_media()
            || self.input().needs_reflow
            || runtime_dirty
            || blitz_delivered
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

    /// Keys go to the address bar while it is focused, then to browser
    /// shortcuts, then to the page. A key the page did not cancel, and that
    /// did not go to a focused text field, falls back to the browser's own
    /// default (scrolling, Escape).
    fn handle_keyboard(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: KeyEvent) {
        let released = event.state == ElementState::Released;
        if !released {
            // A focused address bar captures typing before the page does.
            // Ctrl-chords deliberately fall through so tab management keeps working.
            if self.url_edit.is_some() && self.handle_url_edit_key(&event.logical_key) {
                self.request_redraw();
                return;
            }
            if self.handle_browser_shortcut(event_loop, &event.logical_key) {
                return;
            }
        } else if self.url_edit.is_some() {
            return;
        }
        let input = key_input(&event, self.modifiers);
        let outcome = match self.input_mut().runtime.as_mut() {
            Some(runtime) => runtime.dispatch_key(&input),
            None => crate::js_engine::KeyOutcome::default(),
        };
        self.request_redraw();
        if self.follow_pending_navigation() {
            return;
        }
        if released || outcome.default_prevented || outcome.editable_target {
            return;
        }
        self.handle_page_default_key(event_loop, event.logical_key);
    }

    /// Browser-level shortcuts. Returns true when the key was one.
    fn handle_browser_shortcut(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, key: &Key) -> bool {
        let ctrl = self.modifiers.control_key();
        match key {
            // Focus the address bar, following the usual browser binding.
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("l") => {
                self.url_edit = Some(String::new());
                self.request_redraw();
            }
            // Tab management, following the usual browser bindings.
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("t") => {
                self.open_new_tab();
                self.request_redraw();
            }
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("w") => {
                if !self.close_tab(self.active) {
                    event_loop.exit();
                    return true;
                }
                self.request_redraw();
            }
            Key::Named(NamedKey::Tab) if ctrl => {
                self.cycle_tab(self.modifiers.shift_key());
                self.request_redraw();
            }
            _ => return false,
        }
        true
    }

    /// The browser's default for a key the page left alone.
    fn handle_page_default_key(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, key: Key) {
        match key {
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

    fn current_modifiers(&self) -> u32 {
        (self.modifiers.shift_key() as u32)
            | ((self.modifiers.control_key() as u32) << 1)
            | ((self.modifiers.alt_key() as u32) << 2)
            | ((self.modifiers.super_key() as u32) << 3)
    }

    fn content_coords(&self) -> (f32, f32, f32, f32) {
        let client_x = self.mouse_x as f32;
        let client_y = (self.mouse_y - super::BROWSER_CHROME_HEIGHT as f64) as f32;
        let content_x = client_x;
        let content_y = (self.mouse_y - super::BROWSER_CHROME_HEIGHT as f64 + self.tab().scroll_y) as f32;
        (client_x, client_y, content_x, content_y)
    }

    fn sync_blitz_scroll(&self) {
        let scroll_y = self.tab().scroll_y;
        if let Some(doc) = self.input().blitz_doc.as_ref() {
            doc.borrow_mut().set_scroll_y(scroll_y);
        }
    }

    fn hit_test_content(&self, content_x: f32, content_y: f32) -> Option<crate::dom::NodePtr> {
        self.sync_blitz_scroll();
        self.input()
            .blitz_doc
            .as_ref()
            .and_then(|doc| doc.borrow().hit_test_dom_node(content_x, content_y))
            .or_else(|| self.input().layout.borrow().hit_test(content_x, content_y))
    }

    fn set_cursor_icon(&self, icon: winit::window::CursorIcon) {
        if let Some(window) = &self.window {
            window.set_cursor(icon);
        }
    }

    fn clear_page_hover(&mut self) {
        let changed = self
            .input()
            .blitz_doc
            .as_ref()
            .is_some_and(|doc| doc.borrow_mut().clear_hover());
        if changed {
            self.request_redraw();
        }
    }

    fn chrome_hit_at_cursor(&self) -> Option<super::chrome::ChromeHit> {
        let width = self
            .surface
            .as_ref()
            .map(|s| s.config.width as f64)
            .unwrap_or_else(|| self.input().viewport.borrow().width as f64);
        super::chrome::chrome_hit_test(self.mouse_x, self.mouse_y, self.tabs.len(), width)
    }

    fn update_hover_and_cursor(&mut self) {
        if self.mouse_y < super::BROWSER_CHROME_HEIGHT as f64 {
            self.clear_page_hover();
            let icon = match self.chrome_hit_at_cursor() {
                Some(super::chrome::ChromeHit::UrlBar) => winit::window::CursorIcon::Text,
                Some(_) => winit::window::CursorIcon::Pointer,
                None => winit::window::CursorIcon::Default,
            };
            self.set_cursor_icon(icon);
            return;
        }
        let (_, _, content_x, content_y) = self.content_coords();
        self.sync_blitz_scroll();
        let (hover_changed, cursor) = match self.input().blitz_doc.as_ref() {
            Some(doc) => {
                let mut doc = doc.borrow_mut();
                let changed = doc.set_hover_to(content_x, content_y);
                let cursor = doc.cursor_icon().unwrap_or(winit::window::CursorIcon::Default);
                (changed, cursor)
            }
            None => (false, winit::window::CursorIcon::Default),
        };
        if hover_changed {
            self.request_redraw();
        }
        self.set_cursor_icon(cursor);
    }

    fn handle_mouse_press(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Clicks in the chrome band go to the tab strip or address bar, not
        // the page. Any click that isn't on the bar drops bar focus.
        if self.mouse_y < super::BROWSER_CHROME_HEIGHT as f64 {
            self.pressed_page_target = None;
            let hit = self.chrome_hit_at_cursor();
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
                Some(super::chrome::ChromeHit::Reload) => {
                    if let Some(url) = self.input().base_url.clone() {
                        self.input_mut().navigate_to(&url);
                        self.tab_mut().scroll_y = 0.0;
                        self.request_redraw();
                    }
                }
                None => {}
            }
            return;
        }

        // A click into the page drops address-bar focus.
        if self.url_edit.take().is_some() {
            self.request_redraw();
        }

        let (client_x, client_y, content_x, content_y) = self.content_coords();
        self.sync_blitz_scroll();
        let active_changed = self.input().blitz_doc.as_ref().is_some_and(|doc| {
            let mut doc = doc.borrow_mut();
            let hover_changed = doc.set_hover_to(content_x, content_y);
            doc.activate_hover_node() || hover_changed
        });
        if active_changed {
            self.request_redraw();
        }

        let press_node = self.hit_test_content(content_x, content_y);
        self.pressed_page_target = press_node.as_ref().map(|n| (self.active, n.clone()));
        if let Some(node) = press_node {
            let modifiers = self.current_modifiers();
            if let Some(runtime) = self.input_mut().runtime.as_mut() {
                runtime.dispatch_mouse_down(&node, client_x, client_y, modifiers);
                self.request_redraw();
            }
            self.follow_pending_navigation();
        }
    }

    fn handle_mouse_release(&mut self) {
        let pressed = self.pressed_page_target.take();
        let active_cleared = self
            .input()
            .blitz_doc
            .as_ref()
            .is_some_and(|doc| doc.borrow_mut().unactivate_hover_node());
        if active_cleared {
            self.request_redraw();
        }

        let Some((tab_idx, press_node)) = pressed else {
            return;
        };
        if tab_idx != self.active || self.mouse_y < super::BROWSER_CHROME_HEIGHT as f64 {
            return;
        }

        let (client_x, client_y, content_x, content_y) = self.content_coords();
        let release_node = self.hit_test_content(content_x, content_y);
        let click_target = release_node
            .as_ref()
            .and_then(|release| common_dom_ancestor(&press_node, release));

        if self.input().runtime.is_none() {
            if click_target.is_some()
                && let Some(href) = self
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
            }
            return;
        }

        if let Some(release) = release_node {
            let modifiers = self.current_modifiers();
            if let Some(runtime) = self.input_mut().runtime.as_mut() {
                runtime.dispatch_mouse_up_and_click(
                    &release,
                    click_target.as_ref(),
                    client_x,
                    client_y,
                    modifiers,
                );
                self.request_redraw();
            }
            self.follow_pending_navigation();
        }
    }

    /// Performs a navigation the page requested (location, form submission),
    /// if there is one. Returns whether it navigated.
    fn follow_pending_navigation(&mut self) -> bool {
        let url = self.input_mut().runtime.as_mut().and_then(|runtime| runtime.take_pending_navigation());
        let Some(url) = url else { return false };
        self.pressed_page_target = None;
        self.input_mut().navigate_to(&url);
        self.tab_mut().scroll_y = 0.0;
        self.request_redraw();
        true
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

/// Nearest common inclusive DOM ancestor of `a` and `b` (crossing shadow hosts
/// via `SyntheticShadowTreeBackend` so slotted/shadow children resolve cleanly).
fn common_dom_ancestor(
    a: &crate::dom::NodePtr,
    b: &crate::dom::NodePtr,
) -> Option<crate::dom::NodePtr> {
    use crate::dom::ShadowTreeBackend;
    let next_parent = |node: &crate::dom::NodePtr| {
        crate::dom::parent_ptr(node)
            .or_else(|| crate::dom::SyntheticShadowTreeBackend.host_for_shadow_root(node))
    };
    let mut ancestors = Vec::new();
    let mut cur = Some(a.clone());
    while let Some(node) = cur {
        ancestors.push(node.clone());
        cur = next_parent(&node);
    }
    let mut cur_b = Some(b.clone());
    while let Some(node) = cur_b {
        if ancestors.iter().any(|cand| std::rc::Rc::ptr_eq(cand, &node)) {
            return Some(node);
        }
        cur_b = next_parent(&node);
    }
    None
}

/// The window's key event in DOM terms (`key`, `code`, legacy `keyCode`).
fn key_input(event: &KeyEvent, modifiers: winit::keyboard::ModifiersState) -> crate::js_engine::KeyInput {
    use winit::keyboard::PhysicalKey;
    let key = match &event.logical_key {
        Key::Character(c) => c.to_string(),
        Key::Named(NamedKey::Space) => " ".to_string(),
        Key::Named(named) => format!("{named:?}"),
        Key::Dead(_) => "Dead".to_string(),
        Key::Unidentified(_) => "Unidentified".to_string(),
    };
    let code = match event.physical_key {
        PhysicalKey::Code(code) => format!("{code:?}"),
        PhysicalKey::Unidentified(_) => String::new(),
    };
    crate::js_engine::KeyInput {
        key_code: legacy_key_code(&event.logical_key),
        key,
        code,
        text: event.text.as_ref().map(|t| t.to_string()),
        shift: modifiers.shift_key(),
        ctrl: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
        released: event.state == ElementState::Released,
    }
}

/// `keyCode` as browsers report it for the common keys; 0 otherwise.
fn legacy_key_code(key: &Key) -> u32 {
    match key {
        Key::Character(c) => match c.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some(ch @ ('A'..='Z' | '0'..='9')) if c.chars().count() == 1 => ch as u32,
            _ => 0,
        },
        Key::Named(named) => match named {
            NamedKey::Backspace => 8,
            NamedKey::Tab => 9,
            NamedKey::Enter => 13,
            NamedKey::Shift => 16,
            NamedKey::Control => 17,
            NamedKey::Alt => 18,
            NamedKey::Escape => 27,
            NamedKey::Space => 32,
            NamedKey::PageUp => 33,
            NamedKey::PageDown => 34,
            NamedKey::End => 35,
            NamedKey::Home => 36,
            NamedKey::ArrowLeft => 37,
            NamedKey::ArrowUp => 38,
            NamedKey::ArrowRight => 39,
            NamedKey::ArrowDown => 40,
            NamedKey::Delete => 46,
            _ => 0,
        },
        _ => 0,
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
