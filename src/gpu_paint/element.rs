use crate::layout::LayoutBox;
use peniko::Fill;
use vello::kurbo::{Affine, Rect as KRect, RoundedRect};
use vello::Scene;

use super::color::parse_color;

pub(super) fn paint_element_with_opacity(layout_box: &LayoutBox, scene: &mut Scene, opacity: f32) {
    let r = layout_box.rect();
    let styles = layout_box.styles();
    let bg_color_name = styles
        .get("background-color")
        .or_else(|| styles.get("background"))
        .unwrap_or("transparent");
    let mut bg_color = parse_color(bg_color_name);
    let mut border_color = parse_color(styles.get("border-color").unwrap_or("black"));
    let border = layout_box.styles().border_width();

    bg_color.components[3] *= opacity;
    border_color.components[3] *= opacity;

    let radius = styles
        .get("border-radius")
        .and_then(|radius| radius.trim_end_matches("px").parse::<f32>().ok())
        .unwrap_or(0.0) as f64;

    let rounded_rect = RoundedRect::from_rect(
        KRect::new(
            r.x as f64,
            r.y as f64,
            (r.x + r.width) as f64,
            (r.y + r.height) as f64,
        ),
        radius,
    );

    paint_shadow_if_needed(layout_box, scene, opacity, radius);

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
        scene.stroke(
            &vello::kurbo::Stroke::new(border.top as f64),
            Affine::IDENTITY,
            border_color,
            None,
            &rounded_rect,
        );
    }
}

fn paint_shadow_if_needed(layout_box: &LayoutBox, scene: &mut Scene, opacity: f32, radius: f64) {
    let styles = layout_box.styles();
    if styles
        .get("box-shadow")
        .filter(|shadow| *shadow != "none")
        .is_none()
    {
        return;
    }

    let r = layout_box.rect();
    let shadow_color = peniko::Color::from_rgba8(0, 0, 0, ((60.0 * opacity) as u8).min(255));
    let shadow_rect = KRect::new(
        (r.x + 3.0) as f64,
        (r.y + 3.0) as f64,
        (r.x + r.width + 3.0) as f64,
        (r.y + r.height + 3.0) as f64,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        shadow_color,
        None,
        &RoundedRect::from_rect(shadow_rect, radius.max(2.0)),
    );
}
