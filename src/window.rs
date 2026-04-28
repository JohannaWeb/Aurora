// Import layout tree for rendering
// RUST FUNDAMENTAL: This module sits at the boundary between computed layout data and actual OS/window output.
use crate::layout::LayoutTree;
// Import GPU painter for Vello rendering
use crate::gpu_paint::GpuPainter;
// Import image cache for decoded image data
use crate::ImageCache;
// Import Arc for thread-safe sharing
// RUST FUNDAMENTAL: GUI/rendering stacks often need shared ownership across async or callback-driven code,
// which is why `Arc<T>` shows up frequently around GPU and window resources.
use std::sync::Arc;
// Import Vello graphics primitives
use vello::{
    // Import Affine for transformation matrices
    kurbo::Affine,
    // Import color and fill types
    peniko::{Color, Fill},
    // Import render context and surface for GPU rendering
    util::{RenderContext, RenderSurface},
    // Import WebGPU backend
    wgpu,
    // Import Vello renderer and scene
    Renderer,
    RendererOptions,
    Scene,
};
// Import Winit window event handling
// RUST FUNDAMENTAL: GUI frameworks often split related types into nested modules, so one `use` block can
// bring several enums and structs into scope at once.
use winit::{
    // Import window event types
    event::{ElementState, KeyEvent, WindowEvent},
    // Import event loop
    event_loop::EventLoop,
    // Import keyboard key types
    keyboard::{Key, NamedKey},
    // Import Window type
    window::Window,
};

// Open interactive window for rendering layout
pub fn open(layout: &LayoutTree, images: &ImageCache) -> Result<(), String> {
    // Check environment variable for screenshot output path
    // RUST FUNDAMENTAL: Environment lookups return `Result<String, VarError>` because the variable may not exist.
    let screenshot_path = std::env::var("AURORA_SCREENSHOT");
    // If screenshot path provided, render to file instead of window
    if let Ok(path) = screenshot_path {
        // Render layout to PNG file
        render_to_file(layout, &path);
        // Return success
        return Ok(());
    }

    // Create new event loop for window events
    // RUST FUNDAMENTAL: `.map_err(...)` is useful when a lower-level library has its own error type
    // but this function wants to expose a simpler `String` error.
    let event_loop =
        EventLoop::new().map_err(|error| format!("failed to create event loop: {error}"))?;
    // Create Aurora application state with layout and image cache
    let mut app = AuroraApp::new(layout, images);

    // Run event loop with application
    event_loop
        // Run the application
        .run_app(&mut app)
        // Map errors to string format
        .map_err(|error| format!("failed to run event loop: {error}"))
}

fn render_to_file(layout: &LayoutTree, path: &str) {
    use image::{ImageBuffer, Rgba};
    // RUST FUNDAMENTAL: A nested `use` is scoped to this function, which keeps file-level imports smaller
    // when a crate is only needed by one helper.

    eprintln!("Rendering to PNG: {}", path);
    // RUST FUNDAMENTAL: `eprintln!` writes to stderr, which is useful for status messages that should not be
    // mixed into normal stdout output.

    // RUST FUNDAMENTAL: Unsuffixed integer literals are inferred by context; here the explicit `u32` suffix fixes the type directly.
    let width = 1200u32;
    let height = 1024u32;

    // Create a white background
    let mut img = ImageBuffer::new(width, height);

    // Fill with white
    // RUST FUNDAMENTAL: Iterating over `pixels_mut()` yields mutable references to each pixel in the image buffer.
    for pixel in img.pixels_mut() {
        *pixel = Rgba([255, 255, 255, 255]);
    }

    // Render the actual layout tree with text
    render_layout_with_text(layout, &mut img, 0, 0);

    // Save to file
    if let Err(e) = img.save(path) {
        eprintln!("Failed to save screenshot: {}", e);
    } else {
        eprintln!("Screenshot saved to {}", path);
    }
}

