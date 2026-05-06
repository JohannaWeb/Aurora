use super::length::parse_length_px;
use super::{EdgeSizes, Margin, MarginValue};

pub(super) fn parse_margin_shorthand(value: Option<&str>) -> Margin {
    let Some(value) = value else {
        return Margin::zero();
    };

    let parts = value.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [all] => {
            let val = parse_margin_value(all);
            Margin {
                top: val.to_px(),
                right: val,
                bottom: val.to_px(),
                left: val,
            }
        }
        [vertical, horizontal] => {
            let v = parse_margin_value(vertical);
            let h = parse_margin_value(horizontal);
            Margin {
                top: v.to_px(),
                right: h,
                bottom: v.to_px(),
                left: h,
            }
        }
        [top, horizontal, bottom] => {
            let t = parse_margin_value(top);
            let h = parse_margin_value(horizontal);
            let b = parse_margin_value(bottom);
            Margin {
                top: t.to_px(),
                right: h,
                bottom: b.to_px(),
                left: h,
            }
        }
        [top, right, bottom, left] => {
            let t = parse_margin_value(top);
            let r = parse_margin_value(right);
            let b = parse_margin_value(bottom);
            let l = parse_margin_value(left);
            Margin {
                top: t.to_px(),
                right: r,
                bottom: b.to_px(),
                left: l,
            }
        }
        _ => Margin::zero(),
    }
}

pub(super) fn parse_margin_value(value: &str) -> MarginValue {
    if value == "auto" {
        MarginValue::Auto
    } else {
        MarginValue::Px(parse_length_px(value).unwrap_or(0.0))
    }
}

pub(super) fn parse_box_shorthand(value: Option<&str>) -> EdgeSizes {
    let Some(value) = value else {
        return EdgeSizes::zero();
    };

    let parts = value
        .split_whitespace()
        .filter_map(parse_length_px)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [all] => EdgeSizes {
            top: *all,
            right: *all,
            bottom: *all,
            left: *all,
        },
        [vertical, horizontal] => EdgeSizes {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => EdgeSizes {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left] => EdgeSizes {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        _ => EdgeSizes::zero(),
    }
}

pub(super) fn parse_border_width_shorthand(value: Option<&str>) -> EdgeSizes {
    let Some(value) = value else {
        return EdgeSizes::zero();
    };

    match value.split_whitespace().find_map(parse_length_px) {
        Some(width) => EdgeSizes {
            top: width,
            right: width,
            bottom: width,
            left: width,
        },
        None => EdgeSizes::zero(),
    }
}

pub(super) fn parse_border_color_shorthand(value: Option<&str>) -> Option<&str> {
    value?
        .split_whitespace()
        .find(|part| parse_length_px(part).is_none() && *part != "solid")
}
