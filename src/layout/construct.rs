use crate::css::{DisplayMode, EdgeSizes, Margin};
use crate::style::StyledNode;

use super::{LayoutBox, LayoutKind, ViewportSize};

impl LayoutBox {
    pub(in crate::layout) fn layout_root(
        node: &StyledNode,
        viewport: ViewportSize,
    ) -> Option<Self> {
        // if the styled root cannot produce a layout box.
        let mut root = Self::from_styled_node(node, 0.0, 0.0, viewport.width, viewport.height)?;
        root.rect.width = viewport.width;
        Some(root)
    }
    // Layout a styled node recursively based on its type
    pub(in crate::layout) fn from_styled_node(
        node: &StyledNode,
        x: f32,
        y: f32,
        available_width: f32,
        viewport_height: f32,
    ) -> Option<Self> {
        // though it does allocate temporary strings each time.
        if node.tag_name() == Some("style".to_string())
            || node.tag_name() == Some("script".to_string())
        {
            return None;
        }

        match node.tag_name() {
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
                viewport_height,
            )),
            Some(tag_name) => {
                Self::from_element(&tag_name, node, x, y, available_width, viewport_height)
            }
            None => Some(Self::layout_text(
                &node.text().unwrap_or_default(),
                node.styles().clone(),
                x,
                y,
            )),
        }
    }
    // Layout an element node based on display mode and tag name
    pub(in crate::layout) fn from_element(
        tag_name: &str,
        node: &StyledNode,
        // X position
        x: f32,
        // Y position
        y: f32,
        available_width: f32,
        viewport_height: f32,
    ) -> Option<Self> {
        let styles = node.styles().clone();
        // and additional runtime data like the HTML tag name.
        match styles.display_mode() {
            DisplayMode::None => None,
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
                    viewport_height,
                    mode,
                ))
            }
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
                    viewport_height,
                ))
            }
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
                viewport_height,
            )),
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
                viewport_height,
            )),
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
                viewport_height,
            )),
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
                viewport_height,
            )),
        }
    }
}
