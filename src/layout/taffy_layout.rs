use std::collections::HashMap;
use taffy::prelude::*;

use crate::css::StyleMap;
use crate::dom::Node;
use crate::style::{StyledNode, StyleTree};

use super::taffy_adapter::style_to_taffy;
use super::{LayoutBox, LayoutKind, Rect};

/// Per-node context stored in the Taffy tree.
/// Text and image leaf nodes carry their content so the measure function can run.
enum NodeContext {
    Text {
        text: String,
        styles: StyleMap,
    },
    Image {
        width_hint: Option<f32>,
        height_hint: Option<f32>,
    },
    Element,
}

/// Build a Taffy tree from the StyleTree and compute layout.
/// Uses `compute_layout_with_measure` so Taffy calls Parley for text sizing.
pub fn compute_taffy_layout(
    style_tree: &StyleTree,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutBox {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    let mut node_id_map = HashMap::new();

    let root_handle = build_taffy_tree(&mut taffy, style_tree.root(), &mut node_id_map);

    taffy
        .compute_layout_with_measure(
            root_handle,
            Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            },
            measure_text_node,
        )
        .expect("Taffy layout failed");

    convert_taffy_to_layout_box(&taffy, style_tree.root(), &node_id_map, 0.0, 0.0)
}

/// Taffy measure function — called for leaf nodes that need intrinsic sizing.
/// Runs Parley to measure text given the available width constraint.
fn measure_text_node(
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    _node_id: NodeId,
    node_context: Option<&mut NodeContext>,
    _style: &Style,
) -> Size<f32> {
    let Some(context) = node_context else {
        return Size::ZERO;
    };

    match context {
        NodeContext::Text { text, styles } => {
            if text.is_empty() {
                return Size::ZERO;
            }

            // Resolve available width for line-breaking.
            let available_width = match known_dimensions.width {
                Some(w) => w,
                None => match available_space.width {
                    AvailableSpace::Definite(w) => w,
                    AvailableSpace::MinContent => 0.0,
                    AvailableSpace::MaxContent => f32::INFINITY,
                },
            };

            // Run Parley to get the real measured size.
            let boxes = super::parley_text::layout_text_with_parley(
                None,
                text,
                styles,
                0.0,
                0.0,
                available_width,
            );

            if boxes.is_empty() {
                return Size::ZERO;
            }

            let width = boxes
                .iter()
                .map(|b| b.rect().width)
                .fold(0.0_f32, f32::max);
            let height = boxes.iter().map(|b| b.rect().height).sum::<f32>();

            Size {
                width: known_dimensions.width.unwrap_or(width),
                height: known_dimensions.height.unwrap_or(height),
            }
        }

        NodeContext::Image {
            width_hint,
            height_hint,
        } => Size {
            width: known_dimensions.width.or(*width_hint).unwrap_or(0.0),
            height: known_dimensions.height.or(*height_hint).unwrap_or(0.0),
        },

        NodeContext::Element => Size::ZERO,
    }
}

/// Recursively build a Taffy tree, registering text nodes with measure contexts.
fn build_taffy_tree(
    taffy: &mut TaffyTree<NodeContext>,
    styled_node: &StyledNode,
    node_id_map: &mut HashMap<*const StyledNode, NodeId>,
) -> NodeId {
    let taffy_style = style_to_taffy(styled_node.styles());
    let node_borrow = styled_node.node.borrow();

    let node_id = match &*node_borrow {
        // Text nodes — leaf with measure function.
        Node::Text(text) => {
            drop(node_borrow);
            let context = NodeContext::Text {
                text: text.clone(),
                styles: styled_node.styles().clone(),
            };
            taffy
                .new_leaf_with_context(taffy_style, context)
                .expect("Failed to create text leaf")
        }

        // Image nodes — leaf with optional size hints.
        Node::Element(el) if el.tag_name.eq_ignore_ascii_case("img") => {
            let width_hint = el
                .attributes
                .get("width")
                .and_then(|w| w.parse::<f32>().ok());
            let height_hint = el
                .attributes
                .get("height")
                .and_then(|h| h.parse::<f32>().ok());
            drop(node_borrow);
            let context = NodeContext::Image {
                width_hint,
                height_hint,
            };
            taffy
                .new_leaf_with_context(taffy_style, context)
                .expect("Failed to create image leaf")
        }

        // Element and document nodes — container with children.
        _ => {
            drop(node_borrow);
            let children: Vec<NodeId> = styled_node
                .children()
                .iter()
                .map(|child| build_taffy_tree(taffy, child, node_id_map))
                .collect();

            taffy
                .new_with_children(taffy_style, &children)
                .expect("Failed to create element node")
        }
    };

    node_id_map.insert(styled_node as *const StyledNode, node_id);
    node_id
}

/// Convert Taffy layout results into an Aurora LayoutBox tree.
fn convert_taffy_to_layout_box(
    taffy: &TaffyTree<NodeContext>,
    styled_node: &StyledNode,
    node_id_map: &HashMap<*const StyledNode, NodeId>,
    parent_x: f32,
    parent_y: f32,
) -> LayoutBox {
    let node_id = node_id_map
        .get(&(styled_node as *const StyledNode))
        .copied()
        .expect("StyledNode must have a Taffy NodeId");

    let layout = taffy.layout(node_id).expect("Node must have layout");
    let x = parent_x + layout.location.x;
    let y = parent_y + layout.location.y;
    let width = layout.size.width;
    let height = layout.size.height;

    let styles = styled_node.styles().clone();
    let margin = styles.margin();
    let border = styles.border_width();
    let padding = styles.padding();
    let kind = determine_layout_kind(styled_node);

    let children = styled_node
        .children()
        .iter()
        .map(|child| convert_taffy_to_layout_box(taffy, child, node_id_map, x, y))
        .collect();

    LayoutBox {
        node: Some(styled_node.node.clone()),
        kind,
        rect: Rect { x, y, width, height },
        styles,
        margin,
        border,
        padding,
        children,
    }
}

fn determine_layout_kind(styled_node: &StyledNode) -> LayoutKind {
    let node_borrow = styled_node.node.borrow();
    match &*node_borrow {
        Node::Element(el) => {
            let tag = el.tag_name.clone();
            if tag.eq_ignore_ascii_case("img") {
                LayoutKind::Image {
                    alt: el.attributes.get("alt").cloned(),
                    src: el.attributes.get("src").cloned(),
                    display_mode: styled_node.styles().display_mode(),
                }
            } else {
                LayoutKind::Block { tag_name: tag }
            }
        }
        Node::Text(text) => LayoutKind::Text { text: text.clone() },
        Node::Document { .. } => LayoutKind::Viewport,
    }
}
