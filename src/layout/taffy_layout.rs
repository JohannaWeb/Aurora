use taffy::prelude::*;
use std::collections::HashMap;

use crate::style::{StyledNode, StyleTree};
use crate::dom::Node;

use super::{LayoutBox, LayoutKind, Rect};
use super::taffy_adapter::style_to_taffy;
use super::text_metrics::{font_size_from_styles, line_height_from_styles};

/// Per-node context stored in the Taffy tree.
/// For text leaf nodes we keep the text and font metrics so the measure
/// function can return an accurate intrinsic size.
struct TextContext {
    text: String,
    font_size: f32,
    line_height: f32,
}

/// Build a Taffy tree from the StyleTree and compute layout.
pub fn compute_taffy_layout(
    style_tree: &StyleTree,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutBox {
    let mut taffy: TaffyTree<TextContext> = TaffyTree::new();
    let mut node_id_map = HashMap::new();

    let root_handle = build_taffy_tree(&mut taffy, style_tree.root(), &mut node_id_map);

    taffy
        .compute_layout_with_measure(
            root_handle,
            Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            },
            |known_dimensions, available_space, _node_id, context, _style| {
                measure_text_node(known_dimensions, available_space, context)
            },
        )
        .expect("Taffy layout failed");

    convert_taffy_to_layout_box(&taffy, style_tree.root(), &node_id_map, 0.0, 0.0)
}

/// Measure function called by Taffy for leaf (text) nodes.
fn measure_text_node(
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    context: Option<&mut TextContext>,
) -> Size<f32> {
    let ctx = match context {
        Some(c) => c,
        None => return Size::ZERO,
    };

    let height = known_dimensions.height.unwrap_or(ctx.line_height);

    let available_width = match available_space.width {
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MaxContent | AvailableSpace::MinContent => f32::MAX,
    };

    if let Some(w) = known_dimensions.width {
        return Size { width: w, height };
    }

    // Word-wrap the text into lines and sum up the occupied height.
    let char_width = ctx.font_size;
    let words: Vec<&str> = ctx.text.split_whitespace().collect();
    if words.is_empty() {
        return Size::ZERO;
    }

    let mut line_width: f32 = 0.0;
    let mut max_width: f32 = 0.0;
    let mut lines: u32 = 1;
    let space_w = char_width * 0.3;

    for word in &words {
        let word_w = word.chars().count() as f32 * char_width;
        if line_width > 0.0 && line_width + space_w + word_w > available_width {
            max_width = max_width.max(line_width);
            line_width = word_w;
            lines += 1;
        } else {
            if line_width > 0.0 {
                line_width += space_w;
            }
            line_width += word_w;
        }
    }
    max_width = max_width.max(line_width);

    Size {
        width: max_width.min(available_width),
        height: ctx.line_height * lines as f32,
    }
}

/// Recursively build a Taffy tree, returning the root handle.
fn build_taffy_tree(
    taffy: &mut TaffyTree<TextContext>,
    styled_node: &StyledNode,
    node_id_map: &mut HashMap<*const StyledNode, NodeId>,
) -> NodeId {
    let taffy_style = style_to_taffy(styled_node.styles());

    // Text nodes are Taffy leaf nodes with a measure context.
    if styled_node.tag_name().is_none() {
        if let Some(text) = styled_node.text() {
            let font_size = font_size_from_styles(styled_node.styles());
            let line_height = line_height_from_styles(styled_node.styles());
            let ctx = TextContext { text, font_size, line_height };
            let node_id = taffy
                .new_leaf_with_context(taffy_style, ctx)
                .expect("Failed to create text leaf");
            node_id_map.insert(styled_node as *const StyledNode, node_id);
            return node_id;
        }
    }

    // Element nodes: recurse into children.
    let mut children = Vec::new();
    for child in styled_node.children() {
        let child_handle = build_taffy_tree(taffy, child, node_id_map);
        children.push(child_handle);
    }

    let node_id = taffy
        .new_with_children(taffy_style, &children)
        .expect("Failed to create taffy node");
    node_id_map.insert(styled_node as *const StyledNode, node_id);
    node_id
}

/// Convert Taffy layout results into an Aurora LayoutBox tree.
fn convert_taffy_to_layout_box(
    taffy: &TaffyTree<TextContext>,
    styled_node: &StyledNode,
    node_id_map: &HashMap<*const StyledNode, NodeId>,
    parent_offset_x: f32,
    parent_offset_y: f32,
) -> LayoutBox {
    let styled_node_ptr = styled_node as *const StyledNode;
    let node_id = node_id_map
        .get(&styled_node_ptr)
        .copied()
        .expect("StyledNode must have a corresponding Taffy NodeId");

    let layout = taffy.layout(node_id).expect("Node must have layout");

    let x = parent_offset_x + layout.location.x;
    let y = parent_offset_y + layout.location.y;
    let width = layout.size.width;
    let height = layout.size.height;

    let kind = determine_layout_kind(styled_node);
    let node = Some(styled_node.node.clone());
    let styles = styled_node.styles().clone();
    let margin = styles.margin();
    let border = styles.border_width();
    let padding = styles.padding();

    let mut children = Vec::new();
    for child_styled_node in styled_node.children() {
        let child_layout_box = convert_taffy_to_layout_box(
            taffy,
            child_styled_node,
            node_id_map,
            x,
            y,
        );
        children.push(child_layout_box);
    }

    LayoutBox {
        node,
        kind,
        rect: Rect { x, y, width, height },
        styles,
        margin,
        border,
        padding,
        children,
    }
}

/// Determine the LayoutKind from a StyledNode.
fn determine_layout_kind(styled_node: &StyledNode) -> LayoutKind {
    let node_borrow = styled_node.node.borrow();
    match &*node_borrow {
        Node::Element(el) => {
            let tag_name = el.tag_name.clone();
            if tag_name.eq_ignore_ascii_case("img") {
                let alt = el.attributes.get("alt").cloned();
                let src = el.attributes.get("src").cloned();
                let display_mode = styled_node.styles().display_mode();
                LayoutKind::Image { alt, src, display_mode }
            } else {
                LayoutKind::Block { tag_name }
            }
        }
        Node::Text(text) => LayoutKind::Text { text: text.clone() },
        Node::Document { .. } => LayoutKind::Viewport,
    }
}
