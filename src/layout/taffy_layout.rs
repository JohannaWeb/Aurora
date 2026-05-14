use taffy::prelude::*;
use std::collections::HashMap;

use crate::style::{StyledNode, StyleTree};
use crate::dom::Node;

use super::{LayoutBox, LayoutKind, Rect};
use super::taffy_adapter::style_to_taffy;

/// Build a Taffy tree from the StyleTree and compute layout.
pub fn compute_taffy_layout(
    style_tree: &StyleTree,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutBox {
    let mut taffy = TaffyTree::new();
    let mut node_id_map = HashMap::new();

    // Build the Taffy tree from the StyleTree.
    let root_handle = build_taffy_tree(&mut taffy, style_tree.root(), &mut node_id_map);

    // Compute layout with the viewport size as available space.
    let _result = taffy.compute_layout(
        root_handle,
        Size {
            width: AvailableSpace::Definite(viewport_width),
            height: AvailableSpace::Definite(viewport_height),
        },
    );

    // Convert the Taffy tree back into LayoutBox tree.
    convert_taffy_to_layout_box(&taffy, style_tree.root(), &node_id_map, 0.0, 0.0)
}

/// Recursively build a Taffy tree, returning the root handle.
/// Also populates node_id_map to track which StyledNode corresponds to which NodeId.
fn build_taffy_tree(
    taffy: &mut TaffyTree<()>,
    styled_node: &StyledNode,
    node_id_map: &mut HashMap<*const StyledNode, NodeId>,
) -> NodeId {
    // Convert Aurora's styles to Taffy style.
    let taffy_style = style_to_taffy(styled_node.styles());

    // Recursively build children.
    let mut children = Vec::new();
    for child in styled_node.children() {
        let child_handle = build_taffy_tree(taffy, child, node_id_map);
        children.push(child_handle);
    }

    // Create the node in Taffy.
    let node_id = taffy
        .new_with_children(taffy_style, &children)
        .expect("Failed to create taffy node");

    // Store the mapping from StyledNode pointer to NodeId.
    let styled_node_ptr = styled_node as *const StyledNode;
    node_id_map.insert(styled_node_ptr, node_id);

    node_id
}

/// Convert Taffy layout results into an Aurora LayoutBox tree.
fn convert_taffy_to_layout_box(
    taffy: &TaffyTree<()>,
    styled_node: &StyledNode,
    node_id_map: &HashMap<*const StyledNode, NodeId>,
    parent_offset_x: f32,
    parent_offset_y: f32,
) -> LayoutBox {
    // Look up this node's Taffy NodeId.
    let styled_node_ptr = styled_node as *const StyledNode;
    let node_id = node_id_map
        .get(&styled_node_ptr)
        .copied()
        .expect("StyledNode must have a corresponding Taffy NodeId");

    // Get the computed layout for this node from Taffy.
    let layout = taffy.layout(node_id).expect("Node must have layout");

    // Extract position and size from Taffy.
    let x = parent_offset_x + layout.location.x;
    let y = parent_offset_y + layout.location.y;
    let width = layout.size.width;
    let height = layout.size.height;

    // Determine the LayoutKind from the StyledNode.
    let kind = determine_layout_kind(styled_node);

    // Get the DOM node.
    let node = Some(styled_node.node.clone());

    // Get computed styles.
    let styles = styled_node.styles().clone();

    // Extract margin, border, padding from styles.
    let margin = styles.margin();
    let border = styles.border_width();
    let padding = styles.padding();

    // Recursively convert children.
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
                LayoutKind::Image {
                    alt,
                    src,
                    display_mode,
                }
            } else {
                LayoutKind::Block { tag_name }
            }
        }
        Node::Text(text) => {
            LayoutKind::Text { text: text.clone() }
        }
        Node::Document { .. } => {
            LayoutKind::Viewport
        }
    }
}
