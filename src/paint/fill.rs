pub(super) fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for ch in value.chars().take(max_chars) {
        result.push(ch);
    }
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

fn box_fill_char(tag_name: &str, color: Option<&str>) -> char {
    if let Some(color) = color {
        return color
            .chars()
            .next()
            .unwrap_or(tag_name.chars().next().unwrap_or('#'));
    }

    match tag_name {
        "html" => '=',
        "body" => ':',
        "section" => '+',
        "h1" => '#',
        "p" => '-',
        _ => tag_name.chars().next().unwrap_or('?'),
    }
}

pub(super) fn background_fill_char(
    tag_name: &str,
    background_color: Option<&str>,
    color: Option<&str>,
) -> char {
    if let Some(bg) = background_color {
        let bg_lower = bg.to_lowercase();
        if bg_lower == "white"
            || bg_lower == "#fff"
            || bg_lower == "#ffffff"
            || bg_lower == "transparent"
        {
            return ' ';
        }
        return bg.chars().next().unwrap_or(' ');
    }

    box_fill_char(tag_name, color)
}

pub(super) fn border_fill_char(
    tag_name: &str,
    border_color: Option<&str>,
    color: Option<&str>,
) -> char {
    if let Some(border_color) = border_color {
        return border_color
            .chars()
            .next()
            .map(|ch| ch.to_ascii_uppercase())
            .unwrap_or('*');
    }

    box_fill_char(tag_name, color).to_ascii_uppercase()
}
