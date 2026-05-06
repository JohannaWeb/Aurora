use crate::css::{AlignItems, EdgeSizes, FlexDirection, JustifyContent, StyleMap};

use super::super::LayoutBox;

pub(super) struct FlexContext {
    pub(super) content_width: f32,
    pub(super) content_x: f32,
    pub(super) content_y: f32,
    pub(super) direction: FlexDirection,
    pub(super) justify: JustifyContent,
    pub(super) align: AlignItems,
    pub(super) gap: f32,
    pub(super) wraps: bool,
}

impl FlexContext {
    pub(super) fn from_styles(
        styles: &StyleMap,
        content_width: f32,
        rect_x: f32,
        rect_y: f32,
        border: EdgeSizes,
        padding: EdgeSizes,
    ) -> Self {
        Self {
            content_width,
            content_x: rect_x + border.left + padding.left,
            content_y: rect_y + border.top + padding.top,
            direction: styles.flex_direction(),
            justify: styles.justify_content(),
            align: styles.align_items(),
            gap: styles.gap_px(),
            wraps: styles.flex_wrap(),
        }
    }
}

pub(super) struct FlexMeasurement {
    pub(super) children: Vec<LayoutBox>,
    pub(super) total_width: f32,
    pub(super) total_height: f32,
    pub(super) max_height: f32,
}

impl FlexMeasurement {
    pub(super) fn inner_height(&self, ctx: &FlexContext) -> f32 {
        if ctx.direction == FlexDirection::Row && ctx.wraps {
            0.0
        } else if ctx.direction == FlexDirection::Row {
            self.max_height
        } else {
            self.total_height
        }
    }
}
