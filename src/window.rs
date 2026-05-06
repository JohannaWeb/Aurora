// Import layout tree for rendering
// RUST FUNDAMENTAL: This module sits at the boundary between computed layout data and actual OS/window output.
use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
// Import GPU painter for Vello rendering
use crate::gpu_paint::GpuPainter;
// Import image cache for decoded image data
use crate::ImageCache;
use opus::domain::Identity;
// Import Arc for thread-safe sharing
// RUST FUNDAMENTAL: GUI/rendering stacks often need shared ownership across async or callback-driven code,
// which is why `Arc<T>` shows up frequently around GPU and window resources.
use std::sync::Arc;
// Import Vello graphics primitives
use vello::{
    // Import Affine for transformation matrices
    kurbo::{Affine, Rect as KRect, RoundedRect},
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

pub struct WindowInput {
    pub dom: NodePtr,
    pub stylesheet: Stylesheet,
    pub base_url: Option<String>,
    pub identity: Identity,
    pub viewport: ViewportSize,
    pub layout: LayoutTree,
    pub images: ImageCache,
}

pub(crate) const BROWSER_CHROME_HEIGHT: f32 = 174.0;

// Open interactive window for rendering layout
pub fn open(input: WindowInput) -> Result<(), String> {
    // Check environment variable for screenshot output path
    // RUST FUNDAMENTAL: Environment lookups return `Result<String, VarError>` because the variable may not exist.
    let screenshot_path = std::env::var("AURORA_SCREENSHOT");
    // If screenshot path provided, render to file instead of window
    if let Ok(path) = screenshot_path {
        // Render layout to PNG file
        render_to_file(&input, &path);
        // Return success
        return Ok(());
    }

    // Create new event loop for window events
    // RUST FUNDAMENTAL: `.map_err(...)` is useful when a lower-level library has its own error type
    // but this function wants to expose a simpler `String` error.
    let event_loop =
        EventLoop::new().map_err(|error| format!("failed to create event loop: {error}"))?;
    // Create Aurora application state with layout and image cache
    let mut app = AuroraApp::new(input);

    // Run event loop with application
    event_loop
        // Run the application
        .run_app(&mut app)
        // Map errors to string format
        .map_err(|error| format!("failed to run event loop: {error}"))
}

fn render_to_file(input: &WindowInput, path: &str) {
    use image::{ImageBuffer, Rgba};
    // RUST FUNDAMENTAL: A nested `use` is scoped to this function, which keeps file-level imports smaller
    // when a crate is only needed by one helper.

    eprintln!("Rendering to PNG: {}", path);
    // RUST FUNDAMENTAL: `eprintln!` writes to stderr, which is useful for status messages that should not be
    // mixed into normal stdout output.

    // RUST FUNDAMENTAL: Unsuffixed integer literals are inferred by context; here the explicit `u32` suffix fixes the type directly.
    let width = env_u32("AURORA_SCREENSHOT_WIDTH")
        .or_else(|| env_u32("AURORA_VIEWPORT_WIDTH"))
        .unwrap_or(1200);
    let height = env_u32("AURORA_SCREENSHOT_HEIGHT")
        .or_else(|| env_u32("AURORA_VIEWPORT_HEIGHT"))
        .unwrap_or(1024);

    // Create a white background
    let mut img = ImageBuffer::new(width, height);

    // Fill with white
    // RUST FUNDAMENTAL: Iterating over `pixels_mut()` yields mutable references to each pixel in the image buffer.
    for pixel in img.pixels_mut() {
        *pixel = Rgba([255, 255, 255, 255]);
    }

    render_layout_with_text(
        &input.layout,
        &mut img,
        0,
        BROWSER_CHROME_HEIGHT.round() as i32,
    );
    render_scrollbars(
        input.layout.root(),
        &mut img,
        0,
        BROWSER_CHROME_HEIGHT.round() as i32,
    );
    render_browser_chrome(
        &mut img,
        width,
        input.base_url.as_deref().unwrap_or("aurora://local"),
    );

    // Save to file
    if let Err(e) = img.save(path) {
        eprintln!("Failed to save screenshot: {}", e);
    } else {
        eprintln!("Screenshot saved to {}", path);
    }
}

fn render_scrollbars(
    layout_box: &crate::layout::LayoutBox,
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    offset_x: i32,
    offset_y: i32,
) {
    draw_scrollbar_if_needed(layout_box, img, offset_x, offset_y);
    for child in layout_box.children() {
        render_scrollbars(child, img, offset_x, offset_y);
    }
}

fn draw_scrollbar_if_needed(
    layout_box: &crate::layout::LayoutBox,
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    offset_x: i32,
    offset_y: i32,
) {
    let styles = layout_box.styles();
    let has_scrollbar = matches!(
        styles.get("overflow-y").or_else(|| styles.get("overflow")),
        Some("scroll")
    );
    if !has_scrollbar {
        return;
    }

    let rect = layout_box.rect();
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let (_, image_height) = img.dimensions();
    let track_width = 10.0_f32;
    let track_margin = 3.0_f32;
    let track_x = rect.x + offset_x as f32 + rect.width - track_width - track_margin;
    let track_top = (rect.y + offset_y as f32).max(0.0);
    let track_bottom = (rect.y + rect.height)
        .min(image_height as f32)
        .max(track_top);
    let track_height = (track_bottom - track_top).max(0.0);
    if track_height < 24.0 {
        return;
    }

    let content_height = scroll_content_height(layout_box).max(rect.height);
    let thumb_height =
        (rect.height / content_height * track_height).clamp(48.0, track_height.max(48.0));
    let thumb_top = track_top + 10.0;
    let thumb_bottom = (thumb_top + thumb_height)
        .min(track_bottom - 10.0)
        .max(thumb_top);

    draw_rect(
        img,
        track_x.round().max(0.0) as u32,
        (track_top + 8.0).round().max(0.0) as u32,
        track_width.round() as u32,
        (track_bottom - track_top - 16.0).round().max(1.0) as u32,
        image::Rgba([222, 227, 232, 210]),
    );
    draw_rect(
        img,
        (track_x + 2.0).round().max(0.0) as u32,
        thumb_top.round().max(0.0) as u32,
        (track_width - 4.0).round() as u32,
        (thumb_bottom - thumb_top).round().max(1.0) as u32,
        image::Rgba([144, 153, 164, 230]),
    );
}

fn render_browser_chrome(
    img: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    width: u32,
    url: &str,
) {
    let display_url = chrome_display_url(url);
    let chrome_h = BROWSER_CHROME_HEIGHT.round() as u32;
    draw_rect(img, 0, 0, width, chrome_h, image::Rgba([7, 11, 15, 255]));
    draw_border(
        img,
        0,
        0,
        width.saturating_sub(1),
        chrome_h,
        image::Rgba([27, 34, 42, 255]),
    );

    draw_rect(img, 15, 31, 18, 18, image::Rgba([53, 208, 127, 255]));
    render_text_simple(img, "AURORA", 43, 32, image::Rgba([243, 246, 248, 255]), 14);
    render_text_simple(img, "0.3.1", 137, 32, image::Rgba([66, 80, 95, 255]), 13);
    render_text_simple(
        img,
        "sovereign render path · session 0x4f:c2",
        (width as i32 / 2) - 240,
        32,
        image::Rgba([140, 146, 155, 255]),
        14,
    );
    draw_rect(
        img,
        width.saturating_sub(193),
        25,
        148,
        31,
        image::Rgba([7, 11, 15, 255]),
    );
    draw_border(
        img,
        width.saturating_sub(193),
        25,
        148,
        31,
        image::Rgba([194, 203, 213, 255]),
    );
    render_text_simple(
        img,
        "WGPU · VELLO",
        width.saturating_sub(180) as i32,
        32,
        image::Rgba([194, 203, 213, 255]),
        13,
    );

    draw_rect(img, 14, 70, 175, 40, image::Rgba([14, 23, 23, 255]));
    draw_border(img, 14, 70, 175, 40, image::Rgba([26, 58, 50, 255]));
    render_text_simple(
        img,
        "aurora · sove...",
        45,
        82,
        image::Rgba([240, 245, 242, 255]),
        14,
    );
    render_text_simple(
        img,
        "atlas · font...",
        235,
        82,
        image::Rgba([98, 107, 117, 255]),
        14,
    );
    render_text_simple(
        img,
        "did:plc:k7q3...m...",
        425,
        82,
        image::Rgba([98, 107, 117, 255]),
        14,
    );
    render_text_simple(
        img,
        "bastion / opu...",
        616,
        82,
        image::Rgba([98, 107, 117, 255]),
        14,
    );
    render_text_simple(
        img,
        "loading...",
        807,
        82,
        image::Rgba([98, 107, 117, 255]),
        14,
    );
    render_text_simple(img, "+", 969, 77, image::Rgba([111, 120, 130, 255]), 22);
    render_text_simple(
        img,
        "5 tabs      mem 184 mb      gpu 12%",
        width.saturating_sub(330) as i32,
        82,
        image::Rgba([88, 97, 107, 255]),
        13,
    );

    render_text_simple(img, "‹", 16, 130, image::Rgba([199, 206, 212, 255]), 24);
    render_text_simple(img, "›", 58, 130, image::Rgba([199, 206, 212, 255]), 24);
    render_text_simple(img, "↻", 100, 130, image::Rgba([199, 206, 212, 255]), 24);

    let urlbar_x = 135;
    let urlbar_w = width.saturating_sub(390).max(360);
    draw_rect(
        img,
        urlbar_x,
        124,
        urlbar_w,
        42,
        image::Rgba([11, 17, 23, 255]),
    );
    draw_border(
        img,
        urlbar_x,
        124,
        urlbar_w,
        42,
        image::Rgba([38, 48, 58, 255]),
    );
    draw_rect(img, 148, 130, 69, 30, image::Rgba([11, 17, 23, 255]));
    draw_border(img, 148, 130, 69, 30, image::Rgba([36, 79, 61, 255]));
    render_text_simple(img, "TLS", 163, 136, image::Rgba([65, 204, 120, 255]), 13);
    render_text_simple(img, "/", 231, 130, image::Rgba([40, 49, 58, 255]), 24);
    render_text_simple(
        img,
        &truncate_chrome_text(&display_url, 43),
        269,
        135,
        image::Rgba([122, 130, 139, 255]),
        16,
    );
    let diag_x = width.saturating_sub(610);
    draw_rect(img, diag_x, 128, 355, 32, image::Rgba([18, 24, 33, 255]));
    draw_border(img, diag_x, 128, 355, 32, image::Rgba([29, 38, 48, 255]));
    render_text_simple(
        img,
        "dom 412 · style 38 · layout 96",
        diag_x as i32 + 13,
        136,
        image::Rgba([112, 121, 132, 255]),
        12,
    );
    let identity_x = width.saturating_sub(241);
    draw_rect(
        img,
        identity_x,
        124,
        205,
        42,
        image::Rgba([11, 17, 23, 255]),
    );
    draw_border(
        img,
        identity_x,
        124,
        205,
        42,
        image::Rgba([38, 48, 58, 255]),
    );
    draw_rect(
        img,
        identity_x + 11,
        130,
        30,
        30,
        image::Rgba([51, 209, 122, 255]),
    );
    render_text_simple(
        img,
        "JW",
        identity_x as i32 + 15,
        137,
        image::Rgba([6, 34, 20, 255]),
        12,
    );
    render_text_simple(
        img,
        "@johanna.aurora",
        identity_x as i32 + 51,
        136,
        image::Rgba([238, 243, 246, 255]),
        12,
    );
}

fn truncate_chrome_text(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn chrome_display_url(url: &str) -> String {
    if url.contains("/fixtures/aurora-search/") {
        "https://aurora.sovereign/search".to_string()
    } else if url.contains("/fixtures/google-homepage/") {
        "https://google.com/search".to_string()
    } else if url.contains("/fixtures/demo/") {
        "aurora://fixture/demo".to_string()
    } else {
        url.to_string()
    }
}

fn scroll_content_height(layout_box: &crate::layout::LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    (bottom - rect.y).max(rect.height)
}

fn max_box_bottom(layout_box: &crate::layout::LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    bottom
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse::<u32>().ok()
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

struct AuroraApp {
    input: WindowInput,
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    surface: Option<RenderSurface<'static>>,
    window: Option<Arc<Window>>,
    scroll_y: f64,
}

impl AuroraApp {
    fn new(input: WindowInput) -> Self {
        // RUST FUNDAMENTAL: `Self` inside an `impl` block is an alias for the enclosing type, here `AuroraApp<'a>`.
        Self {
            input,
            context: RenderContext::new(),
            renderers: Vec::new(),
            surface: None,
            window: None,
            scroll_y: 0.0,
        }
    }

    fn reflow(&mut self, width: u32, height: u32) {
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

    fn render(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let _window = self.window.as_ref().unwrap();
        // RUST FUNDAMENTAL: `.as_ref()` converts `Option<T>` into `Option<&T>`, which lets us borrow instead of move out.
        let width = surface.config.width;
        let height = surface.config.height;
        let device_handle = &self.context.devices[surface.dev_id];

        let mut scene = Scene::new();
        // RUST FUNDAMENTAL: `mut` is required because scene-building methods append drawing commands into the scene.

        let content_top = BROWSER_CHROME_HEIGHT as f64;
        let transform = Affine::translate((0.0, content_top - self.scroll_y));

        // Paint the layout
        scene.push_layer(
            Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            transform,
            &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
        );
        GpuPainter::paint(self.input.layout.root(), &mut scene, &self.input.images);
        scene.pop_layer();
        scene.push_layer(
            Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            Affine::translate((0.0, content_top)),
            &vello::kurbo::Rect::new(0.0, content_top, width as f64, height as f64),
        );
        GpuPainter::paint_scrollbars(
            self.input.layout.root(),
            &mut scene,
            (height as f32 - BROWSER_CHROME_HEIGHT).max(1.0),
        );
        scene.pop_layer();
        paint_browser_chrome_scene(
            &mut scene,
            width,
            self.input.base_url.as_deref().unwrap_or("aurora://local"),
        );

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

fn paint_browser_chrome_scene(scene: &mut Scene, width: u32, url: &str) {
    let display_url = chrome_display_url(url);
    fill_scene_rect(
        scene,
        0.0,
        0.0,
        width as f64,
        BROWSER_CHROME_HEIGHT as f64,
        Color::from_rgb8(7, 11, 15),
    );
    stroke_scene_rect(
        scene,
        0.5,
        0.5,
        width as f64 - 1.0,
        BROWSER_CHROME_HEIGHT as f64 - 1.0,
        Color::from_rgb8(27, 34, 42),
    );

    fill_scene_rect(
        scene,
        15.0,
        31.0,
        18.0,
        18.0,
        Color::from_rgb8(53, 208, 127),
    );
    GpuPainter::paint_text_label(
        scene,
        "AURORA",
        43.0,
        32.0,
        14.0,
        Color::from_rgb8(243, 246, 248),
    );
    GpuPainter::paint_text_label(
        scene,
        "0.3.1",
        137.0,
        32.0,
        13.0,
        Color::from_rgb8(66, 80, 95),
    );
    GpuPainter::paint_text_label(
        scene,
        "sovereign render path · session 0x4f:c2",
        width as f64 / 2.0 - 240.0,
        32.0,
        14.0,
        Color::from_rgb8(140, 146, 155),
    );
    let engine_x = width as f64 - 193.0;
    stroke_scene_rect(
        scene,
        engine_x,
        25.0,
        148.0,
        31.0,
        Color::from_rgb8(194, 203, 213),
    );
    GpuPainter::paint_text_label(
        scene,
        "WGPU · VELLO",
        engine_x + 13.0,
        32.0,
        13.0,
        Color::from_rgb8(194, 203, 213),
    );

    fill_scene_rect(scene, 14.0, 70.0, 175.0, 40.0, Color::from_rgb8(14, 23, 23));
    stroke_scene_rect(scene, 14.0, 70.0, 175.0, 40.0, Color::from_rgb8(26, 58, 50));
    GpuPainter::paint_text_label(
        scene,
        "aurora · sove...",
        45.0,
        82.0,
        14.0,
        Color::from_rgb8(240, 245, 242),
    );
    GpuPainter::paint_text_label(
        scene,
        "atlas · font...",
        235.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    GpuPainter::paint_text_label(
        scene,
        "did:plc:k7q3...m...",
        425.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    GpuPainter::paint_text_label(
        scene,
        "bastion / opu...",
        616.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    GpuPainter::paint_text_label(
        scene,
        "loading...",
        807.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    GpuPainter::paint_text_label(
        scene,
        "+",
        969.0,
        77.0,
        22.0,
        Color::from_rgb8(111, 120, 130),
    );
    GpuPainter::paint_text_label(
        scene,
        "5 tabs      mem 184 mb      gpu 12%",
        width as f64 - 330.0,
        82.0,
        13.0,
        Color::from_rgb8(88, 97, 107),
    );

    GpuPainter::paint_text_label(
        scene,
        "‹",
        16.0,
        130.0,
        24.0,
        Color::from_rgb8(199, 206, 212),
    );
    GpuPainter::paint_text_label(
        scene,
        "›",
        58.0,
        130.0,
        24.0,
        Color::from_rgb8(199, 206, 212),
    );
    GpuPainter::paint_text_label(
        scene,
        "↻",
        100.0,
        130.0,
        24.0,
        Color::from_rgb8(199, 206, 212),
    );

    let urlbar_w = (width as f64 - 390.0).max(360.0);
    fill_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(11, 17, 23),
    );
    stroke_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(38, 48, 58),
    );
    stroke_scene_rect(
        scene,
        148.0,
        130.0,
        69.0,
        30.0,
        Color::from_rgb8(36, 79, 61),
    );
    GpuPainter::paint_text_label(
        scene,
        "TLS",
        163.0,
        136.0,
        13.0,
        Color::from_rgb8(65, 204, 120),
    );
    GpuPainter::paint_text_label(scene, "/", 231.0, 130.0, 24.0, Color::from_rgb8(40, 49, 58));
    GpuPainter::paint_text_label(
        scene,
        &truncate_chrome_text(&display_url, 43),
        269.0,
        135.0,
        16.0,
        Color::from_rgb8(122, 130, 139),
    );
    let diag_x = width as f64 - 610.0;
    fill_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(18, 24, 33),
    );
    stroke_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(29, 38, 48),
    );
    GpuPainter::paint_text_label(
        scene,
        "dom 412 · style 38 · layout 96",
        diag_x + 13.0,
        136.0,
        12.0,
        Color::from_rgb8(112, 121, 132),
    );
    let identity_x = width as f64 - 241.0;
    fill_scene_rect(
        scene,
        identity_x,
        124.0,
        205.0,
        42.0,
        Color::from_rgb8(11, 17, 23),
    );
    stroke_scene_rect(
        scene,
        identity_x,
        124.0,
        205.0,
        42.0,
        Color::from_rgb8(38, 48, 58),
    );
    fill_scene_rect(
        scene,
        identity_x + 11.0,
        130.0,
        30.0,
        30.0,
        Color::from_rgb8(51, 209, 122),
    );
    GpuPainter::paint_text_label(
        scene,
        "JW",
        identity_x + 15.0,
        137.0,
        12.0,
        Color::from_rgb8(6, 34, 20),
    );
    GpuPainter::paint_text_label(
        scene,
        "@johanna.aurora",
        identity_x + 51.0,
        136.0,
        12.0,
        Color::from_rgb8(238, 243, 246),
    );
}

fn fill_scene_rect(scene: &mut Scene, x: f64, y: f64, width: f64, height: f64, color: Color) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &KRect::new(x, y, x + width, y + height),
    );
}

fn stroke_scene_rect(scene: &mut Scene, x: f64, y: f64, width: f64, height: f64, color: Color) {
    scene.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        color,
        None,
        &RoundedRect::from_rect(KRect::new(x, y, x + width, y + height), 0.0),
    );
}

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
            initial_width,
            initial_height,
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
                self.reflow(size.width, size.height);
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
