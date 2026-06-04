use crate::css::{DisplayMode, EdgeSizes, Margin, StyleMap};
use std::rc::Rc;

use super::Rect;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    pub(in crate::layout) node: Option<crate::dom::NodePtr>,
    pub(in crate::layout) kind: LayoutKind,
    pub(in crate::layout) rect: Rect,
    pub(in crate::layout) styles: StyleMap,
    pub(in crate::layout) margin: Margin,
    pub(in crate::layout) border: EdgeSizes,
    pub(in crate::layout) padding: EdgeSizes,
    pub(in crate::layout) children: Vec<LayoutBox>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::layout) enum LayoutKind {
    Viewport,
    Block {
        tag_name: String,
    },
    InlineBlock {
        tag_name: String,
    },
    Inline {
        tag_name: String,
    },
    Control {
        tag_name: String,
    },
    Image {
        alt: Option<String>,
        src: Option<String>,
        display_mode: DisplayMode,
    },
    Media {
        src: Option<String>,
        poster: Option<String>,
        display_mode: DisplayMode,
    },
    Text {
        text: String,
    },
}

impl LayoutBox {
    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn total_width(&self) -> f32 {
        self.margin.left.to_px() + self.rect.width + self.margin.right.to_px()
    }

    pub fn total_height(&self) -> f32 {
        self.margin.top.to_px() + self.rect.height + self.margin.bottom.to_px()
    }

    pub fn styles(&self) -> &StyleMap {
        &self.styles
    }

    pub fn children(&self) -> &[LayoutBox] {
        &self.children
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

    pub fn image_src(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Image { src, .. } => src.as_deref(),
            _ => None,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, LayoutKind::Image { .. })
    }

    pub fn media_src(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Media { src, .. } => src.as_deref(),
            _ => None,
        }
    }

    pub fn media_poster(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Media { poster, .. } => poster.as_deref(),
            _ => None,
        }
    }

    pub fn offset(&mut self, dx: f32, dy: f32) {
        self.rect.x += dx;
        self.rect.y += dy;
        for child in &mut self.children {
            child.offset(dx, dy);
        }
    }

    pub fn find_node(&self, node: &crate::dom::NodePtr) -> Option<&LayoutBox> {
        if self
            .node
            .as_ref()
            .map(|n| Rc::ptr_eq(n, node))
            .unwrap_or(false)
        {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_node(node) {
                return Some(found);
            }
        }
        None
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<crate::dom::NodePtr> {
        if !self.rect.contains(x, y) {
            return None;
        }

        // Search children in reverse order (topmost first)
        for child in self.children.iter().rev() {
            if let Some(found) = child.hit_test(x, y) {
                return Some(found);
            }
        }

        self.node.clone()
    }
}
