// Import layout types for rendering
// RUST FUNDAMENTAL: This ASCII/debug painter works with layout output, so it can ignore parsing and CSS details.
use crate::layout::{LayoutBox, LayoutTree, Rect};
// Import Display formatting
use std::fmt::{self, Display, Formatter};

// Character cell dimensions in pixels for ASCII rendering
// RUST FUNDAMENTAL: Converting pixels to coarse character cells is what lets this renderer print a readable terminal approximation.
const CELL_WIDTH_PX: f32 = 6.0;
const CELL_HEIGHT_PX: f32 = 10.0;

// Frame buffer for character-based rendering (text mode)
// RUST FUNDAMENTAL: #[derive(Debug, Clone, PartialEq, Eq)] - derives common traits
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer {
    // Character grid width
    width: usize,
    // Character grid height
    height: usize,
    // Character buffer: width * height elements
    // RUST FUNDAMENTAL: Vec<char> - heap-allocated, owned buffer of Unicode characters
    cells: Vec<char>,
}

// ASCII text renderer using character cell grid
pub struct Painter;

// Painter implementation for text rendering
impl Painter {
    // Paint layout tree to character framebuffer
    pub fn paint(layout_tree: &LayoutTree) -> FrameBuffer {
        // Get root layout box
        let root = layout_tree.root();
        // Get root's position and size
        let rect = root.rect();
        // Calculate grid dimensions from pixels
        let width = (rect.width / CELL_WIDTH_PX).ceil().max(1.0) as usize;
        let height = (rect.height / CELL_HEIGHT_PX).ceil().max(1.0) as usize;
        // RUST FUNDAMENTAL: `.ceil().max(1.0)` ensures even tiny layouts still produce at least a 1x1 framebuffer.
        // Create empty framebuffer
        let mut framebuffer = FrameBuffer::new(width, height);

        // Recursively paint layout boxes to framebuffer
        paint_box(root, &mut framebuffer);

        // Return populated framebuffer
        framebuffer
    }
}

// FrameBuffer implementation with text rendering methods
// RUST FUNDAMENTAL: impl FrameBuffer provides methods for the FrameBuffer struct
// Methods take &self (read-only), &mut self (mutable), or no self (associated functions)
impl FrameBuffer {
    // Create new framebuffer filled with spaces
    // RUST FUNDAMENTAL: fn new() -> Self is constructor pattern; returns instance of Self
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            // RUST FUNDAMENTAL: vec![' '; width * height] - create vec with repeated value
            // Allocates width*height spaces on heap (all initialized to ' ')
            // Similar to: std::vector<char>(width * height, ' ') in C++
            // More efficient than vec![].resize(size, ' ')
            cells: vec![' '; width * height],
        }
    }

    // Set character at grid position
    // RUST FUNDAMENTAL: &mut self - takes mutable borrow to allow modification
    fn set(&mut self, x: usize, y: usize, value: char) {
        // Bounds check to prevent out-of-bounds access
        if x < self.width && y < self.height {
            // RUST FUNDAMENTAL: y * self.width + x converts 2D to 1D index
            // This is row-major indexing: row_index * stride + column_index
            self.cells[y * self.width + x] = value;
        }
    }

    // Fill rectangle region with a character
    fn fill_rect(&mut self, rect: Rect, value: char) {
        // RUST FUNDAMENTAL: `Rect` is passed by value because it is a small copyable geometry type in this codebase.
        // Convert pixel coordinates to cell coordinates
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        // Calculate end coordinates
        let x1 = ((rect.x + rect.width) / CELL_WIDTH_PX).ceil().max(0.0) as usize;
        let y1 = ((rect.y + rect.height) / CELL_HEIGHT_PX).ceil().max(0.0) as usize;

        // Fill all cells in rectangle
        // RUST FUNDAMENTAL: ..range creates iterator; y0..y1 doesn't include y1
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, value);
            }
        }
    }

    // Draw text string starting at rectangle position
    fn draw_text(&mut self, rect: Rect, text: &str) {
        // Convert rectangle x,y to cell coordinates
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;

        // Skip if y position is completely off-screen
        if y0 >= self.height {
            return;
        }
        // RUST FUNDAMENTAL: Early returns keep the main drawing path flat instead of wrapping the whole function in `if`.

        // Draw each character across the row
        // RUST FUNDAMENTAL: .enumerate() provides (index, char) tuples
        for (offset, ch) in text.chars().enumerate() {
            // Calculate x position
            let x = x0 + offset;
            // Stop if we run off the right edge
            if x >= self.width {
                break;
            }
            // Draw character
            self.set(x, y0, ch);
        }
    }

    // Draw box outline with corner and edge characters
    fn draw_outline(&mut self, rect: Rect, corner: char, horizontal: char, vertical: char) {
        // Convert to cell coordinates
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        // Calculate right and bottom edges
        let x1_f = (rect.x + rect.width) / CELL_WIDTH_PX;
        let y1_f = (rect.y + rect.height) / CELL_HEIGHT_PX;
        // Handle fractional coordinates (round up if partial cell)
        let x1 = if x1_f.fract() > 0.0 {
            x1_f.ceil() as usize
        } else {
            x1_f as usize
        }
        .saturating_sub(1)
        .max(x0);
        let y1 = if y1_f.fract() > 0.0 {
            y1_f.ceil() as usize
        } else {
            y1_f as usize
        }
        .saturating_sub(1)
        .max(y0);
        // RUST FUNDAMENTAL: `saturating_sub(1)` avoids underflow if the right or bottom edge would otherwise compute to zero.

        // Skip if box is completely off-screen or invalid
        if x0 >= self.width || y0 >= self.height || x1 < x0 || y1 < y0 {
            return;
        }

        // Draw top and bottom horizontal edges
        // RUST FUNDAMENTAL: ..=range includes both endpoints
        for x in x0..=x1.min(self.width - 1) {
            self.set(x, y0, horizontal);
            if y1 < self.height {
                self.set(x, y1, horizontal);
            }
        }

        // Draw left and right vertical edges
        for y in y0..=y1.min(self.height - 1) {
            self.set(x0, y, vertical);
            if x1 < self.width {
                self.set(x1, y, vertical);
            }
        }

        // Draw corners (overwrite edges)
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

    // Draw label text at specific cell position
    fn draw_label_at_cell(&mut self, cell_x: usize, cell_y: usize, label: &str) {
        // Skip if y is off-screen
        if cell_y >= self.height {
            return;
        }

        // Draw each character
        for (offset, ch) in label.chars().enumerate() {
            // Calculate x position
            let x = cell_x + offset;
            // Stop if x is off-screen
            if x >= self.width {
                break;
            }
            // Draw character
            self.set(x, cell_y, ch);
        }
    }
}

