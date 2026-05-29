use super::StyleTree;
use crate::css::Stylesheet;
use crate::dom::{Node, NodePtr};
use std::collections::BTreeMap;

fn element(tag: &str, children: Vec<NodePtr>) -> NodePtr {
    Node::element(tag, children)
}

#[test]
fn computes_descendant_matched_styles() {
    let mut section_attributes = BTreeMap::new();
    section_attributes.insert("class".to_string(), "hero".to_string());
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element_with_attributes(
            "section",
            section_attributes,
            vec![Node::element("p", vec![Node::text("Hello")])],
        )],
    )]);

    let stylesheet = Stylesheet::parse("section.hero p { color: gold; display: inline; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let rendered = style_tree.to_string();

    assert!(rendered.contains("<p> {color: gold, display: inline}"));
    assert!(rendered.contains("\"Hello\" {color: gold, display: inline}"));
}

#[test]
fn inherits_color_to_descendants() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Inherited")])],
    )]);

    let stylesheet = Stylesheet::parse("body { color: slate; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let rendered = style_tree.to_string();

    assert!(rendered.contains("<p> {color: slate}"));
    assert!(rendered.contains("\"Inherited\" {color: slate, display: inline}"));
}

#[test]
fn inherits_typography_properties() {
    let dom = Node::document(vec![element(
        "body",
        vec![element("p", vec![Node::text("Text")])],
    )]);

    let stylesheet =
        Stylesheet::parse("body { font-size: 16px; font-weight: bold; line-height: 20px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let rendered = style_tree.to_string();

    assert!(rendered.contains("font-size: 16px"));
    assert!(rendered.contains("font-weight: bold"));
    assert!(rendered.contains("line-height: 20px"));
}

#[test]
fn inherits_visibility() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Text")])],
    )]);

    let stylesheet = Stylesheet::parse("body { visibility: hidden; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let rendered = style_tree.to_string();

    assert!(rendered.contains("visibility: hidden"));
}

#[test]
fn custom_properties_do_not_leak_between_unrelated_branches() {
    let mut first_attrs = BTreeMap::new();
    first_attrs.insert("class".to_string(), "first".to_string());
    let mut second_attrs = BTreeMap::new();
    second_attrs.insert("class".to_string(), "second".to_string());

    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element_with_attributes(
                "section",
                first_attrs,
                vec![Node::element("p", vec![Node::text("First")])],
            ),
            Node::element_with_attributes(
                "section",
                second_attrs,
                vec![Node::element("p", vec![Node::text("Second")])],
            ),
        ],
    )]);

    let stylesheet = Stylesheet::parse(
        ".first { --accent: red; } .first p, .second p { color: var(--accent, blue); }",
    );
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let rendered = style_tree.to_string();

    assert!(rendered.contains("\"First\" {color: red, display: inline}"));
    assert!(rendered.contains("\"Second\" {color: blue, display: inline}"));
}
