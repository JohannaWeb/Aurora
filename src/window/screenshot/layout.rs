use super::color::parse_screenshot_color;
use super::primitives::{draw_border, draw_rect};
use super::text::render_text_simple;
use super::ScreenshotImage;
use image::Rgba;

pub(super) fn render_layout_with_text(
    layout: &crate::layout::LayoutTree,
    img: &mut ScreenshotImage,
    offset_x: i32,
    offset_y: i32,
) {
    walk(layout.root(), img, offset_x, offset_y);
}

fn walk(
    box_node: &crate::layout::LayoutBox,
    img: &mut ScreenshotImage,
    offset_x: i32,
    offset_y: i32,
) {
    let rect = box_node.rect();
    let styles = box_node.styles();

    if box_node.text().is_none() && !box_node.is_image() {
        let bg_color = styles
            .get("background-color")
            .or_else(|| styles.get("background"))
            .unwrap_or("transparent");
        if bg_color != "transparent" {
            draw_rect(
                img,
                (rect.x as i32 + offset_x) as u32,
                (rect.y as i32 + offset_y) as u32,
                rect.width as u32,
                rect.height as u32,
                parse_screenshot_color(bg_color),
            );
        }
        let border = styles.border_width();
        if border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0 {
            draw_border(
                img,
                (rect.x as i32 + offset_x) as u32,
                (rect.y as i32 + offset_y) as u32,
                rect.width as u32,
                rect.height as u32,
                parse_screenshot_color(styles.get("border-color").unwrap_or("#dadce0")),
            );
        }
    }

    if box_node.is_image() {
        draw_rect(
            img,
            (rect.x as i32 + offset_x) as u32,
            (rect.y as i32 + offset_y) as u32,
            rect.width as u32,
            rect.height as u32,
            Rgba([220, 235, 250, 255]),
        );
        draw_border(
            img,
            (rect.x as i32 + offset_x) as u32,
            (rect.y as i32 + offset_y) as u32,
            rect.width as u32,
            rect.height as u32,
            Rgba([100, 150, 200, 255]),
        );
    }

    if let Some(text) = box_node.text() {
        render_text_simple(
            img,
            text,
            rect.x as i32 + offset_x,
            rect.y as i32 + offset_y,
            parse_screenshot_color(styles.get("color").unwrap_or("black")),
            styles
                .font_size_px()
                .filter(|&s| s > 0.0)
                .unwrap_or(16.0)
                .max(4.0) as u32,
        );
    }

    for child in box_node.children() {
        walk(child, img, offset_x, offset_y);
    }
}
