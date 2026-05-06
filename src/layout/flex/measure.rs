use crate::css::{DisplayMode, FlexDirection};
use crate::style::StyledNode;

use super::super::LayoutBox;
use super::types::{FlexContext, FlexMeasurement};

pub(super) fn measure_flex_children(
    children: &[StyledNode],
    content_width: f32,
    viewport_height: f32,
    ctx: &FlexContext,
) -> FlexMeasurement {
    let mut measured = FlexMeasurement {
        children: Vec::new(),
        total_width: 0.0,
        total_height: 0.0,
        max_height: 0.0,
    };

    for child in children {
        if child.tag_name() == Some("style".to_string())
            || child.tag_name() == Some("script".to_string())
            || child.styles().display_mode() == DisplayMode::None
        {
            continue;
        }

        let layout_child =
            if child.styles().get("width").is_some() || child.styles().max_width_px().is_some() {
                LayoutBox::from_styled_node(child, 0.0, 0.0, content_width, viewport_height)
            } else {
                measure_intrinsic_child(child, content_width, viewport_height)
            };

        if let Some(layout_child) = layout_child {
            measured.total_width += layout_child.total_width();
            measured.total_height += layout_child.total_height();
            measured.max_height = measured.max_height.max(layout_child.total_height());
            measured.children.push(layout_child);
        }
    }

    let item_count = measured.children.len() as f32;
    if ctx.direction == FlexDirection::Row && item_count > 1.0 && !ctx.wraps {
        measured.total_width += ctx.gap * (item_count - 1.0);
    }
    if ctx.direction == FlexDirection::Column && item_count > 1.0 {
        measured.total_height += ctx.gap * (item_count - 1.0);
    }

    measured
}

fn measure_intrinsic_child(
    child: &StyledNode,
    content_width: f32,
    viewport_height: f32,
) -> Option<LayoutBox> {
    let measured = LayoutBox::from_styled_node(child, 0.0, 0.0, 10000.0, viewport_height)?;
    let intrinsic = if measured.children.is_empty() {
        measured.rect.width + measured.padding.horizontal() + measured.border.horizontal()
    } else {
        let child_max = measured
            .children
            .iter()
            .map(|child| child.total_width())
            .fold(0.0_f32, f32::max);
        child_max + measured.padding.horizontal() + measured.border.horizontal()
    };
    LayoutBox::from_styled_node(
        child,
        0.0,
        0.0,
        intrinsic.min(content_width),
        viewport_height,
    )
}
