use super::*;
use crate::dom::{Node, NodePtr};
use crate::html::Parser;
use std::time::Instant;

#[test]
fn promise_callbacks_run_on_tick() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            Promise.resolve("ready").then((value) => {
                document.body.textContent = value;
            });
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "");
    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "ready");

    runtime
        .execute(
            r#"
            new Promise((resolve) => resolve(20))
                .then((value) => value + 22)
                .then((value) => {
                    document.body.textContent = String(value);
                });
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "42");
    runtime
        .execute(
            r#"
            document.body.textContent = "";
            class TestCard extends HTMLElement {
                connectedCallback() {
                    this.textContent = "connected";
                }
            }
            customElements.define("test-card", TestCard);

            const el = document.createElement("test-card");
            document.body.appendChild(el);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "connected");

    runtime
        .execute(
            r#"
            document.body.textContent = "";
            window.addEventListener("DOMContentLoaded", () => {
                document.body.textContent += "dom";
            });
            document.addEventListener("load", () => {
                document.body.textContent += ":doc-load";
            });
            window.addEventListener("load", () => {
                document.body.textContent += ":win-load";
            });
            "#,
        )
        .unwrap();

    runtime.fire_dom_content_loaded();
    runtime.fire_load();
    assert_eq!(text_content(&dom), "dom:doc-load:win-load");
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}
