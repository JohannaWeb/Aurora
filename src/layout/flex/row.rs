use crate::css::AlignItems;

use super::super::LayoutBox;
use super::spacing::justify_start_and_spacing;
use super::types::FlexContext;

pub(super) fn position_row(
    children: &mut [LayoutBox],
    total_child_width: f32,
    resolved_content_height: f32,
    ctx: &FlexContext,
) {
    let free_width = (ctx.content_width - total_child_width).max(0.0);
    let (mut current_x, spacing) = justify_start_and_spacing(
        ctx.justify,
        ctx.content_x,
        free_width,
        ctx.gap,
        children.len(),
    );

    for child in children {
        let new_x = current_x + child.margin.left.to_px();
        let new_y = match ctx.align {
            AlignItems::Center => {
                let free_y = (resolved_content_height - child.total_height()).max(0.0);
                ctx.content_y + free_y / 2.0 + child.margin.top
            }
            AlignItems::FlexEnd => {
                let free_y = (resolved_content_height - child.total_height()).max(0.0);
                ctx.content_y + free_y + child.margin.top
            }
            _ => ctx.content_y + child.margin.top,
        };

        child.offset(new_x - child.rect.x, new_y - child.rect.y);
        current_x += child.total_width() + spacing;
    }
}
