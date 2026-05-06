use crate::layout::{LayoutBox, Rect};

use super::fill::{background_fill_char, border_fill_char, truncate_label};
use super::{FrameBuffer, CELL_HEIGHT_PX, CELL_WIDTH_PX};

pub(super) fn paint_surface(layout_box: &LayoutBox, tag_name: &str, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    let styles = layout_box.styles();
    let border_width = styles.border_width();
    let background_char =
        background_fill_char(tag_name, styles.background_color(), styles.get("color"));
    let border_char = border_fill_char(tag_name, styles.border_color(), styles.get("color"));

    if background_char != ' ' {
        framebuffer.fill_rect(rect, background_char);
    }

    if border_width.top > 0.0
        || border_width.right > 0.0
        || border_width.bottom > 0.0
        || border_width.left > 0.0
    {
        paint_borders(framebuffer, rect, border_width, border_char);
    }
}

pub(super) fn paint_input(layout_box: &LayoutBox, tag_name: &str, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    let label = if tag_name == "button" {
        layout_box.styles().get("value").unwrap_or("button")
    } else {
        layout_box
            .styles()
            .get("placeholder")
            .or_else(|| layout_box.styles().get("value"))
            .unwrap_or("...")
    };

    let display_label = format!("[ {} ]", truncate_label(label, 16));
    framebuffer.draw_outline(rect, '[', '-', ']');
    let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
    let cell_y = ((rect.y + rect.height / 2.0) / CELL_HEIGHT_PX)
        .floor()
        .max(0.0) as usize;
    framebuffer.draw_label_at_cell(cell_x + 1, cell_y, &display_label);
}

pub(super) fn paint_image(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    framebuffer.fill_rect(rect, 'c');
    framebuffer.draw_outline(rect, '@', '=', '!');

    let label = layout_box
        .image_alt()
        .or_else(|| layout_box.image_src())
        .unwrap_or("image");
    let label = format!("[{}]", truncate_label(label, 14));
    let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
    let cell_y = ((rect.y + rect.height / 2.0) / CELL_HEIGHT_PX)
        .floor()
        .max(0.0) as usize;
    framebuffer.draw_label_at_cell(cell_x, cell_y, &label);
}

fn paint_borders(
    framebuffer: &mut FrameBuffer,
    rect: Rect,
    border_width: crate::css::EdgeSizes,
    border_char: char,
) {
    framebuffer.fill_rect(
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: border_width.top.min(rect.height),
        },
        border_char,
    );
    framebuffer.fill_rect(
        Rect {
            x: rect.x,
            y: rect.y + (rect.height - border_width.bottom).max(0.0),
            width: rect.width,
            height: border_width.bottom.min(rect.height),
        },
        border_char,
    );
    framebuffer.fill_rect(
        Rect {
            x: rect.x,
            y: rect.y,
            width: border_width.left.min(rect.width),
            height: rect.height,
        },
        border_char,
    );
    framebuffer.fill_rect(
        Rect {
            x: rect.x + (rect.width - border_width.right).max(0.0),
            y: rect.y,
            width: border_width.right.min(rect.width),
            height: rect.height,
        },
        border_char,
    );
}
