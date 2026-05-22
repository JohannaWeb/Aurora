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

/// Call styles_for with no ancestors or siblings (the common test case).
fn styles(stylesheet: &Stylesheet, el: &ElementData) -> StyleMap {
    stylesheet.styles_for(el, &[], &[], 0)
}

/// Call styles_for with ancestors but no siblings.
fn styles_with_ancestors(stylesheet: &Stylesheet, el: &ElementData, ancestors: &[ElementData]) -> StyleMap {
    stylesheet.styles_for(el, ancestors, &[], 0)
}

#[test]
fn parser_keeps_braces_inside_declaration_values() {
    let stylesheet =
        Stylesheet::parse(r#"p::before { content: "}"; color: red; } p { color: blue; }"#);

    assert_eq!(stylesheet.rules.len(), 2);
    assert_eq!(stylesheet.rules[0].declarations[0].name, "content");
    // cssparser serialises the quoted string back with quotes
    assert!(stylesheet.rules[0].declarations[0].value.contains('}'));
    assert_eq!(stylesheet.rules[1].declarations[0].value, "blue");
}

#[test]
fn important_declarations_override_later_normal_declarations() {
    let stylesheet = Stylesheet::parse("p { color: red !important; } p { color: blue; }");
    assert_eq!(styles(&stylesheet, &element("p", &[])).get("color"), Some("red"));
}

#[test]
fn display_inline_block_maps_to_distinct_display_mode() {
    let stylesheet = Stylesheet::parse("span { display: inline-block; }");
    assert_eq!(styles(&stylesheet, &element("span", &[])).display_mode(), DisplayMode::InlineBlock);
}

#[test]
fn vertical_margins_can_parse_auto() {
    let stylesheet = Stylesheet::parse("main { margin-top: auto; margin-bottom: auto; }");
    let margin = styles(&stylesheet, &element("main", &[])).margin();
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
        styles_with_ancestors(&stylesheet, &p, &[div.clone()]).get("color"),
        Some("red")
    );
    // Grandchild: div → span → p — should NOT match.
    let span = el("span");
    assert_eq!(
        styles_with_ancestors(&stylesheet, &p, &[div.clone(), span]).get("color"),
        None
    );
}

#[test]
fn attribute_selector_exact_match() {
    let stylesheet = Stylesheet::parse(r#"input[type=checkbox] { display: none; }"#);
    let checkbox = el_with("input", &[("type", "checkbox")]);
    let text_input = el_with("input", &[("type", "text")]);
    let no_type = el("input");

    assert_eq!(styles(&stylesheet, &checkbox).get("display"), Some("none"));
    assert_eq!(styles(&stylesheet, &text_input).get("display"), None);
    assert_eq!(styles(&stylesheet, &no_type).get("display"), None);
}

#[test]
fn attribute_selector_substring_match() {
    let stylesheet = Stylesheet::parse(r#"a[href*="example"] { color: green; }"#);
    let link = el_with("a", &[("href", "https://example.com/foo")]);
    let other = el_with("a", &[("href", "https://other.com")]);

    assert_eq!(styles(&stylesheet, &link).get("color"), Some("green"));
    assert_eq!(styles(&stylesheet, &other).get("color"), None);
}

#[test]
fn not_pseudo_class_negates_match() {
    let stylesheet = Stylesheet::parse("p:not(.lead) { color: gray; }");
    let plain = el_with("p", &[]);
    let lead = el_with("p", &[("class", "lead")]);

    assert_eq!(styles(&stylesheet, &plain).get("color"), Some("gray"));
    assert_eq!(styles(&stylesheet, &lead).get("color"), None);
}

#[test]
fn specificity_counts_attribute_selectors() {
    // [attr] selector specificity (0,1,0) should beat plain tag (0,0,1).
    let stylesheet = Stylesheet::parse("p { color: blue; } p[lang] { color: red; }");
    let p_lang = el_with("p", &[("lang", "en")]);
    assert_eq!(styles(&stylesheet, &p_lang).get("color"), Some("red"));
}

#[test]
fn adjacent_sibling_combinator_matches_immediately_preceding() {
    let stylesheet = Stylesheet::parse("h2 + p { margin-top: 0; }");
    let h2 = el("h2");
    let p = el("p");
    let siblings = vec![h2, p.clone()];
    // p at index 1, h2 at index 0 — adjacent match
    assert_eq!(
        stylesheet.styles_for(&p, &[], &siblings, 1).get("margin-top"),
        Some("0")
    );
    // p at index 0 — no preceding sibling
    assert_eq!(
        stylesheet.styles_for(&p, &[], &siblings, 0).get("margin-top"),
        None
    );
}

#[test]
fn is_pseudo_class_matches_any_in_list() {
    let stylesheet = Stylesheet::parse(":is(h1, h2, h3) { font-weight: bold; }");
    assert_eq!(styles(&stylesheet, &el("h1")).get("font-weight"), Some("bold"));
    assert_eq!(styles(&stylesheet, &el("h2")).get("font-weight"), Some("bold"));
    assert_eq!(styles(&stylesheet, &el("p")).get("font-weight"), None);
}

#[test]
fn media_query_rules_are_included() {
    let stylesheet = Stylesheet::parse("@media screen { p { color: red; } }");
    assert_eq!(styles(&stylesheet, &el("p")).get("color"), Some("red"));
}

#[test]
fn print_media_query_rules_are_excluded() {
    let stylesheet = Stylesheet::parse("@media print { p { color: invisible; } } p { color: visible; }");
    assert_eq!(styles(&stylesheet, &el("p")).get("color"), Some("visible"));
}

#[test]
fn ua_sets_display_none_on_head() {
    let ua = Stylesheet::user_agent_stylesheet();
    let head = el("head");
    assert_eq!(styles(&ua, &head).get("display"), Some("none"),
        "UA stylesheet must set display:none on <head>");
    let style_el = el("style");
    assert_eq!(styles(&ua, &style_el).get("display"), Some("none"),
        "UA stylesheet must set display:none on <style>");
}
