use super::primitives::draw_rect;
use super::ScreenshotImage;
use crate::window::scroll_metrics::scroll_content_height;
use image::Rgba;

pub(super) fn render_scrollbars(
    layout_box: &crate::layout::LayoutBox,
    img: &mut ScreenshotImage,
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
    img: &mut ScreenshotImage,
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
        Rgba([222, 227, 232, 210]),
    );
    draw_rect(
        img,
        (track_x + 2.0).round().max(0.0) as u32,
        thumb_top.round().max(0.0) as u32,
        (track_width - 4.0).round() as u32,
        (thumb_bottom - thumb_top).round().max(1.0) as u32,
        Rgba([144, 153, 164, 230]),
    );
}
