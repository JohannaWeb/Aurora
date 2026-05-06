use crate::css::{EdgeSizes, FlexDirection, Margin, MarginValue, StyleMap};
use crate::style::StyledNode;

use super::constants::BLOCK_VERTICAL_PADDING;
use super::constraints::{clamp_content_height, clamp_content_width};
use super::{LayoutBox, LayoutKind, Rect};

mod column;
mod measure;
mod row;
mod row_wrap;
mod spacing;
mod types;

use measure::measure_flex_children;
use types::FlexContext;

impl LayoutBox {
    pub(in crate::layout) fn layout_flex_container(
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

        if let (MarginValue::Auto, MarginValue::Auto) = (margin.left, margin.right) {
            let total_box_width = content_width + padding.horizontal() + border.horizontal();
            let free_space = (available_width - total_box_width).max(0.0);
            rect_x = x + free_space / 2.0;
        }

        let ctx = FlexContext::from_styles(&styles, content_width, rect_x, rect_y, border, padding);
        let mut measured = measure_flex_children(children, content_width, viewport_height, &ctx);
        let mut resolved_content_height =
            clamp_content_height(&styles, measured.inner_height(&ctx), viewport_height)
                .max(BLOCK_VERTICAL_PADDING);

        if ctx.direction == FlexDirection::Row && ctx.wraps {
            resolved_content_height = row_wrap::position_wrapped_rows(
                &mut measured.children,
                &styles,
                viewport_height,
                &ctx,
            );
        } else if ctx.direction == FlexDirection::Row {
            row::position_row(
                &mut measured.children,
                measured.total_width,
                resolved_content_height,
                &ctx,
            );
        } else {
            column::position_column(
                &mut measured.children,
                measured.total_height,
                resolved_content_height,
                &ctx,
            );
        }

        Self {
            kind,
            rect: Rect {
                x: rect_x,
                y: rect_y,
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
            children: measured.children,
        }
    }
}
