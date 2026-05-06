use crate::style::StyleTree;

use super::constants::{DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH};
use super::LayoutBox;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTree {
    root: LayoutBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSize {
    pub width: f32,
    pub height: f32,
}

impl LayoutTree {
    #[allow(dead_code)]
    pub fn from_style_tree(style_tree: &StyleTree) -> Self {
        Self::from_style_tree_with_viewport_width(style_tree, DEFAULT_VIEWPORT_WIDTH)
    }

    pub fn from_style_tree_with_viewport_width(
        style_tree: &StyleTree,
        viewport_width: f32,
    ) -> Self {
        Self::from_style_tree_with_viewport(
            style_tree,
            ViewportSize {
                width: viewport_width,
                height: DEFAULT_VIEWPORT_HEIGHT,
            },
        )
    }

    pub fn from_style_tree_with_viewport(style_tree: &StyleTree, viewport: ViewportSize) -> Self {
        let root = LayoutBox::layout_root(style_tree.root(), viewport)
            .expect("style tree root must produce a viewport");
        Self { root }
    }

    pub fn root(&self) -> &LayoutBox {
        &self.root
    }
}
