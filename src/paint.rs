use crate::layout::{LayoutBox, LayoutTree, Rect};
use std::fmt::{self, Display, Formatter};

const CELL_WIDTH_PX: f32 = 16.0;
const CELL_HEIGHT_PX: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer {
    width: usize,
    height: usize,
    cells: Vec<char>,
}

pub struct Painter;

impl Painter {
    pub fn paint(layout_tree: &LayoutTree) -> FrameBuffer {
        let root = layout_tree.root();
        let rect = root.rect();
        let width = (rect.width / CELL_WIDTH_PX).ceil().max(1.0) as usize;
        let height = (rect.height / CELL_HEIGHT_PX).ceil().max(1.0) as usize;
        let mut framebuffer = FrameBuffer::new(width, height);

        let mut items = Vec::new();
        let mut order = 0;
        collect_display_items(root, &mut items, &mut order, 0);

        // Sort by z-index then by tree order
        items.sort_by_key(|&(z, o, _, _)| (z, o));

        for (_, _, _, layout_box) in items {
            paint_single_box(layout_box, &mut framebuffer);
        }

        framebuffer
    }
}

fn collect_display_items<'a>(
    layout_box: &'a LayoutBox,
    items: &mut Vec<(i32, usize, usize, &'a LayoutBox)>,
    order: &mut usize,
    depth: usize,
) {
    if layout_box.styles().opacity() < 0.5 || layout_box.styles().visibility() == "hidden" {
        return;
    }

    *order += 1;
    items.push((layout_box.z_index(), *order, depth, layout_box));
    for child in layout_box.children() {
        collect_display_items(child, items, order, depth + 1);
    }
}

impl FrameBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![' '; width * height],
        }
    }

    fn set(&mut self, x: usize, y: usize, value: char) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = value;
        }
    }

    fn fill_rect(&mut self, rect: Rect, value: char) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        let x1 = ((rect.x + rect.width) / CELL_WIDTH_PX).ceil().max(0.0) as usize;
        let y1 = ((rect.y + rect.height) / CELL_HEIGHT_PX).ceil().max(0.0) as usize;

        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, value);
            }
        }
    }

    fn draw_text(&mut self, rect: Rect, text: &str) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;

        if y0 >= self.height {
            return;
        }

        for (offset, ch) in text.chars().enumerate() {
            let x = x0 + offset;
            if x >= self.width {
                break;
            }
            self.set(x, y0, ch);
        }
    }

    fn draw_outline(&mut self, rect: Rect, corner: char, horizontal: char, vertical: char) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        let x1_f = (rect.x + rect.width) / CELL_WIDTH_PX;
        let y1_f = (rect.y + rect.height) / CELL_HEIGHT_PX;
        let x1 = if x1_f.fract() > 0.0 { x1_f.ceil() as usize } else { x1_f as usize }.saturating_sub(1).max(x0);
        let y1 = if y1_f.fract() > 0.0 { y1_f.ceil() as usize } else { y1_f as usize }.saturating_sub(1).max(y0);

        if x0 >= self.width || y0 >= self.height || x1 < x0 || y1 < y0 {
            return;
        }

        // Top and bottom edges
        for x in x0..=x1.min(self.width - 1) {
            self.set(x, y0, horizontal);
            if y1 < self.height {
                self.set(x, y1, horizontal);
            }
        }

        // Left and right edges
        for y in y0..=y1.min(self.height - 1) {
            self.set(x0, y, vertical);
            if x1 < self.width {
                self.set(x1, y, vertical);
            }
        }

        // Corners
        self.set(x0, y0, corner);
        if x1 < self.width {
            self.set(x1, y0, corner);
        }
        if y1 < self.height {
            self.set(x0, y1, corner);
        }
        if x1 < self.width && y1 < self.height {
            self.set(x1, y1, corner);
        }
    }

    fn draw_label_at_cell(&mut self, cell_x: usize, cell_y: usize, label: &str) {
        if cell_y >= self.height {
            return;
        }

        for (offset, ch) in label.chars().enumerate() {
            let x = cell_x + offset;
            if x >= self.width {
                break;
            }
            self.set(x, cell_y, ch);
        }
    }
}

impl Display for FrameBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for row in self.cells.chunks(self.width) {
            let line = row.iter().collect::<String>();
            writeln!(f, "{}", line.trim_end_matches(' '))?;
        }
        Ok(())
    }
}

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
        // Print the framebuffer
        write!(f, "{}", self.framebuffer)?;

        // Print separator and boxes list
        writeln!(f)?;
        writeln!(f, "Boxes:")?;

        for box_info in &self.boxes {
            let indent = "  ".repeat(box_info.depth);
            write!(
                f,
                "{}{:<22} x={:<5} y={:<5} w={:<5} h={}\n",
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

        let mut items = Vec::new();
        let mut order = 0;
        collect_display_items(root, &mut items, &mut order, 0);

        // Sort by z-index then by tree order for the framebuffer
        items.sort_by_key(|&(z, o, _, _)| (z, o));

        for (_, _, depth, layout_box) in items {
            debug_single_box(layout_box, &mut framebuffer, &mut boxes, depth);
        }

        DebugFrame { framebuffer, boxes }
    }
}