fn render_layout_with_text(
    layout: &LayoutTree,
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    offset_x: i32,
    offset_y: i32,
) {
    let root = layout.root();
    // RUST FUNDAMENTAL: A function-local helper can still access types from the outer module through full paths
    // like `crate::layout::LayoutBox`.
    // RUST FUNDAMENTAL: Nested helper functions keep traversal logic local when it is only relevant to one outer function.
    fn walk(
        box_node: &crate::layout::LayoutBox,
        img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
        offset_x: i32,
        offset_y: i32,
    ) {
        let rect = box_node.rect();
        let styles = box_node.styles();
        // RUST FUNDAMENTAL: These are immutable borrows, so they can coexist while this frame of recursion reads state.

        // Draw background for non-text boxes
        if box_node.text().is_none() && !box_node.is_image() {
            // RUST FUNDAMENTAL: Chaining `.or_else(...).unwrap_or(...)` is a compact fallback ladder for optional values.
            let bg_color_str = styles
                .get("background-color")
                .or_else(|| styles.get("background"))
                .unwrap_or("transparent");
            // RUST FUNDAMENTAL: `unwrap_or(...)` returns the contained `&str` or the default, so the result is no longer an `Option`.

            if bg_color_str != "transparent" {
                let color = parse_screenshot_color(bg_color_str);
                draw_rect(
                    img,
                    (rect.x as i32 + offset_x) as u32,
                    (rect.y as i32 + offset_y) as u32,
                    rect.width as u32,
                    rect.height as u32,
                    color,
                );
            }

            let border = styles.border_width();
            // RUST FUNDAMENTAL: Storing this in a local avoids recomputing the helper for each edge check.
            if border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0 {
                let border_color =
                    parse_screenshot_color(styles.get("border-color").unwrap_or("#dadce0"));
                draw_border(
                    img,
                    (rect.x as i32 + offset_x) as u32,
                    (rect.y as i32 + offset_y) as u32,
                    rect.width as u32,
                    rect.height as u32,
                    border_color,
                );
            }
        }

        // Render images as colored placeholders
        if box_node.is_image() {
            let color = image::Rgba([220, 235, 250, 255]); // Light blue
            draw_rect(
                img,
                (rect.x as i32 + offset_x) as u32,
                (rect.y as i32 + offset_y) as u32,
                rect.width as u32,
                rect.height as u32,
                color,
            );

            // Draw border
            draw_border(
                img,
                (rect.x as i32 + offset_x) as u32,
                (rect.y as i32 + offset_y) as u32,
                rect.width as u32,
                rect.height as u32,
                image::Rgba([100, 150, 200, 255]), // Medium blue
            );
        }

        // Render text
        if let Some(text) = box_node.text() {
            // RUST FUNDAMENTAL: `if let Some(text)` both checks the enum variant and introduces `text` as a borrowed `&str`.
            let color_str = styles.get("color").unwrap_or("black");
            let color = parse_screenshot_color(color_str);
            let font_size = styles.font_size_px().filter(|&s| s > 0.0).unwrap_or(16.0);
            // RUST FUNDAMENTAL: `.filter(...)` on `Option<T>` keeps the value only when it passes the predicate.

            render_text_simple(
                img,
                text,
                (rect.x as i32 + offset_x) as i32,
                (rect.y as i32 + offset_y) as i32,
                color,
                font_size.max(4.0) as u32,
            );
        }

        // Recurse to children
        // RUST FUNDAMENTAL: Tree traversal is naturally recursive when each node performs work and then delegates the same work to its children.
        for child in box_node.children() {
            walk(child, img, offset_x, offset_y);
        }
    }

    walk(&root, img, offset_x, offset_y);
}

fn draw_border(
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: image::Rgba<u8>,
) {
    let (width, height) = img.dimensions();
    // RUST FUNDAMENTAL: Destructuring tuple returns into local bindings is a common Rust pattern for small related values.

    // Top and bottom edges
    for px in x..=(x + w).min(width - 1) {
        if y < height {
            img.put_pixel(px, y, color);
        }
        if y + h < height {
            img.put_pixel(px, y + h, color);
        }
    }

    // Left and right edges
    for py in y..=(y + h).min(height - 1) {
        if x < width {
            img.put_pixel(x, py, color);
        }
        if x + w < width {
            img.put_pixel(x + w, py, color);
        }
    }
}

fn render_text_simple(
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    text: &str,
    x: i32,
    y: i32,
    color: image::Rgba<u8>,
    font_size: u32,
) {
    let font_size = font_size as f32;
    // RUST FUNDAMENTAL: Explicit casts make numeric conversions visible, especially when switching between integer pixel sizes and float layout math.
    let text_run = crate::font::layout_text_run(text, font_size);
    let baseline_y = y as f32 + font_size * 0.75;

    for glyph in &text_run.glyphs {
        let ch = glyph.ch;
        if ch == '\n' {
            continue;
        }

        // RUST FUNDAMENTAL: Borrowing `&text_run.glyphs` means this loop reads glyph data without taking ownership of the shaped text run.
        draw_glyph_bitmap(
            img,
            ch,
            x as f32 + glyph.x,
            baseline_y + glyph.y_offset,
            font_size / 32.0,
            color,
        );
    }
}

