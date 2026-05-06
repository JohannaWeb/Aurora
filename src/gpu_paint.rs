// Import layout box for rendering
// RUST FUNDAMENTAL: This painter consumes already-computed layout data rather than parsing or styling directly.
use crate::layout::LayoutBox;
// Import image cache for decoded image data
use crate::ImageCache;
// Import Vello graphics primitives
// RUST FUNDAMENTAL: `Rect as KRect` renames the imported type to avoid colliding with the project's own `Rect`.
use vello::kurbo::{Affine, Rect as KRect, RoundedRect};
// Import color and fill types
use peniko::{Color, Fill, ImageBrush};
// Import Vello scene for GPU rendering
use vello::Scene;
// Import OnceLock for lazy static initialization (thread-safe)
use std::sync::OnceLock;
// Import Mutex for thread-safe interior mutability
use std::sync::Mutex;
// Import HashMap for color caching
// RUST FUNDAMENTAL: Caches are often modeled as maps from a stable key to a computed value.
use std::collections::HashMap;

// Global cache for dominant image colors (lazy-initialized, thread-safe)
// RUST FUNDAMENTAL: OnceLock<Mutex<T>> provides thread-safe lazy initialization
// T is computed once, then Mutex protects shared access
static IMAGE_COLOR_CACHE: OnceLock<Mutex<HashMap<String, [u8; 3]>>> = OnceLock::new();

// Get or initialize the image color cache
fn get_image_color_cache() -> &'static Mutex<HashMap<String, [u8; 3]>> {
    // RUST FUNDAMENTAL: 'static lifetime - valid for entire program duration
    // get_or_init() lazily initializes on first call, returns same reference thereafter
    IMAGE_COLOR_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// GPU painter for rendering layout boxes to Vello scene
// RUST FUNDAMENTAL: Unit struct (no data) - used for organizing methods via impl
pub struct GpuPainter;
// RUST FUNDAMENTAL: A unit struct is zero-sized, so it costs nothing to instantiate; here it is just a namespace for methods.

// GpuPainter implementation
impl GpuPainter {
    // Paint layout box with full opacity
    pub fn paint(layout_box: &LayoutBox, scene: &mut Scene, images: &ImageCache) {
        // Delegate to opacity-aware version with 1.0 opacity
        Self::paint_with_opacity(layout_box, scene, 1.0, images);
    }

    pub fn paint_scrollbars(layout_box: &LayoutBox, scene: &mut Scene, viewport_height: f32) {
        paint_scrollbar_if_needed(layout_box, scene, viewport_height);
        for child in layout_box.children() {
            Self::paint_scrollbars(child, scene, viewport_height);
        }
    }

    pub fn paint_text_label(
        scene: &mut Scene,
        text: &str,
        x: f64,
        y: f64,
        font_size: f32,
        color: Color,
    ) {
        paint_text_label(scene, text, x, y, font_size, color);
    }

    // Paint layout box with opacity blending through tree
    // RUST FUNDAMENTAL: parent_opacity f32 parameter shows how Rust passes primitives by value (Copy type)
    fn paint_with_opacity(
        layout_box: &LayoutBox,
        scene: &mut Scene,
        parent_opacity: f32,
        images: &ImageCache,
    ) {
        // Get styles from layout box
        // RUST FUNDAMENTAL: Borrowing the styles via `&StyleMap` avoids cloning per render step.
        let styles = layout_box.styles();
        // Get opacity property (0.0 to 1.0)
        let opacity = styles.opacity();
        // Calculate effective opacity combining parent and current
        let effective_opacity = parent_opacity * opacity;

        // Skip rendering if completely transparent or hidden
        if effective_opacity < 0.01 || styles.visibility() == "hidden" {
            return;
        }

        // Get position and size
        // RUST FUNDAMENTAL: `rect()` returns by value because `Rect` is a small `Copy` type.
        let rect = layout_box.rect();
        // RUST FUNDAMENTAL: Keeping `rect` in a local also makes the later branches easier to read than repeating `layout_box.rect()`.

        // Render based on box type
        if layout_box.is_viewport() {
            // Viewport: fill entire background
            let bg_color = parse_color(styles.background_color().unwrap_or("white"));
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                bg_color,
                None,
                &KRect::new(
                    rect.x as f64,
                    rect.y as f64,
                    (rect.x + rect.width) as f64,
                    (rect.y + rect.height) as f64,
                ),
            );
        } else if layout_box.is_image() {
            // Image: render with decoded pixels when available, else gray placeholder
            paint_image(layout_box, scene, images);
        } else if let Some(text) = layout_box.text() {
            // Text: render with opacity
            paint_text_with_opacity(layout_box, text, scene, effective_opacity);
        } else {
            // Element: render with borders and background
            paint_element_with_opacity(layout_box, scene, effective_opacity);
        }

