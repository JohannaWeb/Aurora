use super::Parser;
use crate::dom::{Node, NodePtr};
use std::collections::BTreeMap;

fn element(tag: &str, children: Vec<NodePtr>) -> NodePtr {
    Node::element_with_attributes(tag, BTreeMap::new(), children)
}

#[test]
fn parses_nested_html_into_dom_tree() {
    let mut parser = Parser::new("<html><body><p>Hello</p><p>World</p></body></html>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        Node::document(vec![element(
            "html",
            vec![element(
                "body",
                vec![
                    element("p", vec![Node::text("Hello")]),
                    element("p", vec![Node::text("World")]),
                ],
            )],
        )])
    );
}

#[test]
fn ignores_whitespace_only_text_nodes() {
    let mut parser = Parser::new("<div>\n  <p>Text</p>\n</div>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        Node::document(vec![Node::element(
            "div",
            vec![Node::element("p", vec![Node::text("Text")])],
        )])
    );
}

#[test]
fn keeps_script_contents_as_raw_text() {
    let mut parser = Parser::new("<script>if (a < b) { run(); }</script>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        Node::document(vec![Node::element(
            "script",
            vec![Node::text("if (a < b) { run(); }")],
        )])
    );
}

#[test]
fn keeps_mismatched_closing_tag_for_parent_recovery() {
    let mut parser = Parser::new("<div><span>t</div><p>x</p>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        Node::document(vec![
            Node::element("div", vec![Node::element("span", vec![Node::text("t")])],),
            Node::element("p", vec![Node::text("x")]),
        ])
    );
}

#[test]
fn treats_img_as_void_element() {
    let mut parser =
        Parser::new("<div><img src=\"cat.txt\" alt=\"sleepy cat\"><p>caption</p></div>");
    let document = parser.parse_document();

    let mut img_attributes = BTreeMap::new();
    img_attributes.insert("alt".to_string(), "sleepy cat".to_string());
    img_attributes.insert("src".to_string(), "cat.txt".to_string());

    assert_eq!(
        document,
        Node::document(vec![Node::element(
            "div",
            vec![
                Node::element_with_attributes("img", img_attributes, Vec::new()),
                Node::element("p", vec![Node::text("caption")]),
            ],
        )])
    );
}

#[test]
fn preserves_tag_attributes() {
    let mut parser =
        Parser::new(r#"<div id="app" class="shell main"><p data-role=hero hidden>Hello</p></div>"#);
    let document = parser.parse_document();

    let mut div_attributes = BTreeMap::new();
    div_attributes.insert("class".to_string(), "shell main".to_string());
    div_attributes.insert("id".to_string(), "app".to_string());

    let mut p_attributes = BTreeMap::new();
    p_attributes.insert("data-role".to_string(), "hero".to_string());
    p_attributes.insert("hidden".to_string(), String::new());

    assert_eq!(
        document,
        Node::document(vec![Node::element_with_attributes(
            "div",
            div_attributes,
            vec![Node::element_with_attributes(
                "p",
                p_attributes,
                vec![Node::text("Hello")],
            )],
        )])
    );
}

#[test]
fn handles_quoted_attributes_with_special_characters() {
    let mut parser = Parser::new(
        r#"<a href="http://example.com?foo=bar>baz" title="Text with 'quotes' > inside">Link</a>"#,
    );
    let document = parser.parse_document();

    let mut a_attributes = BTreeMap::new();
    a_attributes.insert(
        "href".to_string(),
        "http://example.com?foo=bar>baz".to_string(),
    );
    a_attributes.insert(
        "title".to_string(),
        "Text with 'quotes' > inside".to_string(),
    );

    assert_eq!(
        document,
        Node::document(vec![Node::element_with_attributes(
            "a",
            a_attributes,
            vec![Node::text("Link")],
        )])
    );
}

#[test]
fn handles_json_in_data_attributes() {
    let mut parser = Parser::new(r#"<div data-config='{"key":"value","num":>0}'></div>"#);
    let document = parser.parse_document();

    let mut div_attributes = BTreeMap::new();
    div_attributes.insert(
        "data-config".to_string(),
        "{\"key\":\"value\",\"num\":>0}".to_string(),
    );

    assert_eq!(
        document,
        Node::document(vec![Node::element_with_attributes(
            "div",
            div_attributes,
            vec![],
        )])
    );
}
