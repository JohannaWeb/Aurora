use crate::layout::{LayoutBox, LayoutTree, Rect};

use super::elements::{paint_image, paint_input, paint_surface};
use super::{FrameBuffer, CELL_HEIGHT_PX, CELL_WIDTH_PX};

pub struct Painter;

impl Painter {
    pub fn paint(layout_tree: &LayoutTree) -> FrameBuffer {
        let root = layout_tree.root();
        let rect = root.rect();
        let width = (rect.width / CELL_WIDTH_PX).ceil().max(1.0) as usize;
        let height = (rect.height / CELL_HEIGHT_PX).ceil().max(1.0) as usize;
        let mut framebuffer = FrameBuffer::new(width, height);

        paint_box(root, &mut framebuffer);
        framebuffer
    }
}

fn paint_box(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
    if layout_box.styles().opacity() < 0.5 || layout_box.styles().visibility() == "hidden" {
        return;
    }

    if layout_box.is_viewport() {
        framebuffer.fill_rect(layout_box.rect(), '.');
    } else if layout_box.is_image() {
        paint_image(layout_box, framebuffer);
    } else if let Some(text) = layout_box.text() {
        paint_text(layout_box, text, framebuffer);
    } else if let Some(tag_name) = layout_box.tag_name() {
        if tag_name == "input" || tag_name == "button" {
            paint_input(layout_box, tag_name, framebuffer);
        } else {
            paint_surface(layout_box, tag_name, framebuffer);
        }
    }

    for child in layout_box.children() {
        paint_box(child, framebuffer);
    }
}

fn paint_text(layout_box: &LayoutBox, text: &str, framebuffer: &mut FrameBuffer) {
    framebuffer.draw_text(layout_box.rect(), text);

    if layout_box.styles().text_decoration() == Some("underline") {
        let rect = layout_box.rect();
        framebuffer.fill_rect(
            Rect {
                x: rect.x,
                y: rect.y + (rect.height - CELL_HEIGHT_PX).max(0.0),
                width: rect.width,
                height: CELL_HEIGHT_PX,
            },
            '_',
        );
    }
}