        // Recursively paint children with inherited opacity
        // RUST FUNDAMENTAL: for loop borrows children iterator
        for child in layout_box.children() {
            Self::paint_with_opacity(child, scene, effective_opacity, images);
        }
    }
}

fn paint_text_label(scene: &mut Scene, text: &str, x: f64, y: f64, font_size: f32, color: Color) {
    let text_run = crate::font::layout_text_run(text, font_size);
    let baseline_y = y + font_size as f64 * 0.75;
    let base_r = (color.components[0] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_g = (color.components[1] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_b = (color.components[2] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_a = (color.components[3] * 255.0).round().clamp(0.0, 255.0) as u8;

    for glyph in &text_run.glyphs {
        if let Some(raster) = crate::font::rasterize_glyph(glyph.ch, font_size) {
            let gx = x + glyph.x as f64 + raster.x_offset as f64;
            let gy = baseline_y + glyph.y_offset as f64 + raster.y_offset as f64;

            for row in 0..raster.height {
                for col in 0..raster.width {
                    let idx = (row * raster.width + col) as usize;
                    let alpha = raster.bitmap.get(idx).copied().unwrap_or(0);
                    if alpha == 0 {
                        continue;
                    }

                    let coverage = alpha as f32 / 255.0;
                    let glyph_alpha = (base_a as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                    let glyph_color = Color::from_rgba8(base_r, base_g, base_b, glyph_alpha);
                    let pixel_rect = KRect::new(
                        gx + col as f64,
                        gy + row as f64,
                        gx + col as f64 + 1.0,
                        gy + row as f64 + 1.0,
                    );
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        glyph_color,
                        None,
                        &pixel_rect,
                    );
                }
            }
        }
    }
}

fn paint_scrollbar_if_needed(layout_box: &LayoutBox, scene: &mut Scene, viewport_height: f32) {
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

    let track_width = 10.0_f64;
    let track_margin = 3.0_f64;
    let track_x = (rect.x + rect.width) as f64 - track_width - track_margin;
    let track_top = rect.y.max(0.0) as f64;
    let track_bottom = (rect.y + rect.height).min(viewport_height).max(rect.y) as f64;
    let track_height = (track_bottom - track_top).max(0.0);
    if track_height < 24.0 {
        return;
    }

    let content_height = scroll_content_height(layout_box).max(rect.height);
    let thumb_height =
        ((rect.height / content_height) as f64 * track_height).clamp(48.0, track_height.max(48.0));
    let thumb_top = track_top + 10.0;
    let thumb_bottom = (thumb_top + thumb_height)
        .min(track_bottom - 10.0)
        .max(thumb_top);

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(222, 227, 232, 210),
        None,
        &RoundedRect::from_rect(
            KRect::new(
                track_x,
                track_top + 8.0,
                track_x + track_width,
                track_bottom - 8.0,
            ),
            5.0,
        ),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(144, 153, 164, 230),
        None,
        &RoundedRect::from_rect(
            KRect::new(
                track_x + 2.0,
                thumb_top,
                track_x + track_width - 2.0,
                thumb_bottom,
            ),
            4.0,
        ),
    );
}

fn scroll_content_height(layout_box: &LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    (bottom - rect.y).max(rect.height)
}

fn max_box_bottom(layout_box: &LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    bottom
}

// Paint an element (div, section, etc.) with background, borders, and shadow
fn paint_element_with_opacity(layout_box: &LayoutBox, scene: &mut Scene, opacity: f32) {
    // Get position/size rect
    let r = layout_box.rect();
    // Get computed styles
    let styles = layout_box.styles();

    // Get background color, trying multiple property names
    // RUST FUNDAMENTAL: .or_else() chains Option<T> handling; unwrap_or() provides default
    let bg_color_name = styles
        .get("background-color")
        .or_else(|| styles.get("background"))
        .unwrap_or("transparent");
    // Parse color string into Color object
    let mut bg_color = parse_color(bg_color_name);

    // Get border color, default to black
    let mut border_color = parse_color(styles.get("border-color").unwrap_or("black"));
    // Get border width
    let border = layout_box.styles().border_width();
    // RUST FUNDAMENTAL: Borrowing styles twice is okay here because both borrows are immutable.

    // Apply opacity to colors by multiplying alpha channel
    // RUST FUNDAMENTAL: .components[3] accesses array; *= modifies in-place
    bg_color.components[3] *= opacity;
    border_color.components[3] *= opacity;
    // RUST FUNDAMENTAL: Mutating small value types like colors in local variables is common when deriving render-time variants from style data.

    // Parse border-radius, default to 0.0
    let radius = if let Some(radius_str) = styles.get("border-radius") {
        // RUST FUNDAMENTAL: if let Some(x) = option unwraps Some variant
        // trim_end_matches() removes "px" suffix, parse() converts string to f32
        radius_str
            .trim_end_matches("px")
            .parse::<f32>()
            .unwrap_or(0.0) as f64
    } else {
        0.0
    };

    // Create rectangle in Vello's coordinate system
    let k_rect = KRect::new(
        r.x as f64,
        r.y as f64,
        (r.x + r.width) as f64,
        (r.y + r.height) as f64,
    );
    // Create rounded rectangle with border radius
    // RUST FUNDAMENTAL: Adapters like `KRect` and `RoundedRect` bridge your own data structures to library-specific geometry types.
    let rounded_rect = RoundedRect::from_rect(k_rect, radius);

    // Paint drop shadow if present
    if let Some(shadow_str) = styles.get("box-shadow") {
        if shadow_str != "none" {
            // RUST FUNDAMENTAL: This branch only checks for presence, not a fully parsed box-shadow syntax yet.
            // Create semi-transparent black shadow color with opacity
            let shadow_color = Color::from_rgba8(0, 0, 0, ((60.0 * opacity) as u8).min(255));
            // Shadow offset slightly down and right
            let shadow_rect = KRect::new(
                (r.x + 3.0) as f64,
                (r.y + 3.0) as f64,
                (r.x + r.width + 3.0) as f64,
                (r.y + r.height + 3.0) as f64,
            );
            // Fill shadow rectangle on scene
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                shadow_color,
                None,
                &RoundedRect::from_rect(shadow_rect, radius.max(2.0)),
            );
        }
    }

    // Paint background if not fully transparent
    if bg_color.components[3] > 0.0 {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            bg_color,
            None,
            &rounded_rect,
        );
    }

    if border.top > 0.0 {
        let stroke_width = border.top as f64;
        // RUST FUNDAMENTAL: The cast to `f64` matches the geometry library's coordinate type.
        scene.stroke(
            &vello::kurbo::Stroke::new(stroke_width),
            Affine::IDENTITY,
            border_color,
            None,
            &rounded_rect,
        );
    }
}

