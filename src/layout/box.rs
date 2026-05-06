use crate::css::{DisplayMode, EdgeSizes, Margin, StyleMap};

use super::Rect;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
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
        self.rect.x += dx;
        self.rect.y += dy;
        for child in &mut self.children {
            child.offset(dx, dy);
        }
    }
}
