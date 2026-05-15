//! Headless rendering pipeline using ImageBackend.
//! Used by visual regression tests — no GPU, no display required.

use crate::css::Stylesheet;
use crate::html::Parser;
use crate::layout::{LayoutTree, ViewportSize};
use crate::render::ImageBackend;
use crate::style::StyleTree;
use image::RgbaImage;
use opus::domain::{Capability, Identity, IdentityKind};

/// Render an HTML string to an RGBA image at the given viewport size.
/// Used by visual regression tests.
pub fn render_to_image(html: &str, width: u32, height: u32) -> RgbaImage {
    let identity = headless_identity();
    let viewport = ViewportSize {
        width: width as f32,
        height: height as f32,
    };

    // Parse HTML.
    let dom = Parser::new(html).parse_document();

    // Build stylesheet and layout.
    let mut stylesheet = Stylesheet::from_dom(&dom, None, &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, viewport);

    // Load images (file:// only in headless).
    let images = crate::runner::load_images(layout.root(), None, &identity);

    // Paint using the software ImageBackend.
    let mut backend = ImageBackend::new(width, height);
    paint_layout_box(layout.root(), &mut backend, &images, 1.0);

    backend.image
}

/// Render an HTML fixture file to an RGBA image.
pub fn render_fixture_to_image(fixture_html_path: &str, width: u32, height: u32) -> RgbaImage {
    let html = std::fs::read_to_string(fixture_html_path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", fixture_html_path));
    render_to_image(&html, width, height)
}

fn paint_layout_box(
    layout_box: &crate::layout::LayoutBox,
    backend: &mut ImageBackend,
    images: &crate::ImageCache,
    parent_opacity: f32,
) {
    use crate::render::commands::{BorderEdge, Bounds, RenderBackend, Rgba};

    let styles = layout_box.styles();
    let opacity = parent_opacity * styles.opacity();
    if opacity < 0.01 || styles.visibility() == "hidden" {
        return;
    }

    let rect = layout_box.rect();
    let bounds = Bounds::new(rect.x, rect.y, rect.width, rect.height);

    // Paint background.
    if !layout_box.is_viewport() && !layout_box.text().is_some() {
        if let Some(bg) = styles.background_color() {
            if let Some(color) = parse_color_str(bg) {
                backend.fill_rect(bounds, color, opacity);
            }
        }

        // Paint border.
        let bw = styles.border_width();
        let border_edge = BorderEdge {
            top: bw.top,
            right: bw.right,
            bottom: bw.bottom,
            left: bw.left,
        };
        if let Some(bc) = styles.border_color() {
            if let Some(color) = parse_color_str(bc) {
                backend.stroke_rect(bounds, border_edge, color, opacity);
            }
        }
    }

    // Paint image.
    if layout_box.is_image() {
        if let Some(src) = layout_box.image_src() {
            if let Some(img_data) = images.get(src) {
                backend.draw_image(
                    bounds,
                    img_data.data.data(),
                    img_data.width,
                    img_data.height,
                    opacity,
                );
            } else {
                backend.draw_image_placeholder(bounds, opacity);
            }
        }
        for child in layout_box.children() {
            paint_layout_box(child, backend, images, opacity);
        }
        return;
    }

    // Paint text.
    if let Some(text) = layout_box.text() {
        let color_str = styles.get("color").unwrap_or("black");
        let color = parse_color_str(color_str).unwrap_or(Rgba::BLACK);
        let font_size = styles.font_size_px().unwrap_or(16.0);
        backend.draw_text(text, rect.x, rect.y, font_size, color, opacity);
        return;
    }

    // Recurse into children.
    for child in layout_box.children() {
        paint_layout_box(child, backend, images, opacity);
    }
}

fn parse_color_str(s: &str) -> Option<crate::render::commands::Rgba> {
    use crate::render::commands::Rgba;
    let s = s.trim().to_lowercase();
    if s == "transparent" || s == "none" {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            return Some(Rgba::new(
                ((n >> 16) & 0xFF) as u8,
                ((n >> 8) & 0xFF) as u8,
                (n & 0xFF) as u8,
                255,
            ));
        }
        if hex.len() == 3 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let r = ((n >> 8) & 0xF) as u8;
            let g = ((n >> 4) & 0xF) as u8;
            let b = (n & 0xF) as u8;
            return Some(Rgba::new(r * 17, g * 17, b * 17, 255));
        }
    }
    match s.as_str() {
        "black" => Some(Rgba::BLACK),
        "white" => Some(Rgba::WHITE),
        "red" => Some(Rgba::new(255, 0, 0, 255)),
        "green" => Some(Rgba::new(0, 128, 0, 255)),
        "blue" => Some(Rgba::new(0, 0, 255, 255)),
        "gray" | "grey" => Some(Rgba::new(128, 128, 128, 255)),
        "transparent" => None,
        _ => Some(Rgba::new(64, 64, 64, 255)),
    }
}

fn headless_identity() -> Identity {
    Identity::new(
        "did:headless:test",
        "Headless",
        IdentityKind::Agent,
        [Capability::ReadWorkspace],
    )
}
