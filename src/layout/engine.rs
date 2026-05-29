//! Layout engine selection: Taffy for block/flex/grid trees, legacy for inline and
//! features not yet ported (margin collapse, replaced-element sizing).

use crate::css::{DisplayMode, MarginValue};
use crate::style::StyledNode;

use super::taffy_layout::compute_taffy_layout;
use super::{LayoutBox, ViewportSize};

/// Build the layout tree root using Taffy or the legacy engine as appropriate.
pub fn layout_root_from_style_tree(
    style_tree: &crate::style::StyleTree,
    viewport: ViewportSize,
) -> LayoutBox {
    if needs_legacy_layout(style_tree.root()) {
        LayoutBox::layout_root(style_tree.root(), viewport).expect("layout root must produce a box")
    } else {
        compute_taffy_layout(style_tree, viewport.width, viewport.height)
    }
}

fn needs_legacy_layout(node: &StyledNode) -> bool {
    if needs_legacy_for_node(node) {
        return true;
    }
    node.children().iter().any(needs_legacy_layout)
}

fn needs_legacy_for_node(node: &StyledNode) -> bool {
    if let Some(tag) = node.tag_name() {
        if matches!(
            tag.as_str(),
            "img" | "svg" | "canvas" | "iframe" | "textarea" | "input" | "button"
        ) {
            return true;
        }

        match node.styles().display_mode() {
            DisplayMode::Inline
            | DisplayMode::InlineBlock
            | DisplayMode::InlineFlex
            | DisplayMode::InlineGrid => return true,
            _ => {}
        }
    }

    if has_block_margin_collapse_scenario(node) {
        return true;
    }

    false
}

/// Legacy block layout collapses vertical margins between block-flow siblings; Taffy does not yet match.
fn has_block_margin_collapse_scenario(node: &StyledNode) -> bool {
    let block_children: Vec<&StyledNode> = node
        .children()
        .iter()
        .filter(|child| child.tag_name().is_some() && is_block_flow(child))
        .collect();

    if block_children.len() < 2 {
        return false;
    }

    block_children.iter().any(|child| {
        let margin = child.styles().margin();
        margin.top != MarginValue::Px(0.0) || margin.bottom != MarginValue::Px(0.0)
    })
}

fn is_block_flow(node: &StyledNode) -> bool {
    matches!(
        node.styles().display_mode(),
        DisplayMode::Block
            | DisplayMode::Flex
            | DisplayMode::Grid
            | DisplayMode::FlowRoot
            | DisplayMode::Table
            | DisplayMode::TableRow
            | DisplayMode::ListItem
    )
}
