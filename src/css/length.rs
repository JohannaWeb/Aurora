#[derive(Debug, Clone, Copy)]
pub enum LengthValue {
    Px(f32),
    Percent(f32),
    Rem(f32),
    Em(f32),
    Vh(f32),
    Vw(f32),
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
            LengthValue::Vw(v) => viewport_width * v / 100.0,
            LengthValue::Vh(v) => viewport_height * v / 100.0,
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
    if let Some(v) = value.strip_suffix('%') {
        return v.trim().parse::<f32>().ok().map(LengthValue::Percent);
    }
    if let Some(v) = value.strip_suffix("rem") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Rem);
    }
    if let Some(v) = value.strip_suffix("em") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Em);
    }
    if let Some(v) = value.strip_suffix("vw") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vw);
    }
    if let Some(v) = value.strip_suffix("vh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vh);
    }
    None
}

pub(super) fn parse_length_px(value: &str) -> Option<f32> {
    let value = value.trim();
    if value == "0" {
        return Some(0.0);
    }
    value.strip_suffix("px")?.trim().parse::<f32>().ok()
}
