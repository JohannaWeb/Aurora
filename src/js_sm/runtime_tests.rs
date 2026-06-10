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

#[test]
fn custom_elements_upgrade_existing_dom_nodes_on_define() {
    let dom = Parser::new("<html><body><test-card></test-card></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class TestCard extends HTMLElement {
                connectedCallback() {
                    this.textContent = "hydrated";
                }
            }
            customElements.define("test-card", TestCard);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "hydrated");
}

#[test]
fn dom_module_templates_resolve_for_custom_elements() {
    let dom = Parser::new(
        "<html><body>\
         <dom-module id=\"test-card\"><template><span>hydrated</span></template></dom-module>\
         </body></html>",
    )
    .parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class DomModule extends HTMLElement {}
            customElements.define("dom-module", DomModule);

            class TestCard extends HTMLElement {
            }

            customElements.define("test-card", TestCard);
            document.body.textContent = String(!!customElements.get("test-card").template);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "true");

    runtime
        .execute(
            r#"
            (() => {
                document.body.textContent = "";
                const template = customElements.get("test-card").template;
                document.body.textContent = String(template.content.cloneNode(true).nodeType);
            })();
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "11");

    runtime
        .execute(
            r#"
            (() => {
                document.body.textContent = "";
                const template = customElements.get("test-card").template;
                document.body.appendChild(template.content.cloneNode(true));
            })();
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "hydrated");
}

#[test]
fn document_current_script_tracks_running_script_node() {
    let dom =
        Parser::new("<html><body><script src=\"/app.js\"></script></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());
    let script = find_first_tag(&dom, "script").expect("script element should exist");

    runtime.set_current_script(Some(&script));
    runtime
        .execute(
            r#"
            document.body.textContent =
                document.currentScript.tagName + ":" + document.currentScript.getAttribute("src");
            "#,
        )
        .unwrap();
    runtime.set_current_script(None);

    assert_eq!(text_content(&dom), "SCRIPT:/app.js");
}

#[test]
fn request_idle_callback_receives_deadline_object() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            requestIdleCallback((deadline) => {
                document.body.textContent = String(deadline.didTimeout) + ":" + String(typeof deadline.timeRemaining);
            });
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "false:function");
}

#[test]
fn message_channel_delivers_messages() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const channel = new MessageChannel();
            channel.port2.onmessage = (event) => {
                document.body.textContent = event.data;
            };
            channel.port1.postMessage("ping");
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "ping");
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}

fn find_first_tag(node: &NodePtr, tag: &str) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Document { children, .. } => {
            children.iter().find_map(|child| find_first_tag(child, tag))
        }
        Node::Element(element) => {
            if element.tag_name == tag {
                return Some(node.clone());
            }
            element
                .children
                .iter()
                .find_map(|child| find_first_tag(child, tag))
        }
        Node::Text(_) => None,
    }
}
