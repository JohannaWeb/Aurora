#![allow(dead_code, unused_imports)]

use std::collections::HashMap;
use std::rc::Rc;
use taffy::prelude::*;

use crate::dom::NodePtr;
use crate::style::StyleTree;

use super::taffy_adapter::style_to_taffy;
use super::{LayoutBox, LayoutKind, Rect, ViewportSize};

/// Owns the Taffy tree and DOM→NodeId mapping across reflows.
/// Keeps the Taffy tree alive so dirty-bit invalidation can skip clean subtrees.
pub struct LayoutDocument {
    taffy: TaffyTree<()>,
    /// Maps Rc pointer address → Taffy NodeId for O(1) dirty marking.
    node_map: HashMap<usize, NodeId>,
    /// The root NodeId in the Taffy tree.
    root_id: Option<NodeId>,
    viewport: ViewportSize,
}

impl LayoutDocument {
    pub fn new(viewport: ViewportSize) -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: HashMap::new(),
            root_id: None,
            viewport,
        }
    }

    pub fn set_viewport(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        // Mark root dirty so layout is recomputed at new size.
        if let Some(root_id) = self.root_id {
            let _ = self.taffy.mark_dirty(root_id);
        }
    }

    /// Full rebuild from StyleTree. Call this when DOM structure changes.
    pub fn rebuild(&mut self, style_tree: &StyleTree) {
        self.taffy = TaffyTree::new();
        self.node_map.clear();
        self.root_id = None;

        let root_id = self.build_node(style_tree.root());
        self.root_id = Some(root_id);
    }

    /// Mark a DOM node dirty for layout recompute.
    /// Style-only changes: pass layout_dirty = false.
    /// Tree mutations: pass layout_dirty = true.
    pub fn mark_dirty(&mut self, node: &NodePtr, layout_dirty: bool) {
        let key = Rc::as_ptr(node) as usize;
        if let Some(&node_id) = self.node_map.get(&key) {
            let _ = self.taffy.mark_dirty(node_id);
            if layout_dirty {
                // Walk ancestors and mark them dirty too.
                self.mark_ancestors_dirty(node_id);
            }
        }
    }

    /// Compute (or recompute) layout. Taffy skips clean subtrees automatically.
    pub fn compute(&mut self, style_tree: &StyleTree) -> LayoutBox {
        let root_id = match self.root_id {
            Some(id) => id,
            None => {
                self.rebuild(style_tree);
                self.root_id.unwrap()
            }
        };

        let available = Size {
            width: AvailableSpace::Definite(self.viewport.width),
            height: AvailableSpace::Definite(self.viewport.height),
        };
        let _ = self.taffy.compute_layout(root_id, available);

        self.to_layout_box(style_tree.root(), 0.0, 0.0)
    }

    fn build_node(&mut self, styled_node: &crate::style::StyledNode) -> NodeId {
        let taffy_style = style_to_taffy(styled_node.styles());

        let children: Vec<NodeId> = styled_node
            .children()
            .iter()
            .map(|child| self.build_node(child))
            .collect();

        let node_id = self
            .taffy
            .new_with_children(taffy_style, &children)
            .expect("Failed to create taffy node");

        // Register the DOM node → NodeId mapping.
        let key = Rc::as_ptr(&styled_node.node) as usize;
        self.node_map.insert(key, node_id);

        node_id
    }

    fn mark_ancestors_dirty(&mut self, node_id: NodeId) {
        let mut current = node_id;
        loop {
            match self.taffy.parent(current) {
                Some(parent) => {
                    let _ = self.taffy.mark_dirty(parent);
                    current = parent;
                }
                None => break,
            }
        }
    }

    fn to_layout_box(
        &self,
        styled_node: &crate::style::StyledNode,
        parent_x: f32,
        parent_y: f32,
    ) -> LayoutBox {
        let key = Rc::as_ptr(&styled_node.node) as usize;
        let node_id = self.node_map.get(&key).copied();

        let (x, y, width, height) = if let Some(id) = node_id {
            if let Ok(layout) = self.taffy.layout(id) {
                (
                    parent_x + layout.location.x,
                    parent_y + layout.location.y,
                    layout.size.width,
                    layout.size.height,
                )
            } else {
                (parent_x, parent_y, 0.0, 0.0)
            }
        } else {
            (parent_x, parent_y, 0.0, 0.0)
        };

        let styles = styled_node.styles().clone();
        let margin = styles.margin();
        let border = styles.border_width();
        let padding = styles.padding();
        let kind = determine_layout_kind(styled_node);

        let children = styled_node
            .children()
            .iter()
            .map(|child| self.to_layout_box(child, x, y))
            .collect();

        LayoutBox {
            node: Some(styled_node.node.clone()),
            kind,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            styles,
            margin,
            border,
            padding,
            children,
        }
    }
}

fn determine_layout_kind(styled_node: &crate::style::StyledNode) -> LayoutKind {
    use crate::dom::Node;
    let node_borrow = styled_node.node.borrow();
    match &*node_borrow {
        Node::Element(el) => {
            let tag = el.tag_name.to_lowercase();
            if tag == "img" {
                LayoutKind::Image {
                    alt: el.attributes.get("alt").cloned(),
                    src: el.attributes.get("src").cloned(),
                    display_mode: styled_node.styles().display_mode(),
                }
            } else {
                LayoutKind::Block {
                    tag_name: el.tag_name.clone(),
                }
            }
        }
        Node::Text(text) => LayoutKind::Text { text: text.clone() },
        Node::Document { .. } => LayoutKind::Viewport,
    }
}
