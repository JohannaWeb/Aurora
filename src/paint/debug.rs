use crate::layout::{LayoutBox, LayoutTree, Rect};
use std::fmt::{self, Display, Formatter};

use super::fill::truncate_label;
use super::{FrameBuffer, CELL_HEIGHT_PX, CELL_WIDTH_PX};

#[derive(Debug, Clone)]
struct BoxInfo {
    label: String,
    depth: usize,
    rect: Rect,
}

pub struct DebugFrame {
    framebuffer: FrameBuffer,
    boxes: Vec<BoxInfo>,
}

impl Display for DebugFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.framebuffer)?;
        writeln!(f)?;
        writeln!(f, "Boxes:")?;

        for box_info in &self.boxes {
            let indent = "  ".repeat(box_info.depth);
            writeln!(
                f,
                "{}{:<22} x={:<5} y={:<5} w={:<5} h={}",
                indent,
                box_info.label,
                box_info.rect.x as i32,
                box_info.rect.y as i32,
                box_info.rect.width as i32,
                box_info.rect.height as i32,
            )?;
        }

        Ok(())
    }
}

pub struct DebugPainter;

impl DebugPainter {
    pub fn paint(layout_tree: &LayoutTree) -> DebugFrame {
        let root = layout_tree.root();
        let rect = root.rect();
        let width = (rect.width / CELL_WIDTH_PX).ceil().max(1.0) as usize;
        let height = (rect.height / CELL_HEIGHT_PX).ceil().max(1.0) as usize;
        let mut framebuffer = FrameBuffer::new(width, height);
        let mut boxes = Vec::new();

        debug_box(root, &mut framebuffer, &mut boxes, 0);
        DebugFrame { framebuffer, boxes }
    }
}

fn debug_box(
    layout_box: &LayoutBox,
    framebuffer: &mut FrameBuffer,
    boxes: &mut Vec<BoxInfo>,
    depth: usize,
) {
    let rect = layout_box.rect();
    let (label, corner, horizontal, vertical) = debug_label(layout_box);

    if layout_box.text().is_none() && layout_box.tag_name() != Some("anonymous-inline") {
        framebuffer.draw_outline(rect, corner, horizontal, vertical);
        draw_list_marker(layout_box, framebuffer, rect);
        draw_tag_label(layout_box, framebuffer, rect);
    }

    boxes.push(BoxInfo { label, depth, rect });

    for child in layout_box.children() {
        debug_box(child, framebuffer, boxes, depth + 1);
    }
}

fn debug_label(layout_box: &LayoutBox) -> (String, char, char, char) {
    if layout_box.is_viewport() {
        ("viewport".to_string(), '#', '=', '#')
    } else if layout_box.is_image() {
        let alt = layout_box.image_alt().unwrap_or("img");
        (
            format!("img(\"{}\")", truncate_label(alt, 12)),
            '@',
            '=',
            '!',
        )
    } else if let Some(tag_name) = layout_box.tag_name() {
        (format!("block<{}>", tag_name), '+', '-', '|')
    } else if let Some(text) = layout_box.text() {
        let truncated = truncate_label(text, 12);
        (format!("text(\"{}\")", truncated), '+', '-', '|')
    } else {
        ("unknown".to_string(), '+', '-', '|')
    }
}

fn draw_list_marker(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer, rect: Rect) {
    if layout_box.tag_name() == Some("li") {
        let bullet_x = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let bullet_y = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        if bullet_x > 0 {
            framebuffer.set(bullet_x - 1, bullet_y, '•');
        }
    }
}

fn draw_tag_label(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer, rect: Rect) {
    let cell_width = (rect.width / CELL_WIDTH_PX).ceil() as usize;
    if cell_width < 4 {
        return;
    }

    let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
    let cell_y = (rect.y / CELL_HEIGHT_PX).ceil().max(0.0) as usize;
    let label = if layout_box.is_image() {
        "[img]".to_string()
    } else if let Some(tag_name) = layout_box.tag_name() {
        format!("<{}>", tag_name)
    } else {
        "vp".to_string()
    };
    framebuffer.draw_label_at_cell(cell_x, cell_y, &label);
}
