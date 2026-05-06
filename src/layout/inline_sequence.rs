use crate::css::{EdgeSizes, Margin, StyleMap};
use crate::style::StyledNode;

use super::constants::{
    DEFAULT_VIEWPORT_HEIGHT, INLINE_BOX_HEIGHT, TEXT_CHAR_WIDTH, TEXT_LINE_HEIGHT,
};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_inline_sequence(
        children: &[&StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Option<Self> {
        if children.is_empty() {
            return None;
        }

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
            if let Some(mut layout_child) = Self::from_styled_node(
                child,
                line_x,
                line_y,
                remaining_width,
                DEFAULT_VIEWPORT_HEIGHT,
            ) {
                if line_x > x && layout_child.total_width() > remaining_width {
                    max_line_width = max_line_width.max(line_x - x);
                    line_y += line_height.max(TEXT_LINE_HEIGHT);
                    line_x = x;
                    line_height = 0.0;

                    if let Some(reflowed_child) = Self::from_styled_node(
                        child,
                        line_x,
                        line_y,
                        available_width,
                        DEFAULT_VIEWPORT_HEIGHT,
                    ) {
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
}
