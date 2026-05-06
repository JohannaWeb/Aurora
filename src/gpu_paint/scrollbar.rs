use crate::layout::LayoutBox;
use peniko::{Color, Fill};
use vello::kurbo::{Affine, Rect as KRect, RoundedRect};
use vello::Scene;

pub(super) fn paint_scrollbar_if_needed(
    layout_box: &LayoutBox,
    scene: &mut Scene,
    viewport_height: f32,
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

    paint_track(scene, track_x, track_width, track_top, track_bottom);
    paint_thumb(scene, track_x, track_width, thumb_top, thumb_bottom);
}

fn paint_track(scene: &mut Scene, track_x: f64, track_width: f64, top: f64, bottom: f64) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(222, 227, 232, 210),
        None,
        &RoundedRect::from_rect(
            KRect::new(track_x, top + 8.0, track_x + track_width, bottom - 8.0),
            5.0,
        ),
    );
}

fn paint_thumb(scene: &mut Scene, track_x: f64, track_width: f64, top: f64, bottom: f64) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(144, 153, 164, 230),
        None,
        &RoundedRect::from_rect(
            KRect::new(track_x + 2.0, top, track_x + track_width - 2.0, bottom),
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
