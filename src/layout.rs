// Import CSS layout properties
// RUST FUNDAMENTAL: Large modules often import many related domain types at once.
// Grouping them in one `use` keeps signatures shorter and makes it obvious which concepts this file works with.
use crate::css::{
    AlignItems, BoxSizing, DisplayMode, EdgeSizes, FlexDirection, JustifyContent, Margin,
    MarginValue, StyleMap, TextAlign,
};
// Import styled DOM tree
use crate::style::{StyleTree, StyledNode};
// Import Display formatting
use std::fmt::{self, Display, Formatter};

// Default viewport width for layout (unused, kept for reference)
#[allow(dead_code)]
const DEFAULT_VIEWPORT_WIDTH: f32 = 1200.0;
// Vertical padding for block elements in layout
const BLOCK_VERTICAL_PADDING: f32 = 6.0;
// Average character width in pixels
const TEXT_CHAR_WIDTH: f32 = 8.0;
// Line height for text rendering
const TEXT_LINE_HEIGHT: f32 = 18.0;
// Height of inline elements
const INLINE_BOX_HEIGHT: f32 = 16.0;

// Complete layout tree with positioned boxes
// RUST FUNDAMENTAL: A dedicated wrapper type around the root box makes ownership and API boundaries clearer
// than passing around a bare `LayoutBox` everywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTree {
    // Root box of the layout tree (viewport)
    root: LayoutBox,
}

// Single layout box with position, size, and styling
// RUST FUNDAMENTAL: This struct is the core "computed layout" record.
// Each box owns its own rectangle, style snapshot, and child boxes, so the whole layout tree is self-contained.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    // Type of layout box (block, inline, image, etc.)
    kind: LayoutKind,
    // Rectangle with position and dimensions
    rect: Rect,
    // CSS styles applied to this box
    styles: StyleMap,
    // Margin around the box
    margin: Margin,
    // Border width for each side
    border: EdgeSizes,
    // Padding inside the box
    padding: EdgeSizes,
    // Child layout boxes
    children: Vec<LayoutBox>,
}

// Enumeration of layout box types
// RUST FUNDAMENTAL: Enums are a good fit when one conceptual thing can take several shapes with different associated data.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LayoutKind {
    // Viewport (main rendering surface)
    Viewport,
    // Block-level element (takes full width)
    Block {
        tag_name: String,
    },
    // Inline-block element (inline but acts as block)
    InlineBlock {
        tag_name: String,
    },
    // Inline element (flows with text)
    Inline {
        tag_name: String,
    },
    // Control element (input, button, etc.)
    Control {
        tag_name: String,
    },
    // Image element with alt text and src
    Image {
        alt: Option<String>,
        src: Option<String>,
        display_mode: DisplayMode,
    },
    // Text node containing string content
    Text {
        text: String,
    },
}

// Rectangle representing a box's position and dimensions
// RUST FUNDAMENTAL: #[derive(Debug, Clone, Copy, PartialEq)]
// Copy trait = automatically copy on assignment (bitwise copy); only for small stack types
// f32 is Copy; after let r2 = r1, both r1 and r2 are valid (not moved)
// Without Copy: let r2 = r1 would move r1, leaving it inaccessible
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    // X coordinate (pixels from left)
    // RUST FUNDAMENTAL: f32 is 32-bit floating point; represents coordinates with decimal precision
    pub x: f32,

    // Y coordinate (pixels from top)
    pub y: f32,

    // Width in pixels
    pub width: f32,

    // Height in pixels
    pub height: f32,
}

// Implementation of LayoutTree
impl LayoutTree {
    // Create layout tree from styled tree using default viewport width
    #[allow(dead_code)]
    pub fn from_style_tree(style_tree: &StyleTree) -> Self {
        // Delegate to method with explicit viewport width
        Self::from_style_tree_with_viewport_width(style_tree, DEFAULT_VIEWPORT_WIDTH)
    }

    // Create layout tree from styled tree with specified viewport width
    pub fn from_style_tree_with_viewport_width(
        // Styled DOM tree to layout
        style_tree: &StyleTree,
        // Width of viewport in pixels
        viewport_width: f32,
    ) -> Self {
        // Build layout box tree starting from styled root
        // RUST FUNDAMENTAL: `.expect(...)` is reasonable when failing here would indicate a logic bug rather than a normal runtime condition.
        let root = LayoutBox::layout_root(style_tree.root(), viewport_width)
            // Panic if root fails (shouldn't happen)
            .expect("style tree root must produce a viewport");
        // Wrap root in LayoutTree
        Self { root }
    }

    // Get root layout box of the tree
    pub fn root(&self) -> &LayoutBox {
        // Return reference to root box
        &self.root
    }
}

// LayoutBox implementation with layout calculation methods
impl LayoutBox {
    // Create root viewport box with specified width
    fn layout_root(node: &StyledNode, viewport_width: f32) -> Option<Self> {
        // Build layout starting from position (0,0) with full width
        // RUST FUNDAMENTAL: The `?` operator keeps this constructor concise by propagating `None`
        // if the styled root cannot produce a layout box.
        let mut root = Self::from_styled_node(node, 0.0, 0.0, viewport_width)?;
        // Set viewport width to fill available space
        root.rect.width = viewport_width;
        // Return root box
        Some(root)
    }

    // Layout a styled node recursively based on its type
    fn from_styled_node(node: &StyledNode, x: f32, y: f32, available_width: f32) -> Option<Self> {
        // Skip style and script tags (non-visual)
        // RUST FUNDAMENTAL: Comparing against `Some("...".to_string())` works because `Option<String>` implements `PartialEq`,
        // though it does allocate temporary strings each time.
        if node.tag_name() == Some("style".to_string())
            || node.tag_name() == Some("script".to_string())
        {
            return None;
        }

        // Dispatch based on node type
        // RUST FUNDAMENTAL: Match guards like `None if node.text().is_none()` let one pattern branch on extra conditions.
        match node.tag_name() {
            // Document node without text becomes viewport
            None if node.text().is_none() => Some(Self::layout_container(
                LayoutKind::Viewport,
                node.styles().clone(),
                Margin::zero(),
                EdgeSizes::zero(),
                EdgeSizes::zero(),
                node.children(),
                x,
                y,
                available_width,
            )),
            // Element node - dispatch to element handler
            Some(tag_name) => Self::from_element(&tag_name, node, x, y, available_width),
            // Text node - layout as text
            // RUST FUNDAMENTAL: `unwrap_or_default()` is a convenient fallback when the default value for the type is acceptable.
            None => Some(Self::layout_text(
                &node.text().unwrap_or_default(),
                node.styles().clone(),
                x,
                y,
            )),
        }
    }

