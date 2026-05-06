use crate::css::{AlignItems, StyleMap};

use super::super::constants::BLOCK_VERTICAL_PADDING;
use super::super::constraints::clamp_content_height;
use super::super::LayoutBox;
use super::spacing::justify_start_and_spacing;
use super::types::FlexContext;

pub(super) fn position_wrapped_rows(
    children: &mut [LayoutBox],
    styles: &StyleMap,
    viewport_height: f32,
    ctx: &FlexContext,
) -> f32 {
    let rows = collect_rows(children, ctx.content_width, ctx.gap);
    let row_heights = row_heights(children, &rows);
    let total_rows_height = total_rows_height(&row_heights, ctx.gap);
    let resolved_content_height = clamp_content_height(styles, total_rows_height, viewport_height)
        .max(BLOCK_VERTICAL_PADDING);

    let mut current_y = ctx.content_y;
    for (row_index, row) in rows.iter().enumerate() {
        let row_width = row_width(children, row, ctx.gap);
        let free_width = (ctx.content_width - row_width).max(0.0);
        let (mut current_x, spacing) =
            justify_start_and_spacing(ctx.justify, ctx.content_x, free_width, ctx.gap, row.len());

        for index in row {
            let child = &mut children[*index];
            let new_x = current_x + child.margin.left.to_px();
            let new_y = match ctx.align {
                AlignItems::Center => {
                    let free_y = (row_heights[row_index] - child.total_height()).max(0.0);
                    current_y + free_y / 2.0 + child.margin.top
                }
                AlignItems::FlexEnd => {
                    let free_y = (row_heights[row_index] - child.total_height()).max(0.0);
                    current_y + free_y + child.margin.top
                }
                _ => current_y + child.margin.top,
            };

            child.offset(new_x - child.rect.x, new_y - child.rect.y);
            current_x += child.total_width() + spacing;
        }

        current_y += row_heights[row_index] + ctx.gap;
    }

    resolved_content_height
}

fn collect_rows(children: &[LayoutBox], content_width: f32, gap: f32) -> Vec<Vec<usize>> {
    let mut rows = Vec::new();
    let mut current_row = Vec::new();
    let mut current_row_width = 0.0;

    for (index, child) in children.iter().enumerate() {
        let child_width = child.total_width();
        let proposed = if current_row.is_empty() {
            child_width
        } else {
            current_row_width + gap + child_width
        };

        if !current_row.is_empty() && proposed > content_width {
            rows.push(current_row);
            current_row = vec![index];
            current_row_width = child_width;
        } else {
            current_row_width = proposed;
            current_row.push(index);
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }
    rows
}

fn row_heights(children: &[LayoutBox], rows: &[Vec<usize>]) -> Vec<f32> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|index| children[*index].total_height())
                .fold(0.0_f32, f32::max)
        })
        .collect()
}

fn total_rows_height(row_heights: &[f32], gap: f32) -> f32 {
    let mut total = row_heights.iter().sum::<f32>();
    if row_heights.len() > 1 {
        total += gap * (row_heights.len() as f32 - 1.0);
    }
    total
}

fn row_width(children: &[LayoutBox], row: &[usize], gap: f32) -> f32 {
    row.iter()
        .enumerate()
        .map(|(i, index)| children[*index].total_width() + if i > 0 { gap } else { 0.0 })
        .sum()
}
