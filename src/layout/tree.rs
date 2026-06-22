use crate::css::{EdgeSizes, Margin, StyleMap};
use crate::style::StyleTree;

use super::constants::{DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH};
use super::engine::layout_root_from_style_tree;
use super::{LayoutBox, LayoutKind, Rect};

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
        let root = layout_root_from_style_tree(style_tree, viewport);
        Self { root }
    }

    pub fn placeholder(viewport: ViewportSize) -> Self {
        Self {
            root: LayoutBox {
                node: None,
                kind: LayoutKind::Viewport,
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: viewport.width,
                    height: viewport.height,
                },
                styles: StyleMap::default(),
                margin: Margin::zero(),
                border: EdgeSizes::zero(),
                padding: EdgeSizes::zero(),
                children: Vec::new(),
            },
        }
    }

    #[cfg(feature = "taffy-document")]
    pub fn from_root(root: LayoutBox) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &LayoutBox {
        &self.root
    }

    #[allow(dead_code)]
    pub fn find_box_for_node(&self, node: &crate::dom::NodePtr) -> Option<&LayoutBox> {
        self.root.find_node(node)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<crate::dom::NodePtr> {
        self.root.hit_test(x, y)
    }
}
