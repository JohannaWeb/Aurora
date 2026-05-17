use super::*;
use std::collections::BTreeMap;

fn el(tag_name: &str) -> ElementData {
    element(tag_name, &[])
}

fn el_with(tag_name: &str, attrs: &[(&str, &str)]) -> ElementData {
    element(tag_name, attrs)
}

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

#[test]
fn child_combinator_requires_direct_parent() {
    let stylesheet = Stylesheet::parse("div > p { color: red; }");
    let div = el("div");
    let p = el("p");
    // Direct child: div → p — should match.
    assert_eq!(
        stylesheet.styles_for(&p, &[div.clone()]).get("color"),
        Some("red")
    );
    // Grandchild: div → span → p — should NOT match.
    let span = el("span");
    assert_eq!(
        stylesheet.styles_for(&p, &[div.clone(), span]).get("color"),
        None
    );
    // Ancestor without child combinator — just div alone as parent still matches.
    assert_eq!(
        stylesheet.styles_for(&p, &[div]).get("color"),
        Some("red")
    );
}

#[test]
fn attribute_selector_exact_match() {
    let stylesheet = Stylesheet::parse(r#"input[type=checkbox] { display: none; }"#);
    let checkbox = el_with("input", &[("type", "checkbox")]);
    let text_input = el_with("input", &[("type", "text")]);
    let no_type = el("input");

    assert_eq!(stylesheet.styles_for(&checkbox, &[]).get("display"), Some("none"));
    assert_eq!(stylesheet.styles_for(&text_input, &[]).get("display"), None);
    assert_eq!(stylesheet.styles_for(&no_type, &[]).get("display"), None);
}

#[test]
fn attribute_selector_substring_match() {
    let stylesheet = Stylesheet::parse(r#"a[href*="example"] { color: green; }"#);
    let link = el_with("a", &[("href", "https://example.com/foo")]);
    let other = el_with("a", &[("href", "https://other.com")]);

    assert_eq!(stylesheet.styles_for(&link, &[]).get("color"), Some("green"));
    assert_eq!(stylesheet.styles_for(&other, &[]).get("color"), None);
}

#[test]
fn not_pseudo_class_negates_match() {
    let stylesheet = Stylesheet::parse("p:not(.lead) { color: gray; }");
    let plain = el_with("p", &[]);
    let lead = el_with("p", &[("class", "lead")]);

    assert_eq!(stylesheet.styles_for(&plain, &[]).get("color"), Some("gray"));
    assert_eq!(stylesheet.styles_for(&lead, &[]).get("color"), None);
}

#[test]
fn specificity_counts_attribute_selectors() {
    // [attr] selector specificity (0,1,0) should beat plain tag (0,0,1).
    let stylesheet = Stylesheet::parse("p { color: blue; } p[lang] { color: red; }");
    let p_lang = el_with("p", &[("lang", "en")]);
    assert_eq!(stylesheet.styles_for(&p_lang, &[]).get("color"), Some("red"));
}
