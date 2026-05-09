use super::*;
use crate::html::Parser;
use std::time::Instant;

#[test]
fn inline_style_mutation_updates_attribute_and_requests_reflow() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                document.body.style.setProperty("width", "10px");
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(body_attr(&dom, "style").as_deref(), Some("width: 10px"));
}

#[test]
fn inline_style_property_assignment_updates_attribute_and_requests_reflow() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                document.body.style.width = "12px";
                document.body.style.backgroundColor = "red";
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(
        body_attr(&dom, "style").as_deref(),
        Some("background-color: red; width: 12px")
    );
}

#[test]
fn inline_style_property_assignment_empty_value_removes_property() {
    let dom = Parser::new(r#"<html><body style="width: 12px; color: blue"></body></html>"#)
        .parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                document.body.style.width = "";
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(body_attr(&dom, "style").as_deref(), Some("color: blue"));
}

#[test]
fn inline_style_set_property_empty_value_removes_property() {
    let dom = Parser::new(r#"<html><body style="width: 12px; color: blue"></body></html>"#)
        .parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                document.body.style.setProperty("color", "");
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(body_attr(&dom, "style").as_deref(), Some("width: 12px"));
}

#[test]
fn inline_style_css_text_assignment_replaces_attribute_and_requests_reflow() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                document.body.style.cssText = "height: 44px; color: green";
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(
        body_attr(&dom, "style").as_deref(),
        Some("color: green; height: 44px")
    );
}

fn body_attr(node: &NodePtr, name: &str) -> Option<String> {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().find_map(|child| body_attr(child, name)),
        Node::Element(element) if element.tag_name == "body" => {
            element.attributes.get(name).cloned()
        }
        Node::Element(element) => element
            .children
            .iter()
            .find_map(|child| body_attr(child, name)),
        Node::Text(_) => None,
    }
}
