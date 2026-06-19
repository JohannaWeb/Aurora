use crate::css::{
    AlignItems, BoxSizing, DisplayMode, FlexDirection, JustifyContent, LengthValue, MarginValue,
    StyleMap, parse_length_value,
};
use taffy::prelude::{
    AlignContent as TaffyAlignContent, AlignItems as TaffyAlignItems, Dimension,
    Display as TaffyDisplay, FlexDirection as TaffyFlexDirection, FlexWrap, LengthPercentage,
    LengthPercentageAuto, Position, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle,
};

use super::ViewportSize;

#[cfg(test)]
fn style_to_taffy(styles: &StyleMap) -> TaffyStyle {
    style_to_taffy_with_viewport(
        styles,
        ViewportSize {
            width: 800.0,
            height: 600.0,
        },
    )
}

pub fn style_to_taffy_with_viewport(styles: &StyleMap, viewport: ViewportSize) -> TaffyStyle {
    let font_size = styles.font_size_px().unwrap_or(16.0);
    let mut taffy = TaffyStyle::default();
    taffy.display = taffy_display(styles.display_mode());
    taffy.position = taffy_position(styles.get("position"));
    taffy.size = TaffySize {
        width: box_dimension(styles.get("width"), styles, viewport, font_size, true),
        height: box_dimension(styles.get("height"), styles, viewport, font_size, false),
    };
    taffy.min_size = TaffySize {
        width: box_dimension(styles.get("min-width"), styles, viewport, font_size, true),
        height: box_dimension(styles.get("min-height"), styles, viewport, font_size, false),
    };
    taffy.max_size = TaffySize {
        width: box_max_dimension(styles.get("max-width"), styles, viewport, font_size, true),
        height: box_max_dimension(styles.get("max-height"), styles, viewport, font_size, false),
    };
    taffy.margin = taffy_margin(styles);
    taffy.padding = edge_lengths(styles.padding());
    taffy.border = edge_lengths(styles.border_width());
    taffy.flex_direction = match styles.flex_direction() {
        FlexDirection::Row => TaffyFlexDirection::Row,
        FlexDirection::Column => TaffyFlexDirection::Column,
    };
    taffy.flex_wrap = if styles.flex_wrap() {
        FlexWrap::Wrap
    } else {
        FlexWrap::NoWrap
    };
    taffy.justify_content = Some(match styles.justify_content() {
        JustifyContent::FlexStart => TaffyAlignContent::FlexStart,
        JustifyContent::Center => TaffyAlignContent::Center,
        JustifyContent::FlexEnd => TaffyAlignContent::FlexEnd,
        JustifyContent::SpaceBetween => TaffyAlignContent::SpaceBetween,
        JustifyContent::SpaceAround => TaffyAlignContent::SpaceAround,
    });
    taffy.align_items = Some(match styles.align_items() {
        AlignItems::Stretch => TaffyAlignItems::Stretch,
        AlignItems::FlexStart => TaffyAlignItems::FlexStart,
        AlignItems::Center => TaffyAlignItems::Center,
        AlignItems::FlexEnd => TaffyAlignItems::FlexEnd,
    });
    let gap = length_percentage(styles.get("gap")).unwrap_or(LengthPercentage::length(0.0));
    let column_gap = length_percentage(styles.get("column-gap")).unwrap_or(gap);
    let row_gap = length_percentage(styles.get("row-gap")).unwrap_or(gap);
    taffy.gap = TaffySize {
        width: column_gap,
        height: row_gap,
    };
    // Inset (left/right/top/bottom) drives placement of positioned boxes. Without
    // it, every absolutely-positioned element collapses to the container origin.
    taffy.inset = TaffyRect {
        left: inset_value(styles.get("left"), viewport, font_size),
        right: inset_value(styles.get("right"), viewport, font_size),
        top: inset_value(styles.get("top"), viewport, font_size),
        bottom: inset_value(styles.get("bottom"), viewport, font_size),
    };
    taffy
}

fn inset_value(
    value: Option<&str>,
    viewport: ViewportSize,
    font_size: f32,
) -> LengthPercentageAuto {
    let Some(value) = value.map(str::trim) else {
        return LengthPercentageAuto::auto();
    };
    if value == "auto" {
        return LengthPercentageAuto::auto();
    }
    if value == "0" {
        return LengthPercentageAuto::length(0.0);
    }
    if let Some(px) = value.strip_suffix("px") {
        if let Ok(px) = px.trim().parse::<f32>() {
            return LengthPercentageAuto::length(px);
        }
    }
    if let Some(percent) = value.strip_suffix('%') {
        if let Ok(p) = percent.trim().parse::<f32>() {
            return LengthPercentageAuto::percent(p / 100.0);
        }
    }
    parse_length_value(value)
        .map(|length: LengthValue| {
            LengthPercentageAuto::length(length.to_px(
                viewport.width,
                font_size,
                font_size,
                viewport.width,
                viewport.height,
            ))
        })
        .unwrap_or_else(LengthPercentageAuto::auto)
}

