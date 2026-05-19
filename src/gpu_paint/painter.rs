use crate::layout::LayoutBox;
use crate::{ImageCache, SvgCache};
use peniko::{Color, Fill};
use vello::kurbo::{Affine, Rect as KRect};
use vello::Scene;

use super::color::parse_color;
use super::element::paint_element_with_opacity;
use super::image::paint_image;
use super::scrollbar::paint_scrollbar_if_needed;
use super::svg::render_svg_tree;
use super::text::{paint_text_label, paint_text_with_opacity};

pub struct GpuPainter;

impl GpuPainter {
    pub fn paint(layout_box: &LayoutBox, scene: &mut Scene, images: &ImageCache, svgs: &SvgCache) {
        Self::paint_with_opacity(layout_box, scene, 1.0, images, svgs);
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

    fn paint_with_opacity(
        layout_box: &LayoutBox,
        scene: &mut Scene,
        parent_opacity: f32,
        images: &ImageCache,
        svgs: &SvgCache,
    ) {
        let styles = layout_box.styles();
        let effective_opacity = parent_opacity * styles.opacity();

        if effective_opacity < 0.01 || styles.visibility() == "hidden" {
            return;
        }

        if layout_box.is_viewport() {
            paint_viewport(layout_box, scene);
        } else if layout_box.is_image() {
            // Check SVG cache first, then fall back to raster image cache.
            let r = layout_box.rect();
            let src = layout_box.image_src().unwrap_or("");
            if let Some(tree) = svgs.get(src) {
                render_svg_tree(scene, tree, r.x, r.y, r.width, r.height);
            } else {
                paint_image(layout_box, scene, images);
            }
        } else if layout_box.is_svg_element() {
            paint_inline_svg(layout_box, scene);
        } else if let Some(text) = layout_box.text() {
            paint_text_with_opacity(layout_box, text, scene, effective_opacity);
        } else {
            paint_element_with_opacity(layout_box, scene, effective_opacity);
        }

        for child in layout_box.children() {
            Self::paint_with_opacity(child, scene, effective_opacity, images, svgs);
        }
    }
}

/// Paint an inline `<svg>` element by serialising its DOM subtree and rendering via usvg.
fn paint_inline_svg(layout_box: &LayoutBox, scene: &mut Scene) {
    let Some(node_ptr) = layout_box.node() else {
        return;
    };
    let svg_markup = crate::dom::serialize_svg_node(&node_ptr);
    if let Some(tree) = super::svg::parse_svg(&svg_markup) {
        let r = layout_box.rect();
        render_svg_tree(scene, &tree, r.x, r.y, r.width, r.height);
    }
}

fn paint_viewport(layout_box: &LayoutBox, scene: &mut Scene) {
    let styles = layout_box.styles();
    let rect = layout_box.rect();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        parse_color(styles.background_color().unwrap_or("white")),
        None,
        &KRect::new(
            rect.x as f64,
            rect.y as f64,
            (rect.x + rect.width) as f64,
            (rect.y + rect.height) as f64,
        ),
    );
}
