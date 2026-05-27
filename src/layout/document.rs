use std::collections::HashMap;
use std::rc::Rc;
use taffy::prelude::*;

use crate::dom::{Node, NodePtr};
use crate::style::StyleTree;

use super::taffy_adapter::style_to_taffy_with_viewport;
use super::text_metrics::{font_size_from_styles, line_height_from_styles};
use super::{LayoutBox, LayoutKind, Rect, ViewportSize};

struct TextContext {
    text: String,
    font_size: f32,
    line_height: f32,
}

/// Owns the Taffy tree across reflows.
/// Taffy skips clean subtrees on recompute — dirty bits propagate up from JS mutations.
pub struct LayoutDocument {
    taffy: TaffyTree<TextContext>,
    node_map: HashMap<usize, NodeId>,
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
        if let Some(root_id) = self.root_id {
            let _ = self.taffy.mark_dirty(root_id);
        }
    }

    /// Mark a DOM node dirty. layout_dirty=true also marks ancestors.
    pub fn mark_dirty(&mut self, node: &NodePtr, layout_dirty: bool) {
        let key = Rc::as_ptr(node) as usize;
        if let Some(&node_id) = self.node_map.get(&key) {
            let _ = self.taffy.mark_dirty(node_id);
            if layout_dirty {
                let mut current = node_id;
                while let Some(parent) = self.taffy.parent(current) {
                    let _ = self.taffy.mark_dirty(parent);
                    current = parent;
                }
            }
        }
    }

    /// Full rebuild. Call when DOM structure changes (appendChild, removeChild, innerHTML).
    pub fn rebuild(&mut self, style_tree: &StyleTree) {
        self.taffy = TaffyTree::new();
        self.node_map.clear();
        self.root_id = None;
        if let Some(root_id) = self.build_node(style_tree.root()) {
            self.root_id = Some(root_id);
        }
    }

    /// Incremental recompute. Taffy only re-layouts dirty subtrees.
    /// Falls back to full rebuild if the tree hasn't been built yet.
    pub fn compute(&mut self, style_tree: &StyleTree) -> LayoutBox {
        if self.root_id.is_none() {
            self.rebuild(style_tree);
        }

        let root_id = match self.root_id {
            Some(id) => id,
            None => return LayoutBox {
                node: None,
                kind: LayoutKind::Viewport,
                rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
                styles: Default::default(),
                margin: crate::css::Margin::zero(),
                border: crate::css::EdgeSizes::zero(),
                padding: crate::css::EdgeSizes::zero(),
                children: Vec::new(),
            },
        };

        let viewport = self.viewport;
        self.taffy
            .compute_layout_with_measure(
                root_id,
                Size {
                    width: AvailableSpace::Definite(viewport.width),
                    height: AvailableSpace::Definite(viewport.height),
                },
                |known, available, _id, ctx, _style| measure_text(known, available, ctx),
            )
            .expect("Taffy layout failed");

        self.to_layout_box(style_tree.root(), 0.0, 0.0)
    }

    fn build_node(&mut self, styled_node: &crate::style::StyledNode) -> Option<NodeId> {
        use crate::css::DisplayMode;

        if styled_node.styles().display_mode() == DisplayMode::None {
            return None;
        }
        if let Some(tag) = styled_node.tag_name() {
            if matches!(tag.as_str(), "style" | "script") {
                return None;
            }
        }

        let mut taffy_style = style_to_taffy_with_viewport(styled_node.styles(), self.viewport);
        let node_borrow = styled_node.node.borrow();

        if let Node::Text(text) = &*node_borrow {
            let font_size = font_size_from_styles(styled_node.styles());
            let line_height = line_height_from_styles(styled_node.styles());
            taffy_style.size.width = Dimension::auto();
            taffy_style.size.height = Dimension::auto();
            taffy_style.align_self = Some(AlignSelf::FlexStart);
            let ctx = TextContext {
                text: text.clone(),
                font_size,
                line_height,
            };
            drop(node_borrow);
            let node_id = self
                .taffy
                .new_leaf_with_context(taffy_style, ctx)
                .expect("Failed to create text leaf");
            self.node_map
                .insert(Rc::as_ptr(&styled_node.node) as usize, node_id);
            return Some(node_id);
        }
        drop(node_borrow);

        let children: Vec<NodeId> = styled_node
            .children()
            .iter()
            .filter_map(|child| self.build_node(child))
            .collect();

        let node_id = self
            .taffy
            .new_with_children(taffy_style, &children)
            .expect("Failed to create element node");
        self.node_map
            .insert(Rc::as_ptr(&styled_node.node) as usize, node_id);
        Some(node_id)
    }

    fn to_layout_box(
        &self,
        styled_node: &crate::style::StyledNode,
        parent_x: f32,
        parent_y: f32,
    ) -> LayoutBox {
        let key = Rc::as_ptr(&styled_node.node) as usize;

        let (x, y, width, height) = match self.node_map.get(&key).and_then(|&id| self.taffy.layout(id).ok()) {
            Some(layout) => (
                parent_x + layout.location.x,
                parent_y + layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
            None => (parent_x, parent_y, 0.0, 0.0),
        };

        let styles = styled_node.styles().clone();
        let kind = determine_kind(styled_node);
        let children = styled_node
            .children()
            .iter()
            .filter(|child| !should_skip(child))
            .map(|child| self.to_layout_box(child, x, y))
            .collect();

        LayoutBox {
            node: Some(styled_node.node.clone()),
            kind,
            rect: Rect { x, y, width, height },
            styles: styles.clone(),
            margin: styles.margin(),
            border: styles.border_width(),
            padding: styles.padding(),
            children,
        }
    }
}

