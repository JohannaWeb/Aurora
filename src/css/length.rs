#[derive(Debug, Clone, Copy)]
pub enum LengthValue {
    Px(f32),
    Percent(f32),
    Rem(f32),
    Em(f32),
    Ch(f32),
    Ex(f32),
    Vh(f32),
    Vw(f32),
    VMin(f32),
    VMax(f32),
    Svh(f32),
    Lvh(f32),
    Dvh(f32),
    /// CSS grid fraction unit — cannot be resolved outside a grid container; returns 0.
    Fr,
    /// Line-height relative — approximated as font_size (no computed line-height available here).
    Lh(f32),
    /// Root line-height relative — approximated as root_font_size.
    Rlh(f32),
}

impl LengthValue {
    pub fn to_px(
        self,
        available: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> f32 {
        match self {
            LengthValue::Px(v) => v,
            LengthValue::Percent(v) => available * v / 100.0,
            LengthValue::Rem(v) => root_font_size * v,
            LengthValue::Em(v) => font_size * v,
            LengthValue::Ch(v) => font_size * 0.5 * v,
            LengthValue::Ex(v) => font_size * 0.5 * v,
            LengthValue::Vw(v) => viewport_width * v / 100.0,
            LengthValue::Vh(v) => viewport_height * v / 100.0,
            LengthValue::VMin(v) => viewport_width.min(viewport_height) * v / 100.0,
            LengthValue::VMax(v) => viewport_width.max(viewport_height) * v / 100.0,
            LengthValue::Svh(v) | LengthValue::Lvh(v) | LengthValue::Dvh(v) => {
                viewport_height * v / 100.0
            }
            LengthValue::Fr => 0.0,
            LengthValue::Lh(v) => font_size * v,
            LengthValue::Rlh(v) => root_font_size * v,
        }
    }
}

pub fn parse_length_value(value: &str) -> Option<LengthValue> {
    let value = value.trim();
    if value == "0" {
        return Some(LengthValue::Px(0.0));
    }
    if let Some(v) = value.strip_suffix("px") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Px);
    }
    if let Some(v) = value.strip_suffix("pt") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 96.0 / 72.0));
    }
    if let Some(v) = value.strip_suffix("pc") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 16.0));
    }
    if let Some(v) = value.strip_suffix("cm") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 96.0 / 2.54));
    }
    if let Some(v) = value.strip_suffix("mm") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 96.0 / 25.4));
    }
    if let Some(v) = value.strip_suffix("Q") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 96.0 / 101.6));
    }
    if let Some(v) = value.strip_suffix('%') {
        return v.trim().parse::<f32>().ok().map(LengthValue::Percent);
    }
    if let Some(v) = value.strip_suffix("rem") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Rem);
    }
    if let Some(v) = value.strip_suffix("em") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Em);
    }
    if let Some(v) = value.strip_suffix("ch") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Ch);
    }
    if let Some(v) = value.strip_suffix("ex") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Ex);
    }
    if let Some(v) = value.strip_suffix("vw") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vw);
    }
    if let Some(v) = value.strip_suffix("vmin") {
        return v.trim().parse::<f32>().ok().map(LengthValue::VMin);
    }
    if let Some(v) = value.strip_suffix("vmax") {
        return v.trim().parse::<f32>().ok().map(LengthValue::VMax);
    }
    if let Some(v) = value.strip_suffix("svh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Svh);
    }
    if let Some(v) = value.strip_suffix("lvh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Lvh);
    }
    if let Some(v) = value.strip_suffix("dvh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Dvh);
    }
    if let Some(v) = value.strip_suffix("vh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vh);
    }
    if let Some(v) = value.strip_suffix("in") {
        return v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Px(v * 96.0));
    }
    if let Some(v) = value.strip_suffix("rlh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Rlh);
    }
    if let Some(v) = value.strip_suffix("lh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Lh);
    }
    if let Some(v) = value.strip_suffix("fr") {
        return v.trim().parse::<f32>().ok().map(|_| LengthValue::Fr);
    }
    None
}

pub(super) fn parse_length_px(value: &str) -> Option<f32> {
    let value = value.trim();
    if value == "0" {
        return Some(0.0);
    }
    match parse_length_value(value)? {
        LengthValue::Px(px) => Some(px),
        _ => None,
    }
}