// Trait implementation to print framebuffer as string
impl Display for FrameBuffer {
    // Format framebuffer by joining cells into lines
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // RUST FUNDAMENTAL: .chunks(self.width) creates iterator over rows
        for row in self.cells.chunks(self.width) {
            // RUST FUNDAMENTAL: .collect::<String>() joins chars into string
            let line = row.iter().collect::<String>();
            // Write line, trimming trailing spaces
            writeln!(f, "{}", line.trim_end_matches(' '))?;
            // RUST FUNDAMENTAL: The `?` operator works with `fmt::Result` too, returning early if writing to the formatter fails.
        }
        // Return ok result
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct BoxInfo {
    // RUST FUNDAMENTAL: Small helper structs are a clean way to carry related debug data instead of juggling parallel vectors.
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
            // RUST FUNDAMENTAL: Repeating a string is often simpler than manually building indentation in a loop.
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
        // RUST FUNDAMENTAL: Type inference sees `Vec<BoxInfo>` from the later `debug_box` call, so no explicit generic is needed here.

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
    // RUST FUNDAMENTAL: This function accumulates both a visual outline and a sidecar metadata list for debugging.

    // Determine label and outline character based on box type
    let (label, corner, horizontal, vertical) = if layout_box.is_viewport() {
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
        let label = format!("block<{}>", tag_name);
        (label, '+', '-', '|')
    } else if let Some(text) = layout_box.text() {
        let truncated = if text.chars().count() > 12 {
            text.chars().take(9).collect::<String>() + "..."
        } else {
            text.to_string()
        };
        (format!("text(\"{}\")", truncated), '+', '-', '|')
    } else {
        ("unknown".to_string(), '+', '-', '|')
    };

    // For block/inline, draw outline; for text, skip; skip for anonymous inline boxes
    if !layout_box.text().is_some() && layout_box.tag_name() != Some("anonymous-inline") {
        // RUST FUNDAMENTAL: `!option.is_some()` works, though `option.is_none()` is the more direct equivalent.
        framebuffer.draw_outline(rect, corner, horizontal, vertical);

        if layout_box.tag_name() == Some("li") {
            let bullet_x = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
            let bullet_y = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
            if bullet_x > 0 {
                framebuffer.set(bullet_x - 1, bullet_y, '•');
            }
        }

        // Try to place tag label inside the box (skip for anonymous inline boxes)
        let cell_width = ((rect.width) / CELL_WIDTH_PX).ceil() as usize;
        if cell_width >= 4 {
            let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
            let cell_y = (rect.y / CELL_HEIGHT_PX).ceil().max(0.0) as usize;
            let label_short = if layout_box.is_image() {
                "[img]".to_string()
            } else if let Some(tag_name) = layout_box.tag_name() {
                if tag_name == "anonymous-inline" {
                    // Skip label for anonymous inline boxes
                    String::new()
                } else {
                    format!("<{}>", tag_name)
                }
            } else {
                "vp".to_string()
            };
            if !label_short.is_empty() {
                // RUST FUNDAMENTAL: `String` dereferences to `str`, so `&label_short` can be passed where `&str` is expected.
                framebuffer.draw_label_at_cell(cell_x, cell_y, &label_short);
            }
        }
    }

    // Add box info
    boxes.push(BoxInfo { label, depth, rect });

    // Recurse to children
    for child in layout_box.children() {
        debug_box(child, framebuffer, boxes, depth + 1);
    }
}

fn paint_box(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
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
        // RUST FUNDAMENTAL: The ASCII renderer treats text boxes as terminal characters instead of geometric glyph shapes.
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
        if tag_name == "input" || tag_name == "button" {
            paint_input(layout_box, tag_name, framebuffer);
        } else {
            paint_surface(layout_box, tag_name, framebuffer);
        }
    }

