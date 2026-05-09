use super::*;
use crate::html::Parser;
use std::time::Instant;

#[test]
fn request_animation_frame_runs_before_paint_tick() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            requestAnimationFrame(() => {
                document.body.textContent = "paint";
            });
            "#,
        )
        .unwrap();

    assert!(runtime.has_animation_frame_callbacks());
    assert!(runtime.drain_animation_frame_callbacks(Instant::now()));
    assert_eq!(text_content(&dom), "paint");
}

#[test]
fn animation_frame_scheduled_from_animation_frame_waits_for_next_frame() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            requestAnimationFrame(() => {
                document.body.textContent = "first";
                requestAnimationFrame(() => {
                    document.body.textContent = "second";
                });
            });
            "#,
        )
        .unwrap();

    assert!(runtime.drain_animation_frame_callbacks(Instant::now()));
    assert_eq!(text_content(&dom), "first");
    assert!(runtime.has_animation_frame_callbacks());
    assert!(runtime.drain_animation_frame_callbacks(Instant::now()));
    assert_eq!(text_content(&dom), "second");
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}