    // Layout an element node based on display mode and tag name
    fn from_element(
        // HTML tag name
        tag_name: &str,
        // Styled node to layout
        node: &StyledNode,
        // X position
        x: f32,
        // Y position
        y: f32,
        // Available width for layout
        available_width: f32,
    ) -> Option<Self> {
        // Get display mode from styles
        // RUST FUNDAMENTAL: Cloning the `StyleMap` here means each layout box owns the style snapshot it was laid out with.
        let styles = node.styles().clone();
        // Dispatch based on display mode and tag type
        // RUST FUNDAMENTAL: Pattern guards on match arms are useful when dispatch depends on both an enum variant
        // and additional runtime data like the HTML tag name.
        match styles.display_mode() {
            // Display: none means don't render
            DisplayMode::None => None,
            // Image-like elements (img, svg, canvas, iframe)
            mode if tag_name == "img"
                || tag_name == "svg"
                || tag_name == "canvas"
                || tag_name == "iframe" =>
            {
                Some(Self::layout_image(
                    node,
                    styles,
                    node.styles().margin(),
                    node.styles().border_width(),
                    node.styles().padding(),
                    x,
                    y,
                    available_width,
                    mode,
                ))
            }
            // Form control elements (textarea, input, button)
            _ if tag_name == "textarea" || tag_name == "input" || tag_name == "button" => {
                Some(Self::layout_control(
                    tag_name,
                    node,
                    styles,
                    node.styles().margin(),
                    node.styles().border_width(),
                    node.styles().padding(),
                    x,
                    y,
                    available_width,
                ))
            }
            // Display: block (full width, new line)
            DisplayMode::Block => Some(Self::layout_container(
                LayoutKind::Block {
                    tag_name: tag_name.to_string(),
                },
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
            // Display: inline-block (inline positioning, block box)
            DisplayMode::InlineBlock => Some(Self::layout_container(
                LayoutKind::InlineBlock {
                    tag_name: tag_name.to_string(),
                },
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
            // Display: flex (flexbox layout)
            DisplayMode::Flex => Some(Self::layout_flex_container(
                LayoutKind::Block {
                    tag_name: tag_name.to_string(),
                },
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
            // Display: inline (flows with text)
            DisplayMode::Inline => Some(Self::layout_inline(
                tag_name,
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
        }
    }

    fn layout_flex_container(
        kind: LayoutKind,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        // RUST FUNDAMENTAL: Intermediate locals like these keep complex numeric code understandable and debuggable.
        let mut rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width =
            (available_rect_width - padding.horizontal() - border.horizontal()).max(0.0);
        let content_width =
            clamp_content_width(&styles, default_content_width, default_content_width);

        if let (MarginValue::Auto, MarginValue::Auto) = (margin.left, margin.right) {
            // RUST FUNDAMENTAL: Destructuring two values into one tuple pattern is a neat way to check multiple cases at once.
            let total_box_width = content_width + padding.horizontal() + border.horizontal();
            let free_space = (available_width - total_box_width).max(0.0);
            rect_x = x + free_space / 2.0;
        }

        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;

        let direction = styles.flex_direction();
        let justify = styles.justify_content();
        let align = styles.align_items();
        let gap = styles.gap_px();
        let wraps = styles.flex_wrap();

        let mut layout_children = Vec::new();

        // Pass 1: Parse children sizes
        // RUST FUNDAMENTAL: Multi-pass algorithms are common in layout engines because some final values
        // depend on measurements gathered earlier in the same container.
        let mut total_child_width: f32 = 0.0;
        let mut total_child_height: f32 = 0.0;
        let mut max_child_height: f32 = 0.0;
        let mut max_child_width: f32 = 0.0;

        for child in children {
            // RUST FUNDAMENTAL: Early `continue` keeps loop bodies flatter by skipping non-participating children immediately.
            if child.tag_name() == Some("style".to_string())
                || child.tag_name() == Some("script".to_string())
                || child.styles().display_mode() == DisplayMode::None
            {
                continue;
            }

            // Flex item sizing per CSS Flex spec:
            // - If child has explicit width/max-width, measure with that constraint
            // - Otherwise, do two-pass measurement: measure to get intrinsic width, then re-layout at that width
            let has_explicit_width =
                child.styles().width_px().is_some() || child.styles().max_width_px().is_some();

            if !has_explicit_width {
                // First pass: measure at large width to see what children need
                if let Some(measured) = Self::from_styled_node(child, 0.0, 0.0, 10000.0) {
                    // Compute intrinsic width from measured layout
                    // RUST FUNDAMENTAL: `.fold(initial, f32::max)` is a common reduction pattern for finding a maximum value.
                    let intrinsic = if measured.children.is_empty() {
                        measured.rect.width
                            + measured.padding.horizontal()
                            + measured.border.horizontal()
                    } else {
                        let child_max = measured
                            .children
                            .iter()
                            .map(|c| c.total_width())
                            .fold(0.0_f32, f32::max);
                        child_max + measured.padding.horizontal() + measured.border.horizontal()
                    };

                    // Cap to container width
                    let final_width = intrinsic.min(content_width);

                    // Second pass: re-layout at the correct width
                    // RUST FUNDAMENTAL: Re-running a pure-ish calculation with better constraints is often simpler
                    // than trying to mutate the first result into shape.
                    if let Some(layout_child) = Self::from_styled_node(child, 0.0, 0.0, final_width)
                    {
                        let child_w = layout_child.total_width();
                        total_child_width += child_w;
                        total_child_height += layout_child.total_height();
                        max_child_height = max_child_height.max(layout_child.total_height());
                        max_child_width = max_child_width.max(child_w);
                        layout_children.push(layout_child);
                    }
                }
            } else {
                // Has explicit width, measure normally at container width
                if let Some(layout_child) = Self::from_styled_node(child, 0.0, 0.0, content_width) {
                    let child_w = layout_child.total_width();
                    total_child_width += child_w;
                    total_child_height += layout_child.total_height();
                    max_child_height = max_child_height.max(layout_child.total_height());
                    max_child_width = max_child_width.max(child_w);
                    layout_children.push(layout_child);
                }
            }
        }

        let item_count = layout_children.len() as f32;
        // RUST FUNDAMENTAL: Casting with `as` is explicit in Rust, which makes numeric conversions visible to readers.
        if direction == FlexDirection::Row && item_count > 1.0 && !wraps {
            total_child_width += gap * (item_count - 1.0);
        }
        if direction == FlexDirection::Column && item_count > 1.0 {
            total_child_height += gap * (item_count - 1.0);
        }

        let inner_height = if direction == FlexDirection::Row && wraps {
            0.0
        } else if direction == FlexDirection::Row {
            max_child_height
        } else {
            total_child_height
        };
        let mut resolved_content_height =
            clamp_content_height(&styles, inner_height).max(BLOCK_VERTICAL_PADDING);

        // Pass 2: Position items
        if direction == FlexDirection::Row && wraps {
            let mut rows: Vec<Vec<usize>> = Vec::new();
            let mut current_row: Vec<usize> = Vec::new();
            let mut current_row_width = 0.0;

            for (index, child) in layout_children.iter().enumerate() {
                let child_width = child.total_width();
                // RUST FUNDAMENTAL: Temporary proposal variables like this make greedy packing algorithms easier to follow.
                let proposed = if current_row.is_empty() {
                    child_width
                } else {
                    current_row_width + gap + child_width
                };

                if !current_row.is_empty() && proposed > content_width {
                    rows.push(current_row);
                    current_row = vec![index];
                    current_row_width = child_width;
                } else {
                    current_row_width = proposed;
                    current_row.push(index);
                }
            }

            if !current_row.is_empty() {
                rows.push(current_row);
            }

            let mut row_heights = Vec::new();
            let mut total_rows_height = 0.0;
            for row in &rows {
                let row_height = row
                    .iter()
                    .map(|index| layout_children[*index].total_height())
                    .fold(0.0_f32, f32::max);
                row_heights.push(row_height);
                total_rows_height += row_height;
            }
            if rows.len() > 1 {
                total_rows_height += gap * (rows.len() as f32 - 1.0);
            }
            resolved_content_height =
                clamp_content_height(&styles, total_rows_height).max(BLOCK_VERTICAL_PADDING);

            let mut current_y = content_y;
            for (row_index, row) in rows.iter().enumerate() {
                let row_width: f32 = row
                    .iter()
                    .enumerate()
                    .map(|(i, index)| {
                        layout_children[*index].total_width() + if i > 0 { gap } else { 0.0 }
                    })
                    .sum();
                let free_width = (content_width - row_width).max(0.0);
                // RUST FUNDAMENTAL: Matching into a tuple is a clean way to compute two related outputs at once.
                let (mut current_x, spacing) = match justify {
                    JustifyContent::FlexEnd => (content_x + free_width, gap),
                    JustifyContent::Center => (content_x + free_width / 2.0, gap),
                    JustifyContent::SpaceBetween => {
                        let sp = if row.len() > 1 {
                            free_width / (row.len() as f32 - 1.0)
                        } else {
                            0.0
                        };
                        (content_x, sp)
                    }
                    JustifyContent::SpaceAround => {
                        let sp = if !row.is_empty() {
                            free_width / row.len() as f32
                        } else {
                            0.0
                        };
                        (content_x + sp / 2.0, sp)
                    }
                    _ => (content_x, gap),
                };

                for index in row {
                    // RUST FUNDAMENTAL: Taking `&mut layout_children[*index]` gives exclusive access to the chosen child box
                    // so its position can be updated in place.
                    let child = &mut layout_children[*index];
                    let new_x = current_x + child.margin.left.to_px();
                    let new_y = match align {
                        AlignItems::Center => {
                            let free_y = (row_heights[row_index] - child.total_height()).max(0.0);
                            current_y + free_y / 2.0 + child.margin.top
                        }
                        AlignItems::FlexEnd => {
                            let free_y = (row_heights[row_index] - child.total_height()).max(0.0);
                            current_y + free_y + child.margin.top
                        }
                        _ => current_y + child.margin.top,
                    };

                    let dx = new_x - child.rect.x;
                    let dy = new_y - child.rect.y;
                    // RUST FUNDAMENTAL: Representing movement as deltas keeps the offset logic independent of current absolute position.
                    child.offset(dx, dy);
                    current_x += child.total_width() + spacing;
                }

                current_y += row_heights[row_index] + gap;
            }
        } else if direction == FlexDirection::Row {
            let free_width = (content_width - total_child_width).max(0.0);
            let (mut current_x, spacing) = match justify {
                JustifyContent::FlexEnd => (content_x + free_width, gap),
                JustifyContent::Center => (content_x + free_width / 2.0, gap),
                JustifyContent::SpaceBetween => {
                    let sp = if layout_children.len() > 1 {
                        free_width / (layout_children.len() as f32 - 1.0)
                    } else {
                        0.0
                    };
                    (content_x, sp)
                }
                JustifyContent::SpaceAround => {
                    let sp = if !layout_children.is_empty() {
                        free_width / layout_children.len() as f32
                    } else {
                        0.0
                    };
                    (content_x + sp / 2.0, sp)
                }
                _ => (content_x, gap),
            };

            for child in &mut layout_children {
                let new_x = current_x + child.margin.left.to_px();
                let new_y = match align {
                    AlignItems::Center => {
                        let free_y = (resolved_content_height - child.total_height()).max(0.0);
                        content_y + free_y / 2.0 + child.margin.top
                    }
                    AlignItems::FlexEnd => {
                        let free_y = (resolved_content_height - child.total_height()).max(0.0);
                        content_y + free_y + child.margin.top
                    }
                    _ => content_y + child.margin.top,
                };

                let dx = new_x - child.rect.x;
                let dy = new_y - child.rect.y;
                child.offset(dx, dy);

                current_x += child.total_width() + spacing;
            }
        } else {
            // RUST FUNDAMENTAL: This final branch handles the column-direction case after the row-specific paths above.
            let free_height = (resolved_content_height - total_child_height).max(0.0);
            let (mut current_y, spacing) = match justify {
                JustifyContent::FlexEnd => (content_y + free_height, gap),
                JustifyContent::Center => (content_y + free_height / 2.0, gap),
                JustifyContent::SpaceBetween => {
                    let sp = if layout_children.len() > 1 {
                        free_height / (layout_children.len() as f32 - 1.0)
                    } else {
                        0.0
                    };
                    (content_y, sp)
                }
                JustifyContent::SpaceAround => {
                    let sp = if !layout_children.is_empty() {
                        free_height / layout_children.len() as f32
                    } else {
                        0.0
                    };
                    (content_y + sp / 2.0, sp)
                }
                _ => (content_y, gap),
            };

            for child in &mut layout_children {
                let new_y = current_y + child.margin.top;
                let new_x = match align {
                    AlignItems::Center => {
                        let free_w = (content_width - child.total_width()).max(0.0);
                        content_x + free_w / 2.0 + child.margin.left.to_px()
                    }
                    AlignItems::FlexEnd => {
                        let free_w = (content_width - child.total_width()).max(0.0);
                        content_x + free_w + child.margin.left.to_px()
                    }
                    _ => content_x + child.margin.left.to_px(),
                };

                let dx = new_x - child.rect.x;
                let dy = new_y - child.rect.y;
                child.offset(dx, dy);

                current_y += child.total_height() + spacing;
            }
        }

        Self {
            kind,
            rect: Rect {
                x: rect_x,
                y: rect_y,
                // RUST FUNDAMENTAL: `.min(...)` and `.max(...)` are common guardrails in layout math to keep derived dimensions sensible.
                width: (content_width + padding.horizontal() + border.horizontal())
                    .min(available_rect_width),
                height: border.top
                    + padding.top
                    + resolved_content_height
                    + padding.bottom
                    + border.bottom,
            },
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_image(
        node: &StyledNode,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        x: f32,
        y: f32,
        available_width: f32,
        display_mode: DisplayMode,
    ) -> Self {
        let rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        // RUST FUNDAMENTAL: Chaining `attribute(...).as_deref().and_then(...)` converts an owned `Option<String>`
        // into a borrowed optional string slice and then parses it if present.
        let width_hint = node
            .attribute("width")
            .as_deref()
            .and_then(parse_html_length_px)
            .unwrap_or(120.0);
        let height_hint = node
            .attribute("height")
            .as_deref()
            .and_then(parse_html_length_px)
            .unwrap_or(40.0);
        let content_width = clamp_content_width(&styles, width_hint, available_rect_width);
        let content_height = clamp_content_height(&styles, height_hint);

        Self {
            kind: LayoutKind::Image {
                src: node.attribute("src").map(|s| s.to_string()),
                alt: node.attribute("alt").map(|s| s.to_string()),
                display_mode,
            },
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: (content_width + padding.horizontal() + border.horizontal())
                    .min(available_rect_width),
                height: content_height + padding.vertical() + border.vertical(),
            },
            styles,
            margin,
            border,
            padding,
            children: Vec::new(),
        }
    }

    fn layout_control(
        tag_name: &str,
        node: &StyledNode,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let mut rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        // RUST FUNDAMENTAL: Helper functions like `control_label(...)` and `measure_text_width(...)`
        // keep this constructor focused on layout policy instead of low-level string or text-measurement details.
        let label = control_label(tag_name, node);
        let text_styles = styles.clone();
        let label_width = measure_text_width(&label, &text_styles);
        let default_content_width = match tag_name {
            "input" => label_width.max(180.0),
            "textarea" => label_width.max(220.0),
            _ => label_width.max(72.0),
        };
        let default_content_height = match tag_name {
            "textarea" => line_height_from_styles(&text_styles) * 3.0,
            _ => line_height_from_styles(&text_styles),
        };
        let content_width =
            clamp_content_width(&styles, default_content_width, available_rect_width);
        let content_height = clamp_content_height(&styles, default_content_height);

        if let (MarginValue::Auto, MarginValue::Auto) = (margin.left, margin.right) {
            // RUST FUNDAMENTAL: This repeats the same centering rule used for blocks:
            // when both horizontal margins are `auto`, the remaining space is split evenly.
            let total_box_width = content_width + padding.horizontal() + border.horizontal();
            let free_space = (available_width - total_box_width).max(0.0);
            rect_x = x + free_space / 2.0;
        }

        let rect = Rect {
            x: rect_x,
            y: rect_y,
            width: (content_width + padding.horizontal() + border.horizontal())
                .min(available_rect_width),
            height: content_height + padding.vertical() + border.vertical(),
        };

        let mut children = Vec::new();
        if !label.is_empty() {
            let text_width = label_width.min(content_width.max(0.0));
            let text_height = line_height_from_styles(&text_styles);
            let content_x = rect.x + border.left + padding.left;
            let content_y = rect.y + border.top + padding.top;
            // RUST FUNDAMENTAL: Branching on `tag_name` here is layout policy, not parsing.
            // The same DOM node kind can render differently depending on control semantics.
            let text_x = if tag_name == "button" {
                content_x + ((content_width - text_width).max(0.0) / 2.0)
            } else {
                content_x
            };
            let text_y = content_y + ((content_height - text_height).max(0.0) / 2.0);

            children.push(Self {
                kind: LayoutKind::Text { text: label },
                rect: Rect {
                    x: text_x,
                    y: text_y,
                    width: text_width,
                    height: text_height,
                },
                styles: text_styles,
                margin: Margin::zero(),
                border: EdgeSizes::zero(),
                padding: EdgeSizes::zero(),
                children: Vec::new(),
            });
        }

        Self {
            kind: LayoutKind::Control {
                tag_name: tag_name.to_string(),
            },
            rect,
            styles,
            margin,
            border,
            padding,
            children,
        }
    }

    fn layout_container(
        kind: LayoutKind,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let mut rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width =
            (available_rect_width - padding.horizontal() - border.horizontal()).max(0.0);
        let content_width =
            clamp_content_width(&styles, default_content_width, default_content_width);

        // Handle margin: auto for block centering
        if let (MarginValue::Auto, MarginValue::Auto) = (margin.left, margin.right) {
            let total_box_width = content_width + padding.horizontal() + border.horizontal();
            let free_space = (available_width - total_box_width).max(0.0);
            rect_x = x + free_space / 2.0;
        }

        let rect_width =
            (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width);
        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;
        let mut layout_children = Vec::new();
        let mut cursor_y = content_y;
        let mut previous_block_bottom_margin: f32 = 0.0;
        let mut previous_was_block = false;
        let mut inline_group: Vec<&StyledNode> = Vec::new();

        // RUST FUNDAMENTAL: Closures are anonymous functions that can be stored in local variables and called later.
        // This one packages the "flush current inline run" behavior so the same logic can be reused in two places.
        let flush_inline_group = |inline_group: &mut Vec<&StyledNode>,
                                  layout_children: &mut Vec<LayoutBox>,
                                  cursor_y: &mut f32,
                                  content_x: f32,
                                  _content_y: f32,
                                  content_width: f32| {
            if inline_group.is_empty() {
                return;
            }

            if let Some(anon_inline) =
                Self::layout_inline_sequence(inline_group, content_x, *cursor_y, content_width)
            {
                *cursor_y += anon_inline.total_height();
                layout_children.push(anon_inline);
            }
            inline_group.clear();
        };

        for child in children {
            let child_is_block = child
                .tag_name()
                .map(|_| child.styles().display_mode() == DisplayMode::Block)
                .unwrap_or(false);
            // RUST FUNDAMENTAL: `map(...).unwrap_or(false)` is a compact way to say
            // "compute something if the optional tag exists, otherwise use false".

            if child_is_block {
                flush_inline_group(
                    &mut inline_group,
                    &mut layout_children,
                    &mut cursor_y,
                    content_x,
                    content_y,
                    content_width,
                );

                let child_margin = child.styles().margin();
                // RUST FUNDAMENTAL: Margin collapsing is modeled as numeric overlap subtraction here.
                // The engine computes how much vertical space two adjacent margins share.
                let collapse_overlap = if previous_was_block {
                    previous_block_bottom_margin.min(child_margin.top)
                } else {
                    0.0
                };

                if let Some(mut layout_child) = Self::from_styled_node(
                    child,
                    content_x,
                    cursor_y - collapse_overlap,
                    content_width,
                ) {
                    let alignment = styles.text_align();
                    if alignment != TextAlign::Left {
                        let child_width = layout_child.total_width();
                        // RUST FUNDAMENTAL: Match expressions are useful even for tiny arithmetic policy choices.
                        let offset = match alignment {
                            TextAlign::Center => (content_width - child_width) / 2.0,
                            TextAlign::Right => content_width - child_width,
                            TextAlign::Left => 0.0,
                        };
                        if offset > 0.0 {
                            layout_child.offset(offset, 0.0);
                        }
                    }

                    cursor_y += layout_child.total_height();
                    previous_block_bottom_margin = layout_child.margin.bottom;
                    previous_was_block = true;
                    layout_children.push(layout_child);
                }
            } else {
                inline_group.push(child);
                previous_was_block = false;
            }
        }

        flush_inline_group(
            &mut inline_group,
            &mut layout_children,
            &mut cursor_y,
            content_x,
            content_y,
            content_width,
        );

        let content_height = cursor_y - content_y;
        let inner_height = if layout_children.is_empty() {
            BLOCK_VERTICAL_PADDING
        } else {
            content_height + BLOCK_VERTICAL_PADDING
        };
        let resolved_content_height = clamp_content_height(&styles, inner_height);

        Self {
            kind,
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: rect_width,
                height: border.top
                    + padding.top
                    + resolved_content_height
                    + padding.bottom
                    + border.bottom,
            },
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_inline(
        tag_name: &str,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width =
            (available_rect_width - padding.horizontal() - border.horizontal())
                .max(TEXT_CHAR_WIDTH);
        let content_width =
            clamp_content_width(&styles, default_content_width, default_content_width);
        let max_rect_width =
            (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width);
        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;

        let mut layout_children = Vec::new();
        let mut line_x = content_x;
        let mut line_y = content_y;
        let mut line_height: f32 = 0.0;
        let mut max_line_width: f32 = 0.0;

        for child in children {
            if let Some(text) = child.text() {
                // RUST FUNDAMENTAL: Specialized helper functions keep the main loop focused on control flow
                // while delegating text wrapping details to a separate routine.
                let fragments = Self::layout_text_fragments(
                    &text,
                    child.styles().clone(),
                    content_x,
                    content_width,
                    &mut line_x,
                    &mut line_y,
                    &mut line_height,
                    &mut max_line_width,
                );
                layout_children.extend(fragments);
                continue;
            }

            let remaining_width = (content_width - (line_x - content_x)).max(TEXT_CHAR_WIDTH);
            if let Some(mut layout_child) =
                Self::from_styled_node(child, line_x, line_y, remaining_width)
            {
                if line_x > content_x && layout_child.total_width() > remaining_width {
                    // RUST FUNDAMENTAL: This is a line-wrap reflow step:
                    // if the child does not fit on the current line, advance to the next line and lay it out again.
                    max_line_width = max_line_width.max(line_x - content_x);
                    line_y += line_height.max(TEXT_LINE_HEIGHT);
                    line_x = content_x;
                    line_height = 0.0;

                    if let Some(reflowed_child) =
                        Self::from_styled_node(child, line_x, line_y, content_width)
                    {
                        layout_child = reflowed_child;
                    }
                }

                line_x += layout_child.total_width();
                line_height = line_height.max(layout_child.total_height());
                max_line_width = max_line_width.max(line_x - content_x);
                layout_children.push(layout_child);
            }
        }

        let content_used_width = if layout_children.is_empty() {
            content_width.min(120.0)
        } else {
            max_line_width.max((line_x - content_x).min(content_width))
        };
        let total_content_height = if layout_children.is_empty() {
            INLINE_BOX_HEIGHT
        } else {
            (line_y - content_y) + line_height.max(INLINE_BOX_HEIGHT)
        };
        let resolved_content_height = clamp_content_height(&styles, total_content_height);

        let alignment = styles.text_align();
        if alignment != TextAlign::Left {
            let mut line_start = 0;
            while line_start < layout_children.len() {
                let line_y_val = layout_children[line_start].rect.y;
                let mut line_end = line_start + 1;
                while line_end < layout_children.len()
                    && layout_children[line_end].rect.y == line_y_val
                {
                    line_end += 1;
                }

                let row_width: f32 = layout_children[line_start..line_end]
                    .iter()
                    .map(|b| b.total_width())
                    .sum();
                // RUST FUNDAMENTAL: Slicing a vector like `line_start..line_end` gives a borrowed window into consecutive items.
                let offset = match alignment {
                    TextAlign::Center => (content_width - row_width) / 2.0,
                    TextAlign::Right => content_width - row_width,
                    TextAlign::Left => 0.0,
                };

                if offset > 0.0 {
                    for b in &mut layout_children[line_start..line_end] {
                        b.offset(offset, 0.0);
                    }
                }
                line_start = line_end;
            }
        }

        Self {
            kind: LayoutKind::Inline {
                tag_name: tag_name.to_string(),
            },
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: (content_used_width + padding.horizontal() + border.horizontal())
                    .min(max_rect_width),
                height: resolved_content_height + padding.vertical() + border.vertical(),
            },
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_inline_sequence(
        children: &[&StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Option<Self> {
        if children.is_empty() {
            return None;
        }
        // RUST FUNDAMENTAL: Returning `Option<Self>` here lets callers skip constructing anonymous boxes when there is nothing to wrap.

        let mut layout_children = Vec::new();
        let mut line_x = x;
        let mut line_y = y;
        let mut line_height: f32 = 0.0;
        let mut max_line_width: f32 = 0.0;

        for child in children {
            if let Some(text) = child.text() {
                let fragments = Self::layout_text_fragments(
                    &text,
                    child.styles().clone(),
                    x,
                    available_width,
                    &mut line_x,
                    &mut line_y,
                    &mut line_height,
                    &mut max_line_width,
                );
                layout_children.extend(fragments);
                continue;
            }

            let remaining_width = (available_width - (line_x - x)).max(TEXT_CHAR_WIDTH);
            if let Some(mut layout_child) =
                Self::from_styled_node(child, line_x, line_y, remaining_width)
            {
                if line_x > x && layout_child.total_width() > remaining_width {
                    max_line_width = max_line_width.max(line_x - x);
                    line_y += line_height.max(TEXT_LINE_HEIGHT);
                    line_x = x;
                    line_height = 0.0;

                    if let Some(reflowed_child) =
                        Self::from_styled_node(child, line_x, line_y, available_width)
                    {
                        layout_child = reflowed_child;
                    }
                }

                line_x += layout_child.total_width();
                line_height = line_height.max(layout_child.total_height());
                max_line_width = max_line_width.max(line_x - x);
                layout_children.push(layout_child);
            }
        }

        let content_used_width = if layout_children.is_empty() {
            available_width.min(120.0)
        } else {
            max_line_width.max((line_x - x).min(available_width))
        };
        let total_content_height = if layout_children.is_empty() {
            INLINE_BOX_HEIGHT
        } else {
            (line_y - y) + line_height.max(INLINE_BOX_HEIGHT)
        };

        Some(Self {
            // RUST FUNDAMENTAL: Synthetic nodes like `"anonymous-inline"` are common engine internals.
            // They let the layout tree represent structure that did not exist explicitly in the source DOM.
            kind: LayoutKind::Block {
                tag_name: "anonymous-inline".to_string(),
            },
            rect: Rect {
                x,
                y,
                width: content_used_width,
                height: total_content_height,
            },
            styles: StyleMap::default(),
            margin: Margin::zero(),
            border: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            children: layout_children,
        })
    }

    fn layout_text(text: &str, styles: StyleMap, x: f32, y: f32) -> Self {
        let line_height = line_height_from_styles(&styles);

        Self {
            // RUST FUNDAMENTAL: Owned `String` storage in the layout tree decouples text boxes from the original DOM borrow lifetime.
            kind: LayoutKind::Text {
                text: text.to_string(),
            },
            rect: Rect {
                x,
                y,
                width: measure_text_width(text, &styles),
                height: line_height,
            },
            styles,
            margin: Margin::zero(),
            border: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            children: Vec::new(),
        }
    }

    fn decode_entities(text: &str) -> String {
        // RUST FUNDAMENTAL: Like the HTML parser's helper, this favors a simple chain of transformations over maximal efficiency.
        text.replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&apos;", "'")
            .replace("&copy;", "©")
            .replace("&reg;", "®")
            .replace("&trade;", "™")
            .replace("&bull;", "•")
            .replace("&middot;", "·")
            .replace("&ndash;", "–")
            .replace("&mdash;", "—")
    }

    fn layout_text_fragments(
        text: &str,
        styles: StyleMap,
        x: f32,
        available_width: f32,
        line_x: &mut f32,
        line_y: &mut f32,
        line_height: &mut f32,
        max_line_width: &mut f32,
    ) -> Vec<Self> {
        let mut fragments = Vec::new();
        let text = Self::decode_entities(text);

        // RUST FUNDAMENTAL: Splitting into owned `String` words gives the wrapper freedom to move and combine fragments as needed.
        let words = text
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();

        if words.is_empty() {
            return fragments;
        }

        let base_line_height = line_height_from_styles(&styles);
        let mut current_line = String::new();

        for word in words {
            let candidate = if current_line.is_empty() {
                word.clone()
            } else {
                format!("{} {}", current_line, word)
            };
            let candidate_width = measure_text_width(&candidate, &styles);
            let current_line_width = measure_text_width(&current_line, &styles);
            let used_width = (*line_x - x) + current_line_width;
            let remaining_width = (available_width - used_width).max(1.0);

            if !current_line.is_empty() && candidate_width > remaining_width {
                if !current_line.is_empty() {
                    // RUST FUNDAMENTAL: Cloning `styles` per fragment is acceptable here because `StyleMap` is relatively small
                    // and it keeps each laid-out text fragment self-contained.
                    let fragment =
                        Self::layout_text(&current_line, styles.clone(), *line_x, *line_y);
                    *line_x += fragment.rect.width;
                    *max_line_width = max_line_width.max(*line_x - x);
                    fragments.push(fragment);
                }

                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
                current_line = word;
            } else {
                current_line = candidate;
            }
        }

        if !current_line.is_empty() {
            let last_width = measure_text_width(&current_line, &styles);
            if *line_x - x + last_width > available_width && *line_x > x {
                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
            }
            let fragment = Self::layout_text(&current_line, styles.clone(), *line_x, *line_y);
            *line_x += fragment.rect.width;
            *line_height = (*line_height).max(base_line_height);
            *max_line_width = max_line_width.max(*line_x - x);
            fragments.push(fragment);
        }

        fragments
    }

    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        // RUST FUNDAMENTAL: Pretty-printers often mirror tree structure by using indentation depth as recursive state.
        let indent = "  ".repeat(depth);
        match &self.kind {
            LayoutKind::Viewport => {
                writeln!(f, "{indent}viewport {}", self.rect)?;
            }
            LayoutKind::Block { tag_name } => {
                writeln!(
                    f,
                    "{indent}block<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::InlineBlock { tag_name } => {
                writeln!(
                    f,
                    "{indent}inline-block<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Inline { tag_name } => {
                writeln!(
                    f,
                    "{indent}inline<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Control { tag_name } => {
                writeln!(
                    f,
                    "{indent}control<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Image {
                alt,
                src,
                display_mode,
            } => {
                let kind = if *display_mode == DisplayMode::Inline {
                    "inline"
                } else {
                    "block"
                };
                writeln!(
                    f,
                    "{indent}{kind}<img alt={:?} src={:?}> {} {}",
                    alt,
                    src,
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Text { text } => {
                writeln!(f, "{indent}text(\"{text}\") {}", self.rect)?;
            }
        }

        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }

    pub fn rect(&self) -> Rect {
        // RUST FUNDAMENTAL: Returning `Rect` by value is cheap because `Rect` implements `Copy`.
        self.rect
    }

    pub fn total_width(&self) -> f32 {
        // RUST FUNDAMENTAL: Helper accessors like this capture layout-specific definitions,
        // so callers do not duplicate the same geometry formula everywhere.
        self.margin.left.to_px() + self.rect.width + self.margin.right.to_px()
    }

    pub fn total_height(&self) -> f32 {
        self.margin.top + self.rect.height + self.margin.bottom
    }

    #[allow(dead_code)]
    pub fn padding(&self) -> EdgeSizes {
        self.padding
    }

    #[allow(dead_code)]
    pub fn content_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + self.border.left + self.padding.left,
            y: self.rect.y + self.border.top + self.padding.top,
            // RUST FUNDAMENTAL: `.max(0.0)` protects against negative sizes when borders or padding exceed the outer box dimensions.
            width: (self.rect.width - self.border.horizontal() - self.padding.horizontal())
                .max(0.0),
            height: (self.rect.height - self.border.vertical() - self.padding.vertical()).max(0.0),
        }
    }

    pub fn padding_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + self.border.left,
            y: self.rect.y + self.border.top,
            width: (self.rect.width - self.border.horizontal()).max(0.0),
            height: (self.rect.height - self.border.vertical()).max(0.0),
        }
    }

    pub fn styles(&self) -> &StyleMap {
        &self.styles
    }

    pub fn children(&self) -> &[LayoutBox] {
        &self.children
    }

    pub fn tag_name(&self) -> Option<&str> {
        // RUST FUNDAMENTAL: Returning `Option<&str>` borrows the tag name from inside the enum instead of cloning a new `String`.
        match &self.kind {
            LayoutKind::Block { tag_name }
            | LayoutKind::Inline { tag_name }
            | LayoutKind::Control { tag_name } => Some(tag_name),
            LayoutKind::Image { .. } => Some("img"),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn is_viewport(&self) -> bool {
        matches!(self.kind, LayoutKind::Viewport)
    }

    pub fn image_alt(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Image { alt, .. } => alt.as_deref(),
            _ => None,
        }
    }

    pub fn image_src(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Image { src, .. } => src.as_deref(),
            _ => None,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, LayoutKind::Image { .. })
    }

    pub fn is_control(&self) -> bool {
        matches!(self.kind, LayoutKind::Control { .. })
    }

    pub fn offset(&mut self, dx: f32, dy: f32) {
        // RUST FUNDAMENTAL: Recursive in-place mutation like this is a natural fit for tree structures when a parent movement
        // should shift every descendant by the same delta.
        self.rect.x += dx;
        self.rect.y += dy;
        for child in &mut self.children {
            child.offset(dx, dy);
        }
    }
}

fn font_size_from_styles(styles: &StyleMap) -> f32 {
    // RUST FUNDAMENTAL: Chaining fallbacks with `.or_else(...)` lets the code try progressively less precise sources.
    styles
        .font_size_resolved(16.0, 16.0)
        .or_else(|| styles.font_size_px())
        .filter(|&s| s > 0.0)
        .unwrap_or(16.0)
}

fn measure_text_width(text: &str, styles: &StyleMap) -> f32 {
    crate::font::measure_text(text, font_size_from_styles(styles))
}

fn line_height_from_styles(styles: &StyleMap) -> f32 {
    let fs = font_size_from_styles(styles);
    // RUST FUNDAMENTAL: Multiplying by `1.2` here encodes a conventional default line-height when CSS did not specify one.
    styles.line_height_px().unwrap_or(fs * 1.2)
}

fn control_label(tag_name: &str, node: &StyledNode) -> String {
    // RUST FUNDAMENTAL: Matching on string literals is perfectly normal when a small closed set of tag names drives behavior.
    match tag_name {
        "input" => node
            .attribute("value")
            .or_else(|| node.attribute("placeholder"))
            .unwrap_or_default(),
        "textarea" => {
            let from_children = collect_text_content(node).trim().to_string();
            if from_children.is_empty() {
                node.attribute("placeholder").unwrap_or_default()
            } else {
                from_children
            }
        }
        "button" => collect_text_content(node).trim().to_string(),
        _ => String::new(),
    }
}

fn collect_text_content(node: &StyledNode) -> String {
    if let Some(text) = node.text() {
        return text;
    }

    // RUST FUNDAMENTAL: This helper recursively flattens text from a subtree into one owned `String`.
    let mut combined = String::new();
    for child in node.children() {
        let part = collect_text_content(child);
        if part.is_empty() {
            continue;
        }
        if !combined.is_empty() {
            combined.push(' ');
        }
        combined.push_str(part.trim());
    }
    combined
}

impl Display for LayoutTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // RUST FUNDAMENTAL: The wrapper type's `Display` impl delegates to the root node, which centralizes the actual formatting logic.
        self.root.fmt_with_indent(f, 0)
    }
}

impl Display for Rect {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // RUST FUNDAMENTAL: Format specifiers like `{:.0}` round the floating-point value for display without changing the stored value.
        write!(
            f,
            "[x: {:.0}, y: {:.0}, w: {:.0}, h: {:.0}]",
            self.x, self.y, self.width, self.height
        )
    }
}

fn format_styles(styles: &StyleMap) -> String {
    if styles.is_empty() {
        "{}".to_string()
    } else {
        // RUST FUNDAMENTAL: `format!("{styles}")` uses the `Display` implementation of `StyleMap` to produce an owned string.
        format!("{styles}")
    }
}

fn clamp_content_width(styles: &StyleMap, candidate_width: f32, available_width: f32) -> f32 {
    // Resolve width with support for %, rem, em
    // RUST FUNDAMENTAL: This helper centralizes CSS width constraints so callers can work with one consistent "content box width" rule.
    let font_size = styles
        .font_size_resolved(16.0, 16.0)
        .or_else(|| styles.font_size_px())
        .unwrap_or(16.0);
    let mut width = styles
        .width_resolved(available_width, font_size, 16.0, 1200.0)
        .or_else(|| styles.width_px())
        .unwrap_or(candidate_width);
    if styles.box_sizing() == BoxSizing::BorderBox {
        let border = styles.border_width();
        let padding = styles.padding();
        width = (width - border.horizontal() - padding.horizontal()).max(0.0);
    }
    if let Some(min_width) = styles.min_width_px() {
        let mut min = min_width;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            min = (min - border.horizontal() - padding.horizontal()).max(0.0);
        }
        width = width.max(min);
    }
    if let Some(max_width) = styles.max_width_px() {
        let mut max = max_width;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            max = (max - border.horizontal() - padding.horizontal()).max(0.0);
        }
        width = width.min(max);
    }
    width.min(available_width).max(0.0)
}

fn clamp_content_height(styles: &StyleMap, candidate_height: f32) -> f32 {
    // RUST FUNDAMENTAL: Width and height clamping are kept separate because they use different context and CSS properties,
    // even though the overall pattern is similar.
    let mut height = styles.height_px().unwrap_or(candidate_height);
    if styles.box_sizing() == BoxSizing::BorderBox {
        let border = styles.border_width();
        let padding = styles.padding();
        height = (height - border.vertical() - padding.vertical()).max(0.0);
    }
    if let Some(min_height) = styles.min_height_px() {
        let mut min = min_height;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            min = (min - border.vertical() - padding.vertical()).max(0.0);
        }
        height = height.max(min);
    }
    if let Some(max_height) = styles.max_height_px() {
        let mut max = max_height;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            max = (max - border.vertical() - padding.vertical()).max(0.0);
        }
        height = height.min(max);
    }
    height.max(0.0)
}

fn parse_html_length_px(value: &str) -> Option<f32> {
    // RUST FUNDAMENTAL: `unwrap_or(value)` on the stripped suffix means plain numeric HTML attributes like `width="120"`
    // are accepted alongside explicit `px` suffixed values.
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .parse::<f32>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::LayoutTree;
    use crate::css::Stylesheet;
    use crate::dom::Node;
    use crate::style::StyleTree;

    #[test]
    fn builds_layout_boxes_with_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hello")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; color: blue; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert!(rendered.contains("viewport [x: 0, y: 0, w: 1200"));
        assert!(rendered.contains("block<body> {} [x: 0, y: 0, w: 1200"));
        assert!(rendered.contains("inline<p> {color: blue, display: inline}"));
        assert!(rendered.contains("text(\"Hello\") [x: 0, y: 0"));
    }

    #[test]
    fn stacks_block_children_vertically() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("One")]),
                Node::element("section", vec![Node::text("Two")]),
            ],
        )]);
        let stylesheet = Stylesheet::parse("");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert_eq!(rendered.matches("block<section> {}").count(), 2);
        assert!(rendered.contains("text(\"One\") [x: 0, y: 0"));
        assert!(rendered.contains("text(\"Two\")"));
    }