fn paint_text_with_opacity(layout_box: &LayoutBox, text: &str, scene: &mut Scene, opacity: f32) {
    let r = layout_box.rect();
    let styles = layout_box.styles();
    let mut text_color = parse_color(styles.get("color").unwrap_or("black"));
    text_color.components[3] *= opacity;
    // RUST FUNDAMENTAL: Alpha is stored as a float component in this color type, so multiplying by opacity preserves the color channels.

    let mut font_size = styles.font_size_px().filter(|&s| s > 0.0).unwrap_or(16.0);
    if let Ok(zoom_str) = std::env::var("AURORA_ZOOM") {
        if let Ok(zoom) = zoom_str.parse::<f32>() {
            // RUST FUNDAMENTAL: Nested `if let Ok(...)` is a simple way to handle successive fallible steps without introducing extra helper functions.
            font_size *= zoom;
        }
    }

    let text_align = styles.text_align();
    let text_decoration = styles.get("text-decoration").unwrap_or("none");
    // RUST FUNDAMENTAL: These helpers hide raw CSS strings behind typed or normalized accessors.

    let text_run = crate::font::layout_text_run(text, font_size);
    let text_width = text_run.width as f64;
    // RUST FUNDAMENTAL: Matching on a small enum like `TextAlign` is clearer than encoding alignment as raw numbers.
    let offset_x = match text_align {
        crate::css::TextAlign::Center => (r.width as f64 - text_width).max(0.0) / 2.0,
        crate::css::TextAlign::Right => (r.width as f64 - text_width).max(0.0),
        crate::css::TextAlign::Left => 0.0,
    };

    let baseline_y = r.y as f64 + font_size as f64 * 0.75;
    let base_r = (text_color.components[0] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_g = (text_color.components[1] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_b = (text_color.components[2] * 255.0).round().clamp(0.0, 255.0) as u8;
    let base_a = (text_color.components[3] * 255.0).round().clamp(0.0, 255.0) as u8;

    for glyph in &text_run.glyphs {
        // RUST FUNDAMENTAL: Iterating by reference avoids moving the glyph vector out of `text_run`.
        if let Some(raster) = crate::font::rasterize_glyph(glyph.ch, font_size) {
            let gx = r.x as f64 + offset_x + glyph.x as f64 + raster.x_offset as f64;
            let gy = baseline_y + glyph.y_offset as f64 + raster.y_offset as f64;

            for row in 0..raster.height {
                for col in 0..raster.width {
                    let idx = (row * raster.width + col) as usize;
                    // RUST FUNDAMENTAL: This is the same row-major indexing pattern used for many image and matrix buffers.
                    let alpha = raster.bitmap.get(idx).copied().unwrap_or(0);
                    if alpha == 0 {
                        continue;
                    }

                    let coverage = alpha as f32 / 255.0;
                    // RUST FUNDAMENTAL: This loop effectively expands one rasterized glyph bitmap into many tiny filled rectangles,
                    // one per covered pixel.
                    let glyph_alpha = (base_a as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                    let glyph_color = Color::from_rgba8(base_r, base_g, base_b, glyph_alpha);
                    let pixel_rect = KRect::new(
                        gx + col as f64,
                        gy + row as f64,
                        gx + col as f64 + 1.0,
                        gy + row as f64 + 1.0,
                    );
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        glyph_color,
                        None,
                        &pixel_rect,
                    );
                }
            }
        }
    }

    if text_decoration != "none" {
        // RUST FUNDAMENTAL: Decorations are derived after glyph painting because they depend on measured run width and baseline.
        let text_end_x = r.x as f64 + offset_x + text_width;
        let stroke_width_deco = (font_size * 0.1).max(1.0) as f64;
        if text_decoration.contains("underline") {
            let line_y = baseline_y + font_size as f64 * 0.1;
            scene.stroke(
                &vello::kurbo::Stroke::new(stroke_width_deco),
                Affine::IDENTITY,
                text_color,
                None,
                &vello::kurbo::Line::new((r.x as f64 + offset_x, line_y), (text_end_x, line_y)),
            );
        }
    }
}

fn parse_color(name: &str) -> Color {
    let name = name.trim().to_lowercase();
    // RUST FUNDAMENTAL: Normalizing input once at the top avoids repeating case/whitespace handling in every match arm.
    if name.starts_with('#') {
        let hex = &name[1..];
        if hex.len() == 6 {
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                return Color::from_rgb8(
                    ((c >> 16) & 0xFF) as u8,
                    ((c >> 8) & 0xFF) as u8,
                    (c & 0xFF) as u8,
                );
            }
        }
    }
    match name.as_str() {
        // RUST FUNDAMENTAL: Match expressions must be exhaustive, which is why there is a final `_` fallback arm.
        "white" | "#fff" => Color::WHITE,
        "black" | "#000" => Color::BLACK,
        "red" => Color::from_rgb8(255, 0, 0),
        "blue" => Color::from_rgb8(0, 0, 255),
        "green" => Color::from_rgb8(0, 128, 0),
        "transparent" => Color::TRANSPARENT,
        "aurora-cyan" => Color::from_rgb8(0x88, 0xC0, 0xD0),
        "coal" => Color::from_rgb8(0x2E, 0x34, 0x40),
        "rust" => Color::from_rgb8(0xBF, 0x61, 0x6A),
        _ => Color::from_rgb8(0x4C, 0x56, 0x6A),
    }
}

fn paint_image(layout_box: &LayoutBox, scene: &mut Scene, images: &ImageCache) {
    let r = layout_box.rect();
    let k_rect = KRect::new(
        r.x as f64,
        r.y as f64,
        (r.x + r.width) as f64,
        (r.y + r.height) as f64,
    );

    // If the image was fetched and decoded, render it; otherwise fall back to a gray placeholder.
    if let Some(src) = layout_box.image_src() {
        if let Some(img_data) = images.get(src) {
            if img_data.width > 0 && img_data.height > 0 && r.width > 0.0 && r.height > 0.0 {
                let scale_x = r.width as f64 / img_data.width as f64;
                let scale_y = r.height as f64 / img_data.height as f64;
                let affine = Affine::translate((r.x as f64, r.y as f64))
                    * Affine::scale_non_uniform(scale_x, scale_y);
                let brush = ImageBrush::new(img_data.clone());
                scene.draw_image(brush.as_ref(), affine);
                return;
            }
        }
    }

    // Fallback: gray placeholder rectangle
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(200, 200, 200),
        None,
        &k_rect,
    );
}
