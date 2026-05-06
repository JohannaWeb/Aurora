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
