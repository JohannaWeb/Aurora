use crate::css::{EdgeSizes, Margin, StyleMap, TextAlign};
use crate::style::StyledNode;

use super::constants::{INLINE_BOX_HEIGHT, TEXT_CHAR_WIDTH, TEXT_LINE_HEIGHT};
use super::constraints::{clamp_content_height, clamp_content_width};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_inline(
        tag_name: &str,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
        viewport_height: f32,
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
                Self::from_styled_node(child, line_x, line_y, remaining_width, viewport_height)
            {
                if line_x > content_x && layout_child.total_width() > remaining_width {
                    // if the child does not fit on the current line, advance to the next line and lay it out again.
                    max_line_width = max_line_width.max(line_x - content_x);
                    line_y += line_height.max(TEXT_LINE_HEIGHT);
                    line_x = content_x;
                    line_height = 0.0;

                    if let Some(reflowed_child) = Self::from_styled_node(
                        child,
                        line_x,
                        line_y,
                        content_width,
                        viewport_height,
                    ) {
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
        let resolved_content_height =
            clamp_content_height(&styles, total_content_height, viewport_height);

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
}
