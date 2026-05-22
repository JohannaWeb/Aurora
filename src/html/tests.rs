use super::Parser;
use crate::dom::{DocumentMode, Node, NodePtr};
use std::collections::BTreeMap;

fn element(tag: &str, children: Vec<NodePtr>) -> NodePtr {
    Node::element_with_attributes(tag, BTreeMap::new(), children)
}

fn document_with_body(body_children: Vec<NodePtr>) -> NodePtr {
    document_with_head_and_body(Vec::new(), body_children, DocumentMode::Quirks)
}

fn standards_document_with_body(body_children: Vec<NodePtr>) -> NodePtr {
    document_with_head_and_body(Vec::new(), body_children, DocumentMode::NoQuirks)
}

fn document_with_head_and_body(
    head_children: Vec<NodePtr>,
    body_children: Vec<NodePtr>,
    mode: DocumentMode,
) -> NodePtr {
    Node::document_with_mode(
        vec![element(
            "html",
            vec![
                element("head", head_children),
                element("body", body_children),
            ],
        )],
        mode,
    )
}

#[test]
fn parses_nested_html_into_dom_tree() {
    let mut parser =
        Parser::new("<!doctype html><html><body><p>Hello</p><p>World</p></body></html>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        standards_document_with_body(vec![
            element("p", vec![Node::text("Hello")]),
            element("p", vec![Node::text("World")]),
        ])
    );
}

#[test]
fn ignores_whitespace_only_text_nodes() {
    let mut parser = Parser::new("<div>\n  <p>Text</p>\n</div>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        document_with_body(vec![Node::element(
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
        document_with_head_and_body(
            vec![Node::element(
                "script",
                vec![Node::text("if (a < b) { run(); }")],
            )],
            Vec::new(),
            DocumentMode::Quirks,
        )
    );
}

#[test]
fn keeps_mismatched_closing_tag_for_parent_recovery() {
    let mut parser = Parser::new("<div><span>t</div><p>x</p>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        document_with_body(vec![
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
        document_with_body(vec![Node::element(
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
        document_with_body(vec![Node::element_with_attributes(
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
        document_with_body(vec![Node::element_with_attributes(
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
        document_with_body(vec![Node::element_with_attributes(
            "div",
            div_attributes,
            vec![],
        )])
    );
}

#[test]
fn foster_parents_text_around_table_content() {
    let mut parser = Parser::new("<div><table>before<tr><td>cell</td></tr>after</table></div>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        document_with_body(vec![Node::element(
            "div",
            vec![
                Node::text("before"),
                Node::text("after"),
                Node::element(
                    "table",
                    vec![Node::element(
                        "tbody",
                        vec![Node::element(
                            "tr",
                            vec![Node::element("td", vec![Node::text("cell")])],
                        )],
                    )],
                ),
            ],
        )])
    );
}

#[test]
fn reconstructs_formatting_elements_with_adoption_agency_algorithm() {
    let mut parser = Parser::new("<p><b>one<i>two</b>three</i></p>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        document_with_body(vec![Node::element(
            "p",
            vec![
                Node::element(
                    "b",
                    vec![
                        Node::text("one"),
                        Node::element("i", vec![Node::text("two")]),
                    ],
                ),
                Node::element("i", vec![Node::text("three")]),
            ],
        )])
    );
}

#[test]
fn parses_textarea_as_rcdata_and_decodes_entities() {
    let mut parser = Parser::new("<textarea>Tom &amp; Jerry &lt;b&gt;</textarea>");
    let document = parser.parse_document();

    assert_eq!(
        document,
        document_with_body(vec![Node::element(
            "textarea",
            vec![Node::text("Tom & Jerry <b>")],
        )])
    );
}

#[test]
fn style_text_stays_inside_style_element() {
    use crate::dom::Node;
    let html = r#"<!DOCTYPE html>
<html><head>
<style>body{background:#eee;width:60vw}a:link,a:visited{color:#348}</style>
</head><body><p>Hello</p></body></html>"#;

    let dom = super::Parser::new(html).parse_document();

    // Walk the DOM and find any text node containing CSS
    fn find_css_text(node: &crate::dom::NodePtr, path: &str) -> Vec<String> {
        let mut found = Vec::new();
        let b = node.borrow();
        match &*b {
            Node::Text(t) if t.contains('{') => {
                found.push(format!("{path}: {:?}", &t[..t.len().min(60)]));
            }
            Node::Element(el) => {
                let new_path = format!("{path}/<{}>", el.tag_name);
                let children = el.children.clone();
                drop(b);
                for child in children {
                    found.extend(find_css_text(&child, &new_path));
                }
                return found;
            }
            Node::Document { children, .. } => {
                let children = children.clone();
                drop(b);
                for child in children {
                    found.extend(find_css_text(&child, path));
                }
                return found;
            }
            _ => {}
        }
        found
    }

    let occurrences = find_css_text(&dom, "doc");
    println!("CSS text locations: {occurrences:#?}");
    
    for loc in &occurrences {
        assert!(
            loc.contains("<style>"),
            "CSS text found outside <style> element: {loc}"
        );
    }
}
