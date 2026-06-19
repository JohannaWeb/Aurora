use image::Rgba;

pub(super) fn parse_screenshot_color(color_str: &str) -> Rgba<u8> {
    let color_str = color_str.trim().to_lowercase();

    if let Some(hex) = color_str.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                return Rgba([
                    ((c >> 16) & 0xFF) as u8,
                    ((c >> 8) & 0xFF) as u8,
                    (c & 0xFF) as u8,
                    255,
                ]);
            }
        }
    }

    // rgb(r, g, b) / rgba(r, g, b, a) — channels 0-255, alpha 0.0-1.0.
    if let Some(rest) = color_str
        .strip_prefix("rgba(")
        .or_else(|| color_str.strip_prefix("rgb("))
    {
        if let Some(inner) = rest.strip_suffix(')') {
            let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
            if parts.len() >= 3 {
                let chan = |s: &str| s.parse::<f32>().ok().map(|v| v.clamp(0.0, 255.0) as u8);
                if let (Some(r), Some(g), Some(b)) =
                    (chan(parts[0]), chan(parts[1]), chan(parts[2]))
                {
                    let a = parts
                        .get(3)
                        .and_then(|s| s.parse::<f32>().ok())
                        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                        .unwrap_or(255);
                    return Rgba([r, g, b, a]);
                }
            }
        }
    }

    match color_str.as_str() {
        "black" => Rgba([0, 0, 0, 255]),
        "white" => Rgba([255, 255, 255, 255]),
        "red" => Rgba([255, 0, 0, 255]),
        "blue" => Rgba([0, 0, 255, 255]),
        "green" => Rgba([0, 128, 0, 255]),
        "gray" | "grey" => Rgba([128, 128, 128, 255]),
        "coal" => Rgba([0x2E, 0x34, 0x40, 255]),
        _ => Rgba([64, 64, 64, 255]),
    }
}
