use crate::css::StyleMap;
use crate::style::StyledNode;

pub(in crate::layout) fn font_size_from_styles(styles: &StyleMap) -> f32 {
    styles
        .font_size_resolved(16.0, 16.0)
        .or_else(|| styles.font_size_px())
        .filter(|&s| s > 0.0)
        .unwrap_or(16.0)
}

pub(in crate::layout) fn measure_text_width(text: &str, styles: &StyleMap) -> f32 {
    crate::font::measure_text(text, font_size_from_styles(styles))
}

pub(in crate::layout) fn line_height_from_styles(styles: &StyleMap) -> f32 {
    let fs = font_size_from_styles(styles);
    styles.line_height_px().unwrap_or(fs * 1.2)
}

pub(in crate::layout) fn control_label(tag_name: &str, node: &StyledNode) -> String {
    match tag_name {
        "input" => node
            .attribute("value")
            .or_else(|| node.attribute("placeholder"))
            .unwrap_or_default(),
        "textarea" => {
            let from_children = collect_text_content(node).trim().to_string();
            if from_children.is_empty() {
                node.attribute("placeholder").unwrap_or_default()
            } else {
                from_children
            }
        }
        "button" => collect_text_content(node).trim().to_string(),
        _ => String::new(),
    }
}

pub(in crate::layout) fn collect_text_content(node: &StyledNode) -> String {
    if let Some(text) = node.text() {
        return text;
    }

    let mut combined = String::new();
    for child in node.children() {
        let part = collect_text_content(child);
        if part.is_empty() {
            continue;
        }
        if !combined.is_empty() {
            combined.push(' ');
        }
        combined.push_str(part.trim());
    }
    combined
}