    for child in layout_box.children() {
        // RUST FUNDAMENTAL: Rendering order follows tree order, so later siblings can overwrite earlier cells in the framebuffer.
        paint_box(child, framebuffer);
    }
}

fn paint_surface(layout_box: &LayoutBox, tag_name: &str, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    let styles = layout_box.styles();
    let border_width = styles.border_width();
    let background_char =
        background_fill_char(tag_name, styles.background_color(), styles.get("color"));
    let border_char = border_fill_char(tag_name, styles.border_color(), styles.get("color"));
    // RUST FUNDAMENTAL: Helper functions return chars rather than strings because each terminal cell stores exactly one character.

    if background_char != ' ' {
        framebuffer.fill_rect(rect, background_char);
    }

    if border_width.top > 0.0
        || border_width.right > 0.0
        || border_width.bottom > 0.0
        || border_width.left > 0.0
    {
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
}

fn paint_input(layout_box: &LayoutBox, tag_name: &str, framebuffer: &mut FrameBuffer) {
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
    // RUST FUNDAMENTAL: `or_else(...)` defers the second lookup until the placeholder is missing.

    let display_label = format!("[ {} ]", truncate_label(label, 16));
    // RUST FUNDAMENTAL: `format!` builds an owned `String`, unlike `println!` which writes output directly.
    framebuffer.draw_outline(rect, '[', '-', ']');
    let cell_x = (rect.x / CELL_WIDTH_PX).ceil().max(0.0) as usize;
    let cell_y = ((rect.y + rect.height / 2.0) / CELL_HEIGHT_PX)
        .floor()
        .max(0.0) as usize;
    framebuffer.draw_label_at_cell(cell_x + 1, cell_y, &display_label);
}

fn paint_image(layout_box: &LayoutBox, framebuffer: &mut FrameBuffer) {
    let rect = layout_box.rect();
    // RUST FUNDAMENTAL: This ASCII path uses symbolic placeholder characters because the terminal renderer has no pixel image support.
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

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    // RUST FUNDAMENTAL: Iterating over `.chars()` respects Unicode scalar values instead of slicing bytes directly.
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
        // RUST FUNDAMENTAL: `.next()` returns an `Option<char>` because the string might be empty.
        return color
            .chars()
            .next()
            .unwrap_or(tag_name.chars().next().unwrap_or('#'));
    }

    match tag_name {
        "html" => '=',
        "body" => ':',
        "section" => '+',
        "h1" => '#',
        "p" => '-',
        // Intentional NES-style fallback: use the first character of the tag name (e.g. <div> -> 'd')
        _ => tag_name.chars().next().unwrap_or('?'),
    }
}

fn background_fill_char(
    tag_name: &str,
    background_color: Option<&str>,
    color: Option<&str>,
) -> char {
    if let Some(bg) = background_color {
        let bg_lower = bg.to_lowercase();
        // RUST FUNDAMENTAL: Lowercasing once avoids repeating case-insensitive comparisons against several spellings.
        if bg_lower == "white"
            || bg_lower == "#fff"
            || bg_lower == "#ffffff"
            || bg_lower == "transparent"
        {
            return ' ';
        }
        return bg.chars().next().unwrap_or(' ');
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
        // RUST FUNDAMENTAL: `.map(...)` transforms the `Some` case while leaving `None` untouched.
    }

    box_fill_char(tag_name, color).to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{DebugPainter, Painter};
    use crate::css::Stylesheet;
    use crate::dom::Node;
    use crate::layout::LayoutTree;
    use crate::style::StyleTree;

    fn element(tag: &str, children: Vec<crate::dom::NodePtr>) -> crate::dom::NodePtr {
        crate::dom::Node::element(tag, children)
    }

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
        let stylesheet =
            Stylesheet::parse("body { color: cyan; } p { display: inline; color: paper-white; }");
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
        let stylesheet = Stylesheet::parse(
            "p { display: inline; text-decoration: underline; line-height: 28px; }",
        );
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
        let stylesheet = Stylesheet::parse(
            "section { visibility: hidden; background-color: sand; height: 40px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 160.0);

        let framebuffer = Painter::paint(&layout);
        let rendered = framebuffer.to_string();

        // The hidden text should not be painted, but the body (viewport fill) should still be there
        assert!(!rendered.contains("Hidden"));
        // Body background should be there (denoted by ':' in Aurora)
        assert!(rendered.contains(":"));
    }

    #[test]
    fn debug_painter_draws_box_outlines() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hello")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; padding: 20px; }");
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
            vec![Node::element(
                "section",
                vec![Node::element("p", vec![Node::text("Text")])],
            )],
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
