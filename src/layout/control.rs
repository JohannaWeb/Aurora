use crate::css::{EdgeSizes, Margin, MarginValue, StyleMap};
use crate::style::StyledNode;

use super::constraints::{clamp_content_height, clamp_content_width};
use super::text_metrics::{control_label, line_height_from_styles, measure_text_width};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_control(
        tag_name: &str,
        node: &StyledNode,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        x: f32,
        y: f32,
        available_width: f32,
        viewport_height: f32,
    ) -> Self {
        let mut rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
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
        let content_height = clamp_content_height(&styles, default_content_height, viewport_height);

        if let (MarginValue::Auto, MarginValue::Auto) = (margin.left, margin.right) {
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
}