fn taffy_display(display: DisplayMode) -> TaffyDisplay {
    match display {
        DisplayMode::None => TaffyDisplay::None,
        DisplayMode::Flex | DisplayMode::InlineFlex => TaffyDisplay::Flex,
        DisplayMode::Grid | DisplayMode::InlineGrid => TaffyDisplay::Grid,
        _ => TaffyDisplay::Block,
    }
}

fn taffy_position(position: Option<&str>) -> Position {
    match position {
        Some("absolute") | Some("fixed") => Position::Absolute,
        _ => Position::Relative,
    }
}

fn taffy_margin(styles: &StyleMap) -> TaffyRect<LengthPercentageAuto> {
    let margin = styles.margin();
    TaffyRect {
        left: margin_value(margin.left),
        right: margin_value(margin.right),
        top: margin_value(margin.top),
        bottom: margin_value(margin.bottom),
    }
}

fn edge_lengths(edge: crate::css::EdgeSizes) -> TaffyRect<LengthPercentage> {
    TaffyRect {
        left: LengthPercentage::length(edge.left),
        right: LengthPercentage::length(edge.right),
        top: LengthPercentage::length(edge.top),
        bottom: LengthPercentage::length(edge.bottom),
    }
}

fn margin_value(value: MarginValue) -> LengthPercentageAuto {
    match value {
        MarginValue::Px(px) => LengthPercentageAuto::length(px),
        MarginValue::Auto => LengthPercentageAuto::auto(),
    }
}

fn box_dimension(
    value: Option<&str>,
    styles: &StyleMap,
    viewport: ViewportSize,
    font_size: f32,
    horizontal: bool,
) -> Dimension {
    let dim = dimension(value, viewport, font_size);
    expand_for_content_box(dim, styles, horizontal)
}

fn box_max_dimension(
    value: Option<&str>,
    styles: &StyleMap,
    viewport: ViewportSize,
    font_size: f32,
    horizontal: bool,
) -> Dimension {
    let dim = max_dimension(value, viewport, font_size);
    expand_for_content_box(dim, styles, horizontal)
}

/// Aurora's default is `content-box`: authored width/height are content sizes; Taffy expects border-box.
fn expand_for_content_box(dim: Dimension, styles: &StyleMap, horizontal: bool) -> Dimension {
    if styles.box_sizing() != BoxSizing::ContentBox {
        return dim;
    }
    if dim.is_auto() || dim.tag() != 1 {
        return dim; // not a plain length — auto or percent, leave as-is
    }
    let len = dim.value();
    let border = styles.border_width();
    let padding = styles.padding();
    let extra = if horizontal {
        border.horizontal() + padding.horizontal()
    } else {
        border.vertical() + padding.vertical()
    };
    Dimension::length(len + extra)
}

fn dimension(value: Option<&str>, viewport: ViewportSize, font_size: f32) -> Dimension {
    match value {
        Some("auto") | None => Dimension::auto(),
        Some(value) => length_dimension(value, viewport, font_size).unwrap_or(Dimension::auto()),
    }
}

fn max_dimension(value: Option<&str>, viewport: ViewportSize, font_size: f32) -> Dimension {
    match value {
        Some("none") | None => Dimension::auto(),
        Some(value) => length_dimension(value, viewport, font_size).unwrap_or(Dimension::auto()),
    }
}

fn length_dimension(value: &str, viewport: ViewportSize, font_size: f32) -> Option<Dimension> {
    let value = value.trim();
    if value == "0" {
        return Some(Dimension::length(0.0));
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(Dimension::length);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| Dimension::percent(value / 100.0));
    }
    parse_length_value(value).map(|length: LengthValue| {
        Dimension::length(length.to_px(
            viewport.width,
            font_size,
            font_size,
            viewport.width,
            viewport.height,
        ))
    })
}

fn length_percentage(value: Option<&str>) -> Option<LengthPercentage> {
    let value = value?.trim();
    if value == "0" {
        return Some(LengthPercentage::length(0.0));
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(LengthPercentage::length);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| LengthPercentage::percent(value / 100.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::Stylesheet;

    #[test]
    fn maps_block_grid_flex_and_none_display_values() {
        let css = Stylesheet::parse(
            "div { display: grid; width: 50%; height: 24px; margin-left: auto; }",
        );
        let styles = css.styles_for(
            &crate::css::ElementData {
                tag_name: "div".to_string(),
                attributes: Default::default(),
            },
            &[],
            &[],
            0,
        );
        let taffy = style_to_taffy(&styles);

        assert_eq!(taffy.display, TaffyDisplay::Grid);
        assert_eq!(taffy.size.width, Dimension::percent(0.5));
        assert_eq!(taffy.size.height, Dimension::length(24.0));
        assert_eq!(taffy.margin.left, LengthPercentageAuto::auto());
    }
}