fn draw_glyph_bitmap(
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    ch: char,
    x: f32,
    y: f32,
    scale: f32,
    color: image::Rgba<u8>,
) {
    let (width, height) = img.dimensions();
    let Some(metrics) = crate::font::get_glyph_metrics(ch) else {
        // RUST FUNDAMENTAL: `let ... else` is useful when the success path should stay unindented and the failure case exits early.
        return;
    };
    if metrics.width == 0 || metrics.height == 0 {
        return;
    }

    let (atlas, atlas_width, _) = crate::font::get_atlas_texture();
    // RUST FUNDAMENTAL: Tuple destructuring can ignore fields with `_` when only some returned values matter.
    let scale = scale.max(0.1);
    let draw_origin_x = x + metrics.x_offset as f32 * scale;
    let draw_origin_y = y + metrics.y_offset as f32 * scale;
    let scaled_width = (metrics.width as f32 * scale).ceil().max(1.0) as i32;
    let scaled_height = (metrics.height as f32 * scale).ceil().max(1.0) as i32;

    for dy in 0..scaled_height {
        for dx in 0..scaled_width {
            // RUST FUNDAMENTAL: The destination pixel grid may be larger than the source glyph bitmap when scaling up,
            // so the source coordinate is computed by dividing back down.
            let src_x = ((dx as f32) / scale).floor() as u32;
            let src_y = ((dy as f32) / scale).floor() as u32;
            if src_x >= metrics.width || src_y >= metrics.height {
                continue;
            }

            let atlas_x = metrics.x + src_x;
            let atlas_y = metrics.y + src_y;
            let atlas_idx = ((atlas_y * atlas_width + atlas_x) * 4 + 3) as usize;
            // RUST FUNDAMENTAL: The atlas is RGBA, so `+ 3` indexes the alpha byte of each 4-byte pixel.
            let alpha = atlas.get(atlas_idx).copied().unwrap_or(0);
            if alpha == 0 {
                continue;
            }

            let draw_x = draw_origin_x.round() as i32 + dx;
            let draw_y = draw_origin_y.round() as i32 + dy;
            if draw_x < 0 || draw_y < 0 || (draw_x as u32) >= width || (draw_y as u32) >= height {
                continue;
            }

            let dst = img.get_pixel_mut(draw_x as u32, draw_y as u32);
            let coverage = alpha as f32 / 255.0;
            let inv = 1.0 - coverage;
            // RUST FUNDAMENTAL: This is straight alpha blending: new_color * coverage + old_color * (1 - coverage).
            dst.0[0] = (color.0[0] as f32 * coverage + dst.0[0] as f32 * inv).round() as u8;
            dst.0[1] = (color.0[1] as f32 * coverage + dst.0[1] as f32 * inv).round() as u8;
            dst.0[2] = (color.0[2] as f32 * coverage + dst.0[2] as f32 * inv).round() as u8;
            dst.0[3] = 255;
        }
    }
}

fn parse_screenshot_color(color_str: &str) -> image::Rgba<u8> {
    let color_str = color_str.trim().to_lowercase();
    // RUST FUNDAMENTAL: `to_lowercase()` allocates a new `String` because lowercase conversion can change length.

    // Parse hex colors
    if color_str.starts_with('#') {
        let hex = &color_str[1..];
        // RUST FUNDAMENTAL: Slicing with `[1..]` works here because `#` is an ASCII one-byte prefix.
        if hex.len() == 6 {
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                return image::Rgba([
                    ((c >> 16) & 0xFF) as u8,
                    ((c >> 8) & 0xFF) as u8,
                    (c & 0xFF) as u8,
                    255,
                ]);
            }
        }
    }

    // Default colors
    match color_str.as_str() {
        // RUST FUNDAMENTAL: `as_str()` borrows a `&str` view from the owned `String` so it can be pattern-matched cheaply.
        "black" => image::Rgba([0, 0, 0, 255]),
        "white" => image::Rgba([255, 255, 255, 255]),
        "red" => image::Rgba([255, 0, 0, 255]),
        "blue" => image::Rgba([0, 0, 255, 255]),
        "green" => image::Rgba([0, 128, 0, 255]),
        "gray" | "grey" => image::Rgba([128, 128, 128, 255]),
        "coal" => image::Rgba([0x2E, 0x34, 0x40, 255]), // Aurora color
        _ => image::Rgba([64, 64, 64, 255]),            // Default dark gray
    }
}

