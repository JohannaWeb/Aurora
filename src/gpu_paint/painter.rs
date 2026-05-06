use crate::layout::LayoutBox;
use crate::ImageCache;
use peniko::{Color, Fill};
use vello::kurbo::{Affine, Rect as KRect};
use vello::Scene;

use super::color::parse_color;
use super::element::paint_element_with_opacity;
use super::image::paint_image;
use super::scrollbar::paint_scrollbar_if_needed;
use super::text::{paint_text_label, paint_text_with_opacity};

pub struct GpuPainter;

impl GpuPainter {
    pub fn paint(layout_box: &LayoutBox, scene: &mut Scene, images: &ImageCache) {
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

    fn paint_with_opacity(
        layout_box: &LayoutBox,
        scene: &mut Scene,
        parent_opacity: f32,
        images: &ImageCache,
    ) {
        let styles = layout_box.styles();
        let effective_opacity = parent_opacity * styles.opacity();

        if effective_opacity < 0.01 || styles.visibility() == "hidden" {
            return;
        }

        if layout_box.is_viewport() {
            paint_viewport(layout_box, scene);
        } else if layout_box.is_image() {
            paint_image(layout_box, scene, images);
        } else if let Some(text) = layout_box.text() {
            paint_text_with_opacity(layout_box, text, scene, effective_opacity);
        } else {
            paint_element_with_opacity(layout_box, scene, effective_opacity);
        }

        for child in layout_box.children() {
            Self::paint_with_opacity(child, scene, effective_opacity, images);
        }
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