fn debug_single_box(
    layout_box: &LayoutBox,
    framebuffer: &mut FrameBuffer,
    boxes: &mut Vec<BoxInfo>,
    depth: usize,
) {
    let rect = layout_box.rect();

    // Determine label and outline character based on box type
    let (label, corner, horizontal, vertical) = if layout_box.is_viewport() {
        ("viewport".to_string(), '#', '=', '#')
    } else if layout_box.is_image() {
        let alt = layout_box.image_alt().unwrap_or("img");
        (format!("img(\"{}\")", truncate_label(alt, 12)), '@', '=', '!')
    } else if let Some(tag_name) = layout_box.tag_name() {
        let label = format!("block<{}>", tag_name);
        (label, '+', '-', '|')
    } else if let Some(text) = layout_box.text() {
        let truncated = if text.len() > 12 {
            format!("{}...", &text[..9])
        } else {
            text.to_string()
        };
        (format!("text(\"{}\")", truncated), '+', '-', '|')
    } else {
        ("unknown".to_string(), '+', '-', '|')
    };

    // For block/inline, draw outline; for text, skip
    if !layout_box.text().is_some() {
        framebuffer.draw_outline(rect, corner, horizontal, vertical);

        // Try to place tag label inside the box
        let cell_width = ((rect.width) / CELL_WIDTH_PX).ceil() as usize;
        if cell_width >= 4 {
            let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
            let cell_y = (rect.y / CELL_HEIGHT_PX).ceil().max(0.0) as usize;
            let label_short = if layout_box.is_image() {
                "[img]".to_string()
            } else if let Some(tag_name) = layout_box.tag_name() {
                format!("<{}>", tag_name)
            } else {
                "vp".to_string()
            };
            framebuffer.draw_label_at_cell(cell_x, cell_y, &label_short);
        }
    }

    // Add box info
    boxes.push(BoxInfo { label, depth, rect });
}

fn paint_single_box(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
    // Skip painting if opacity is very low
    if layout_box.styles().opacity() < 0.5 {
        return;
    }

    // Skip painting if visibility is hidden
    if layout_box.styles().visibility() == "hidden" {
        return;
    }

    if layout_box.is_viewport() {
        framebuffer.fill_rect(layout_box.rect(), '.');
    } else if layout_box.is_image() {
        paint_image(layout_box, framebuffer);
    } else if let Some(text) = layout_box.text() {
        framebuffer.draw_text(layout_box.rect(), text);

        // Draw underline if text-decoration is set
        if layout_box.styles().text_decoration() == Some("underline") {
            let rect = layout_box.rect();
            let underline_rect = Rect {
                x: rect.x,
                y: rect.y + (rect.height - CELL_HEIGHT_PX).max(0.0),
                width: rect.width,
                height: CELL_HEIGHT_PX,
            };
            framebuffer.fill_rect(underline_rect, '_');
        }
    } else if let Some(tag_name) = layout_box.tag_name() {
        paint_surface(layout_box, tag_name, framebuffer);
    }
}

