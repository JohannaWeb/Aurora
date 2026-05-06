use crate::css::AlignItems;

use super::super::LayoutBox;
use super::spacing::justify_start_and_spacing;
use super::types::FlexContext;

pub(super) fn position_column(
    children: &mut [LayoutBox],
    total_child_height: f32,
    resolved_content_height: f32,
    ctx: &FlexContext,
) {
    let free_height = (resolved_content_height - total_child_height).max(0.0);
    let (mut current_y, spacing) = justify_start_and_spacing(
        ctx.justify,
        ctx.content_y,
        free_height,
        ctx.gap,
        children.len(),
    );

    for child in children {
        let new_y = current_y + child.margin.top;
        let new_x = match ctx.align {
            AlignItems::Center => {
                let free_w = (ctx.content_width - child.total_width()).max(0.0);
                ctx.content_x + free_w / 2.0 + child.margin.left.to_px()
            }
            AlignItems::FlexEnd => {
                let free_w = (ctx.content_width - child.total_width()).max(0.0);
                ctx.content_x + free_w + child.margin.left.to_px()
            }
            _ => ctx.content_x + child.margin.left.to_px(),
        };

        child.offset(new_x - child.rect.x, new_y - child.rect.y);
        current_y += child.total_height() + spacing;
    }
}
