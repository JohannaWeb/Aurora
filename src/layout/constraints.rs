use crate::css::{BoxSizing, StyleMap};

pub(in crate::layout) fn clamp_content_width(
    styles: &StyleMap,
    candidate_width: f32,
    available_width: f32,
) -> f32 {
    // Resolve width with support for %, rem, em
    let font_size = styles
        .font_size_resolved(16.0, 16.0)
        .or_else(|| styles.font_size_px())
        .unwrap_or(16.0);
    let mut width = styles
        .width_resolved(available_width, font_size, 16.0, 1200.0)
        .or_else(|| styles.width_px())
        .unwrap_or(candidate_width);
    if styles.box_sizing() == BoxSizing::BorderBox {
        let border = styles.border_width();
        let padding = styles.padding();
        width = (width - border.horizontal() - padding.horizontal()).max(0.0);
    }
    if let Some(min_width) = styles.min_width_px() {
        let mut min = min_width;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            min = (min - border.horizontal() - padding.horizontal()).max(0.0);
        }
        width = width.max(min);
    }
    if let Some(max_width) = styles.max_width_px() {
        let mut max = max_width;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            max = (max - border.horizontal() - padding.horizontal()).max(0.0);
        }
        width = width.min(max);
    }
    width.min(available_width).max(0.0)
}

pub(in crate::layout) fn clamp_content_height(
    styles: &StyleMap,
    candidate_height: f32,
    viewport_height: f32,
) -> f32 {
    // even though the overall pattern is similar.
    let font_size = styles
        .font_size_resolved(16.0, 16.0)
        .or_else(|| styles.font_size_px())
        .unwrap_or(16.0);
    let mut height = styles
        .height_resolved(candidate_height, font_size, 16.0, viewport_height)
        .or_else(|| styles.height_px())
        .unwrap_or(candidate_height);
    if styles.box_sizing() == BoxSizing::BorderBox {
        let border = styles.border_width();
        let padding = styles.padding();
        height = (height - border.vertical() - padding.vertical()).max(0.0);
    }
    if let Some(min_height) = styles
        .min_height_resolved(candidate_height, font_size, 16.0, viewport_height)
        .or_else(|| styles.min_height_px())
    {
        let mut min = min_height;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            min = (min - border.vertical() - padding.vertical()).max(0.0);
        }
        height = height.max(min);
    }
    if let Some(max_height) = styles
        .max_height_resolved(candidate_height, font_size, 16.0, viewport_height)
        .or_else(|| styles.max_height_px())
    {
        let mut max = max_height;
        if styles.box_sizing() == BoxSizing::BorderBox {
            let border = styles.border_width();
            let padding = styles.padding();
            max = (max - border.vertical() - padding.vertical()).max(0.0);
        }
        height = height.min(max);
    }
    height.max(0.0)
}

pub(in crate::layout) fn parse_html_length_px(value: &str) -> Option<f32> {
    // are accepted alongside explicit `px` suffixed values.
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .parse::<f32>()
        .ok()
}
