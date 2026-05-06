use crate::css::{DisplayMode, EdgeSizes, Margin, StyleMap};
use crate::style::StyledNode;

use super::constraints::{clamp_content_height, clamp_content_width, parse_html_length_px};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_image(
        node: &StyledNode,
        styles: StyleMap,
        margin: Margin,
        border: EdgeSizes,
        padding: EdgeSizes,
        x: f32,
        y: f32,
        available_width: f32,
        viewport_height: f32,
        display_mode: DisplayMode,
    ) -> Self {
        let rect_x = x + margin.left.to_px();
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
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
        let content_height = clamp_content_height(&styles, height_hint, viewport_height);

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
}