fn measure_text(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    ctx: Option<&mut TextContext>,
) -> Size<f32> {
    let ctx = match ctx {
        Some(c) => c,
        None => return Size::ZERO,
    };

    if let Some(w) = known.width {
        return Size {
            width: w,
            height: known.height.unwrap_or(ctx.line_height),
        };
    }

    let available_width = match available.width {
        AvailableSpace::Definite(w) => w,
        _ => f32::MAX,
    };

    let intrinsic = crate::font::measure_text(&ctx.text, ctx.font_size);
    if intrinsic <= available_width {
        return Size {
            width: intrinsic,
            height: ctx.line_height,
        };
    }

    // Word-wrap measurement.
    let words: Vec<&str> = ctx.text.split_whitespace().collect();
    let mut line_w = 0.0_f32;
    let mut max_w = 0.0_f32;
    let mut lines = 1u32;
    let space_w = ctx.font_size * 0.3;

    for word in &words {
        let word_w = word.chars().count() as f32 * ctx.font_size;
        if line_w > 0.0 && line_w + space_w + word_w > available_width {
            max_w = max_w.max(line_w);
            line_w = word_w;
            lines += 1;
        } else {
            if line_w > 0.0 {
                line_w += space_w;
            }
            line_w += word_w;
        }
    }
    max_w = max_w.max(line_w);

    Size {
        width: max_w.min(available_width),
        height: ctx.line_height * lines as f32,
    }
}

fn should_skip(styled_node: &crate::style::StyledNode) -> bool {
    use crate::css::DisplayMode;
    if styled_node.styles().display_mode() == DisplayMode::None {
        return true;
    }
    if let Some(tag) = styled_node.tag_name() {
        if matches!(tag.as_str(), "style" | "script") {
            return true;
        }
    }
    false
}

fn determine_kind(styled_node: &crate::style::StyledNode) -> LayoutKind {
    let node_borrow = styled_node.node.borrow();
    match &*node_borrow {
        Node::Element(el) => {
            if el.tag_name.eq_ignore_ascii_case("img") {
                LayoutKind::Image {
                    alt: el.attributes.get("alt").cloned(),
                    src: el.attributes.get("src").cloned(),
                    display_mode: styled_node.styles().display_mode(),
                }
            } else if el.tag_name.eq_ignore_ascii_case("video") {
                LayoutKind::Media {
                    src: el.attributes.get("src").cloned(),
                    poster: el.attributes.get("poster").cloned(),
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