    #[test]
    fn wraps_inline_text_across_multiple_lines() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("alpha beta gamma delta epsilon zeta")],
            )],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 96.0);
        let rendered = layout.to_string();
        println!(
            "DEBUG wraps_inline_text_across_multiple_lines:\n{}",
            rendered
        );

        assert!(rendered.contains("inline<p> {display: inline}"));
        assert!(rendered.contains("text(\"alpha\") [x: 0, y: 0, w: 80, h: 19]"));
        assert!(rendered.contains("text(\"beta\") [x: 0, y: 19, w: 64, h: 19]"));
        assert!(rendered.contains("text(\"gamma\") [x: 0, y: 38, w: 80, h: 19]"));
    }

    #[test]
    fn wraps_inline_children_when_the_row_fills() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "span",
                vec![
                    Node::element("em", vec![Node::text("hello")]),
                    Node::element("strong", vec![Node::text("world")]),
                ],
            )],
        )]);
        let stylesheet = Stylesheet::parse(
            "span { display: inline; } em { display: inline; } strong { display: inline; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 72.0);
        let rendered = layout.to_string();
        println!(
            "DEBUG wraps_inline_children_when_the_row_fills:\n{}",
            rendered
        );

        assert!(rendered.contains("inline<em> {display: inline}"));
        assert!(rendered.contains("inline<strong> {display: inline}"));
        assert!(rendered.contains("text(\"hello\")"));
        assert!(rendered.contains("text(\"world\")"));
    }

    #[test]
    fn applies_margin_and_padding_to_block_layout() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Box")])],
        )]);
        let stylesheet = Stylesheet::parse("section { margin: 10px 12px; padding: 4px 6px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();
        println!(
            "DEBUG applies_margin_and_padding_to_block_layout:\n{}",
            rendered
        );

        assert!(rendered.contains(
            "block<section> {margin: 10px 12px, padding: 4px 6px} [x: 12, y: 10, w: 176, h: 33]"
        ));
        // Box is text content. 3 chars * 16 = 48.
        // x = 12 (margin) + 6 (padding) = 18.
        // y = 10 (margin) + 4 (padding) = 14.
        assert!(rendered.contains("text(\"Box\") [x: 18, y: 14, w: 48, h: 19]"));
    }

    #[test]
    fn includes_border_width_in_box_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Border")])],
        )]);
        let stylesheet =
            Stylesheet::parse("section { border: 4px solid ember; padding: 6px; width: 80px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 220.0);
        let rendered = layout.to_string();
        println!("DEBUG includes_border_width_in_box_geometry:\n{}", rendered);

        assert!(rendered.contains("block<section> {border: 4px solid ember, padding: 6px, width: 80px} [x: 0, y: 0, w: 100, h: 45]"));
        assert!(rendered.contains("text(\"Border\") [x: 10, y: 10, w: 96, h: 19]"));
    }

    #[test]
    fn applies_fixed_width_and_height_to_block_boxes() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Sized")])],
        )]);
        let stylesheet = Stylesheet::parse("section { width: 120px; height: 48px; padding: 4px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 300.0);
        let rendered = layout.to_string();

        // h = 48 + 8 = 56. w = 120 + 8 = 128.
        assert!(rendered.contains(
            "block<section> {height: 48px, padding: 4px, width: 120px} [x: 0, y: 0, w: 128, h: 56]"
        ));
        assert!(rendered.contains("text(\"Sized\") [x: 4, y: 4, w: 80, h: 19]"));
    }

    #[test]
    fn constrains_inline_wrapping_with_fixed_width() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("one two three four five")],
            )],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; width: 64px; padding: 4px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();
        println!(
            "DEBUG constrains_inline_wrapping_with_fixed_width:\n{}",
            rendered
        );

        assert!(rendered.contains("inline<p> {display: inline, padding: 4px, width: 64px}"));
        assert!(rendered.contains("text(\"one\") [x: 4, y: 4, w: 48, h: 19]"));
        assert!(rendered.contains("text(\"two\")"));
        assert!(rendered.contains("text(\"three\")"));
        assert!(rendered.contains("text(\"four\")"));
        assert!(rendered.contains("text(\"five\")"));
    }

    #[test]
    fn aligns_inline_text_horizontally() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Center")])],
        )]);
        let stylesheet =
            Stylesheet::parse("p { display: inline; text-align: center; width: 100px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();

        // "Center" is 6 chars. 6 * 16.0 = 96.0 px.
        // alignment offset = (100 - 96) / 2 = 2.0
        assert!(rendered.contains("text(\"Center\") [x: 2"));
    }

    #[test]
    fn clamps_block_width_and_height_with_min_and_max() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("Min")]),
                Node::element("article", vec![Node::text("Max")]),
            ],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { width: 40px; min-width: 80px; height: 12px; min-height: 24px; padding: 4px; } article { width: 180px; max-width: 96px; height: 120px; max-height: 40px; padding: 4px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {height: 12px, min-height: 24px, min-width: 80px, padding: 4px, width: 40px} [x: 0, y: 0, w: 88, h: 32]"));
        assert!(rendered.contains("block<article> {height: 120px, max-height: 40px, max-width: 96px, padding: 4px, width: 180px} [x: 0, y: 32, w: 104, h: 48]"));
    }

    #[test]
    fn collapses_vertical_margins_between_block_siblings() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("One")]),
                Node::element("section", vec![Node::text("Two")]),
            ],
        )]);
        let stylesheet =
            Stylesheet::parse("section { margin-top: 12px; margin-bottom: 18px; padding: 4px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        // section h=18. body h = 12 (top) + 18 + 18 + 18 (collapsed bottom if?) = wait.
        // section 1 margin-top 12. y starts at 12.
        // section 1 bottom 18, section 2 top 12. collapsed to 18.
        // section 2 starts at 12 + 18 + 18 = 48.
        assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 12, w: 240, h: 33]"));
        assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 63, w: 240, h: 33]"));
    }

    #[test]
    fn clamps_inline_width_before_wrapping() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("one two three four five")],
            )],
        )]);
        let stylesheet = Stylesheet::parse(
            "p { display: inline; width: 140px; max-width: 64px; min-height: 60px; padding: 4px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();
        println!("DEBUG clamps_inline_width_before_wrapping:\n{}", rendered);

        assert!(rendered.contains("inline<p> {display: inline, max-width: 64px, min-height: 60px, padding: 4px, width: 140px}"));
        assert!(rendered.contains("text(\"one\")"));
        assert!(rendered.contains("text(\"two\")"));
        assert!(rendered.contains("text(\"three\")"));
        assert!(rendered.contains("text(\"four\")"));
        assert!(rendered.contains("text(\"five\")"));
    }

    #[test]
    fn omits_nodes_with_display_none() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hidden")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: none; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert!(!rendered.contains("<p>"));
        assert!(!rendered.contains("Hidden"));
    }

    #[test]
    fn includes_border_width_in_inline_box_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("span", vec![Node::text("Hi")])],
        )]);
        let stylesheet =
            Stylesheet::parse("span { display: inline; border: 4px solid ember; padding: 2px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();

        assert!(rendered.contains(
            "inline<span> {border: 4px solid ember, display: inline, padding: 2px} [x: 0, y: 0, w: 44, h: 31]"
        ));
        assert!(rendered.contains("text(\"Hi\") [x: 6, y: 6, w: 32, h: 19]"));
    }

    #[test]
    fn lays_out_images_with_attributes_as_replaced_boxes() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element_with_attributes(
                "img",
                [
                    ("alt".to_string(), "grumpy cat".to_string()),
                    ("src".to_string(), "cat.txt".to_string()),
                    ("width".to_string(), "120".to_string()),
                    ("height".to_string(), "80".to_string()),
                ]
                .into_iter()
                .collect(),
                Vec::new(),
            )],
        )]);
        let stylesheet =
            Stylesheet::parse("img { display: inline; padding: 4px; border: 2px solid ember; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains(
            "inline<img alt=Some(\"grumpy cat\") src=Some(\"cat.txt\")> {border: 2px solid ember, display: inline, padding: 4px} [x: 0, y: 0, w: 132, h: 92]"
        ));
    }
}
