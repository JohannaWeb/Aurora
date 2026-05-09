use crate::css::{AlignItems, DisplayMode, FlexDirection, JustifyContent, MarginValue, StyleMap};
use taffy::prelude::{
    AlignContent as TaffyAlignContent, AlignItems as TaffyAlignItems, Dimension,
    Display as TaffyDisplay, FlexDirection as TaffyFlexDirection, FlexWrap, LengthPercentage,
    LengthPercentageAuto, Position, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle,
};

pub fn style_to_taffy(styles: &StyleMap) -> TaffyStyle {
    let mut taffy = TaffyStyle::default();
    taffy.display = taffy_display(styles.display_mode());
    taffy.position = taffy_position(styles.get("position"));
    taffy.size = TaffySize {
        width: dimension(styles.get("width")),
        height: dimension(styles.get("height")),
    };
    taffy.min_size = TaffySize {
        width: dimension(styles.get("min-width")),
        height: dimension(styles.get("min-height")),
    };
    taffy.max_size = TaffySize {
        width: max_dimension(styles.get("max-width")),
        height: max_dimension(styles.get("max-height")),
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
    let gap = length_percentage(styles.get("gap")).unwrap_or(LengthPercentage::Length(0.0));
    let column_gap = length_percentage(styles.get("column-gap")).unwrap_or(gap);
    let row_gap = length_percentage(styles.get("row-gap")).unwrap_or(gap);
    taffy.gap = TaffySize {
        width: column_gap,
        height: row_gap,
    };
    taffy
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
        left: LengthPercentage::Length(edge.left),
        right: LengthPercentage::Length(edge.right),
        top: LengthPercentage::Length(edge.top),
        bottom: LengthPercentage::Length(edge.bottom),
    }
}

fn margin_value(value: MarginValue) -> LengthPercentageAuto {
    match value {
        MarginValue::Px(px) => LengthPercentageAuto::Length(px),
        MarginValue::Auto => LengthPercentageAuto::Auto,
    }
}

fn dimension(value: Option<&str>) -> Dimension {
    match value {
        Some("auto") | None => Dimension::Auto,
        Some(value) => length_dimension(value).unwrap_or(Dimension::Auto),
    }
}

fn max_dimension(value: Option<&str>) -> Dimension {
    match value {
        Some("none") | None => Dimension::Auto,
        Some(value) => length_dimension(value).unwrap_or(Dimension::Auto),
    }
}

fn length_dimension(value: &str) -> Option<Dimension> {
    let value = value.trim();
    if value == "0" {
        return Some(Dimension::Length(0.0));
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(Dimension::Length);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| Dimension::Percent(value / 100.0));
    }
    None
}

fn length_percentage(value: Option<&str>) -> Option<LengthPercentage> {
    let value = value?.trim();
    if value == "0" {
        return Some(LengthPercentage::Length(0.0));
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(LengthPercentage::Length);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| LengthPercentage::Percent(value / 100.0));
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
        );
        let taffy = style_to_taffy(&styles);

        assert_eq!(taffy.display, TaffyDisplay::Grid);
        assert_eq!(taffy.size.width, Dimension::Percent(0.5));
        assert_eq!(taffy.size.height, Dimension::Length(24.0));
        assert_eq!(taffy.margin.left, LengthPercentageAuto::Auto);
    }
}
