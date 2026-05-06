use crate::css::{DisplayMode, EdgeSizes, Margin, MarginValue, StyleMap, TextAlign};
use crate::style::StyledNode;

use super::constants::BLOCK_VERTICAL_PADDING;
use super::constraints::{clamp_content_height, clamp_content_width};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_container(
        kind: LayoutKind,
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
                .map(|_| {
                    matches!(
                        child.styles().display_mode(),
                        DisplayMode::Block | DisplayMode::Flex
                    )
                })
                .unwrap_or(false);
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
                    viewport_height,
                ) {
                    let alignment = styles.text_align();
                    if alignment != TextAlign::Left {
                        let child_width = layout_child.total_width();
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
        let resolved_content_height = clamp_content_height(&styles, inner_height, viewport_height);

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
}
