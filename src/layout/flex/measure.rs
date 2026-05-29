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
    let intrinsic = max_content_width(&measured).min(10000.0);
    LayoutBox::from_styled_node(
        child,
        0.0,
        0.0,
        intrinsic.min(content_width),
        viewport_height,
    )
}

/// Recursively find the max-content width of a layout box.
///
/// Regular block boxes expand to fill available width, so their `rect.width` is not
/// a useful intrinsic measurement. We recurse into their children instead.
/// Anonymous inline wrappers and true inline/text/image boxes already carry the
/// correct content width in `rect.width`.
fn max_content_width(b: &LayoutBox) -> f32 {
    use super::super::LayoutKind;
    let is_stretched_block = matches!(&b.kind, LayoutKind::Block { tag_name }
        if tag_name != "anonymous-inline");

    let inner = if is_stretched_block && !b.children.is_empty() {
        let is_row_container = match b.styles.display_mode() {
            DisplayMode::TableRow => true,
            DisplayMode::Flex => {
                b.styles.flex_direction() == FlexDirection::Row && !b.styles.flex_wrap()
            }
            _ => false,
        };

        if is_row_container {
            let gap = b.styles.gap_px();
            let n = b.children.len();
            let sum: f32 = b.children.iter().map(max_content_width).sum();
            sum + if n > 1 { gap * (n as f32 - 1.0) } else { 0.0 }
        } else {
            b.children
                .iter()
                .map(max_content_width)
                .fold(0.0_f32, f32::max)
        }
    } else if is_stretched_block {
        // Empty block: stretched rect.width is not a useful intrinsic measure.
        // Use the CSS-explicit width (or min-width floor), falling back to 0.
        b.styles
            .width_px()
            .unwrap_or_else(|| b.styles.min_width_px().unwrap_or(0.0))
    } else {
        b.rect.width
    };

    inner + b.margin.horizontal() + b.border.horizontal() + b.padding.horizontal()
}
