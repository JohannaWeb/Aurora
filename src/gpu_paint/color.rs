use peniko::Color;

pub(super) fn parse_color(name: &str) -> Color {
    let name = name.trim().to_lowercase();
    if name.starts_with('#') {
        let hex = &name[1..];
        if hex.len() == 6 {
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                return Color::from_rgb8(
                    ((c >> 16) & 0xFF) as u8,
                    ((c >> 8) & 0xFF) as u8,
                    (c & 0xFF) as u8,
                );
            }
        }
    }

    match name.as_str() {
        "white" | "#fff" => Color::WHITE,
        "black" | "#000" => Color::BLACK,
        "red" => Color::from_rgb8(255, 0, 0),
        "blue" => Color::from_rgb8(0, 0, 255),
        "green" => Color::from_rgb8(0, 128, 0),
        "transparent" => Color::TRANSPARENT,
        "aurora-cyan" => Color::from_rgb8(0x88, 0xC0, 0xD0),
        "coal" => Color::from_rgb8(0x2E, 0x34, 0x40),
        "rust" => Color::from_rgb8(0xBF, 0x61, 0x6A),
        _ => Color::from_rgb8(0x4C, 0x56, 0x6A),
    }
}
