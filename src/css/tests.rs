use super::*;
use std::collections::BTreeMap;

fn element(tag_name: &str, attrs: &[(&str, &str)]) -> ElementData {
    ElementData {
        tag_name: tag_name.to_string(),
        attributes: attrs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn parser_keeps_braces_inside_declaration_values() {
    let stylesheet =
        Stylesheet::parse(r#"p::before { content: "}"; color: red; } p { color: blue; }"#);

    assert_eq!(stylesheet.rules.len(), 2);
    assert_eq!(stylesheet.rules[0].declarations[0].name, "content");
    assert_eq!(stylesheet.rules[0].declarations[0].value, "\"}\"");
    assert_eq!(stylesheet.rules[1].declarations[0].value, "blue");
}

#[test]
fn important_declarations_override_later_normal_declarations() {
    let stylesheet = Stylesheet::parse("p { color: red !important; } p { color: blue; }");
    let styles = stylesheet.styles_for(&element("p", &[]), &[]);

    assert_eq!(styles.get("color"), Some("red"));
}

#[test]
fn display_inline_block_maps_to_distinct_display_mode() {
    let stylesheet = Stylesheet::parse("span { display: inline-block; }");
    let styles = stylesheet.styles_for(&element("span", &[]), &[]);

    assert_eq!(styles.display_mode(), DisplayMode::InlineBlock);
}

#[test]
fn vertical_margins_can_parse_auto() {
    let stylesheet = Stylesheet::parse("main { margin-top: auto; margin-bottom: auto; }");
    let styles = stylesheet.styles_for(&element("main", &[]), &[]);
    let margin = styles.margin();

    assert_eq!(margin.top, MarginValue::Auto);
    assert_eq!(margin.bottom, MarginValue::Auto);
}

#[test]
fn parses_additional_length_units() {
    assert_eq!(
        parse_length_value("1in").map(|v| v.to_px(0.0, 16.0, 16.0, 800.0, 600.0)),
        Some(96.0)
    );
    assert_eq!(
        parse_length_value("50vmin").map(|v| v.to_px(0.0, 16.0, 16.0, 800.0, 600.0)),
        Some(300.0)
    );
    assert_eq!(
        parse_length_value("10svh").map(|v| v.to_px(0.0, 16.0, 16.0, 800.0, 600.0)),
        Some(60.0)
    );
}