fn draw_rect(
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: image::Rgba<u8>,
) {
    let (width, height) = img.dimensions();

    for py in y..=(y + h).min(height - 1) {
        for px in x..=(x + w).min(width - 1) {
            // RUST FUNDAMENTAL: The extra bounds check is defensive and keeps `put_pixel` from ever seeing an invalid coordinate.
            if px < width && py < height {
                img.put_pixel(px, py, color);
            }
        }
    }
}

struct AuroraApp<'a> {
    // RUST FUNDAMENTAL: This lifetime parameter says the app may borrow a `LayoutTree` that must outlive the app itself.
    layout: &'a LayoutTree,
    images: &'a ImageCache,
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    surface: Option<RenderSurface<'static>>,
    window: Option<Arc<Window>>,
    scroll_y: f64,
}

impl<'a> AuroraApp<'a> {
    fn new(layout: &'a LayoutTree, images: &'a ImageCache) -> Self {
        // RUST FUNDAMENTAL: `Self` inside an `impl` block is an alias for the enclosing type, here `AuroraApp<'a>`.
        Self {
            layout,
            images,
            context: RenderContext::new(),
            renderers: Vec::new(),
            surface: None,
            window: None,
            scroll_y: 0.0,
        }
    }

    fn render(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let _window = self.window.as_ref().unwrap();
        // RUST FUNDAMENTAL: `.as_ref()` converts `Option<T>` into `Option<&T>`, which lets us borrow instead of move out.
        let width = surface.config.width;
        let height = surface.config.height;
        let device_handle = &self.context.devices[surface.dev_id];

        let mut scene = Scene::new();
        // RUST FUNDAMENTAL: `mut` is required because scene-building methods append drawing commands into the scene.

        // Transform the scene based on scroll
        let transform = Affine::translate((0.0, -self.scroll_y));

        // Paint the layout
        scene.push_layer(
            Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            transform,
            &vello::kurbo::Rect::new(0.0, 0.0, width as f64, 10000.0),
        );
        GpuPainter::paint(self.layout.root(), &mut scene, self.images);
        scene.pop_layer();

        let surface_texture = surface
            .surface
            .get_current_texture()
            .expect("failed to get surface texture");
        // RUST FUNDAMENTAL: `expect(...)` is like `unwrap()` but records a clearer panic message for programmer errors.

        let render_params = vello::RenderParams {
            base_color: Color::WHITE,
            antialiasing_method: vello::AaConfig::Msaa16,
            width,
            height,
        };

        if self.renderers[surface.dev_id].is_none() {
            // RUST FUNDAMENTAL: The vector stores one optional renderer per GPU device id, so initialization is lazy and device-specific.
            self.renderers[surface.dev_id] = Some(
                Renderer::new(
                    &device_handle.device,
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

        let renderer = self.renderers[surface.dev_id].as_mut().unwrap();
        // RUST FUNDAMENTAL: `.as_mut()` gives a mutable reference to the renderer inside `Option<Renderer>`.
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
        // RUST FUNDAMENTAL: GPU APIs often collect commands into an encoder first and submit them later as one batch.
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

impl<'a> winit::application::ApplicationHandler for AuroraApp<'a> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attr = Window::default_attributes()
            .with_title("Aurora Browser (GPU Accelerated)")
            .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 900.0));
        // RUST FUNDAMENTAL: Builder-style APIs chain methods by returning the updated value each time.

        let window = Arc::new(
            event_loop
                .create_window(window_attr)
                .expect("failed to create window"),
        );
        self.window = Some(window.clone());
        // RUST FUNDAMENTAL: `Arc::clone` increments the reference count; it does not duplicate the underlying OS window.

        // Create surface
        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            1200,
            900,
            vello::wgpu::PresentMode::Fifo,
        ))
        .expect("failed to create surface");
        // RUST FUNDAMENTAL: `block_on` runs the async surface-creation future to completion in this synchronous callback.
        self.surface = Some(surface);

        self.renderers
            .resize_with(self.context.devices.len(), || None);
        // RUST FUNDAMENTAL: `resize_with` fills new vector slots by calling the closure once per added element.

        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            // RUST FUNDAMENTAL: Matching over an enum like `WindowEvent` is the idiomatic way to dispatch GUI events in Rust.
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(surface) = self.surface.as_mut() {
                    self.context
                        .resize_surface(surface, size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
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
            } => {
                // RUST FUNDAMENTAL: Pattern matching can destructure nested structs inline and ignore the rest with `..`.
                match logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::ArrowDown) => {
                        self.scroll_y += 20.0;
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        self.scroll_y = (self.scroll_y - 20.0).max(0.0);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