fn paint_surface(layout_box: &LayoutBox, tag_name: &str, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    let styles = layout_box.styles();
    let border_width = styles.border_width();
    let background_char = background_fill_char(tag_name, styles.background_color(), styles.get("color"));
    let border_char = border_fill_char(tag_name, styles.border_color(), styles.get("color"));

    framebuffer.fill_rect(rect, background_char);

    if border_width.top > 0.0 || border_width.right > 0.0 || border_width.bottom > 0.0 || border_width.left > 0.0 {
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

    let padding_rect = layout_box.padding_rect();
    framebuffer.fill_rect(padding_rect, background_char);
}

fn paint_image(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    framebuffer.fill_rect(rect, 'c');
    framebuffer.draw_outline(rect, '@', '=', '!');

    let label = layout_box
        .image_alt()
        .or_else(|| layout_box.image_src())
        .unwrap_or("image");
    let label = format!("[{}]", truncate_label(label, 14));
    let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
    let cell_y = ((rect.y + rect.height / 2.0) / CELL_HEIGHT_PX).floor().max(0.0) as usize;
    framebuffer.draw_label_at_cell(cell_x, cell_y, &label);
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for ch in value.chars().take(max_chars) {
        result.push(ch);
    }
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

fn box_fill_char(tag_name: &str, color: Option<&str>) -> char {
    if let Some(color) = color {
        return color.chars().next().unwrap_or(tag_name.chars().next().unwrap_or('#'));
    }

    match tag_name {
        "html" => '=',
        "body" => ':',
        "section" => '+',
        "h1" => '#',
        "p" => '-',
        _ => tag_name.chars().next().unwrap_or('?'),
    }
}

fn background_fill_char(tag_name: &str, background_color: Option<&str>, color: Option<&str>) -> char {
    if let Some(background_color) = background_color {
        return background_color
            .chars()
            .next()
            .unwrap_or_else(|| box_fill_char(tag_name, color));
    }

    box_fill_char(tag_name, color)
}

fn border_fill_char(tag_name: &str, border_color: Option<&str>, color: Option<&str>) -> char {
    if let Some(border_color) = border_color {
        return border_color
            .chars()
            .next()
            .map(|ch| ch.to_ascii_uppercase())
            .unwrap_or('*');
    }

    box_fill_char(tag_name, color).to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{Painter, DebugPainter};
    use crate::css::Stylesheet;
    use crate::dom::Node;
    use crate::layout::LayoutTree;
    use crate::style::StyleTree;

    #[test]
    fn paints_text_into_framebuffer() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hello")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; color: blue; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree(&style_tree);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        assert!(rendered.contains("Hello"));
    }

    #[test]
    fn paints_colored_boxes_with_different_fill_chars() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("h1", vec![Node::text("Title")]),
                Node::element("p", vec![Node::text("Body")]),
            ],
        )]);
        let stylesheet = Stylesheet::parse("body { color: cyan; } p { display: inline; color: paper-white; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree(&style_tree);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        assert!(rendered.contains("c"));
        assert!(rendered.contains("p"));
    }

    #[test]
    fn paints_backgrounds_and_borders_as_distinct_layers() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Box")])],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { margin: 8px; padding: 8px; background-color: sand; border: 16px solid ember; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 160.0);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        assert!(rendered.contains("E"));
        assert!(rendered.contains("s"));
        assert!(rendered.contains("Box"));
    }

    #[test]
    fn draws_underline_for_text_decoration() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Link")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; text-decoration: underline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree(&style_tree);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        // Should contain the text and underscores for the underline
        assert!(rendered.contains("Link"));
        assert!(rendered.contains("_"));
    }

    #[test]
    fn skips_box_when_opacity_is_zero() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Invisible")])],
        )]);
        let stylesheet = Stylesheet::parse("section { opacity: 0; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 160.0);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        // The invisible text should not be painted
        assert!(!rendered.contains("Invisible"));
    }

    #[test]
    fn hides_box_when_visibility_is_hidden() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Hidden")])],
        )]);
        let stylesheet = Stylesheet::parse("section { visibility: hidden; background-color: sand; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 160.0);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        // The hidden text should not be painted, but the body (viewport fill) should still be there
        assert!(!rendered.contains("Hidden"));
        // Body background should not be there either since it's inside the hidden section
        // But the viewport (.) should be there
        assert!(rendered.contains("."));
    }

    #[test]
    fn debug_painter_draws_box_outlines() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hello")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree(&style_tree);

        let debug_frame = DebugPainter::paint(&layout);
        let rendered = debug_frame.to_string();

        // Should contain outline characters
        assert!(rendered.contains("+"));
        assert!(rendered.contains("-"));
        assert!(rendered.contains("|"));
    }

    #[test]
    fn debug_painter_lists_all_boxes() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::element("p", vec![Node::text("Text")])])],
        )]);
        let stylesheet = Stylesheet::parse("");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree(&style_tree);

        let debug_frame = DebugPainter::paint(&layout);
        let rendered = debug_frame.to_string();

        // Should contain box names in the coordinate list
        assert!(rendered.contains("viewport"));
        assert!(rendered.contains("block<body>"));
        assert!(rendered.contains("block<section>"));
        assert!(rendered.contains("block<p>"));
        assert!(rendered.contains("Boxes:"));
    }

    #[test]
    fn debug_painter_shows_coordinates() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hi")])],
        )]);
        let stylesheet = Stylesheet::parse("p { width: 120px; height: 48px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);

        let debug_frame = DebugPainter::paint(&layout);
        let rendered = debug_frame.to_string();

        // Should show coordinate labels with x=, y=, w=, h=
        assert!(rendered.contains("x="));
        assert!(rendered.contains("y="));
        assert!(rendered.contains("w="));
        assert!(rendered.contains("h="));
    }

    #[test]
    fn paints_image_placeholders_with_alt_text() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element_with_attributes(
                "img",
                [
                    ("alt".to_string(), "cat loaf".to_string()),
                    ("src".to_string(), "cat.txt".to_string()),
                    ("width".to_string(), "96".to_string()),
                    ("height".to_string(), "48".to_string()),
                ]
                .into_iter()
                .collect(),
                Vec::new(),
            )],
        )]);
        let stylesheet = Stylesheet::parse("img { display: inline; border: 2px solid ember; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        assert!(rendered.contains("@"));
        assert!(rendered.contains("[cat loaf]"));
    }
}
