use super::*;
use crate::html::Parser;
use std::time::{Duration, Instant};

#[test]
fn set_timeout_runs_on_tick_and_updates_dom() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                const p = document.createElement("p");
                p.textContent = "ready";
                document.body.appendChild(p);
            }, 0);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "");
    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "ready");
}

#[test]
fn timer_without_dom_mutation_does_not_request_reflow() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom);

    runtime
        .execute("setTimeout(() => { const value = 42; }, 0);")
        .unwrap();

    assert!(!runtime.tick(Instant::now()));
}

#[test]
fn queued_microtask_runs_after_script_entry() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            queueMicrotask(() => { document.body.textContent = "micro"; });
            document.body.textContent = "sync";
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "micro");
}

#[test]
fn timer_microtask_mutation_requests_reflow() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            setTimeout(() => {
                queueMicrotask(() => { document.body.textContent = "micro"; });
            }, 0);
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "micro");
}

#[test]
fn clear_timeout_prevents_callback() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const id = setTimeout(() => {
                document.body.textContent = "should not run";
            }, 0);
            clearTimeout(id);
            "#,
        )
        .unwrap();

    assert!(!runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "");
}

#[test]
fn cancel_animation_frame_prevents_callback() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const id = requestAnimationFrame(() => {
                document.body.textContent = "should not run";
            });
            cancelAnimationFrame(id);
            "#,
        )
        .unwrap();

    assert!(!runtime.has_animation_frame_callbacks());
    assert!(!runtime.drain_animation_frame_callbacks(Instant::now()));
    assert_eq!(text_content(&dom), "");
}

#[test]
fn set_interval_repeats_until_cleared() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            let count = 0;
            const id = setInterval(() => {
                count += 1;
                document.body.textContent = String(count);
                if (count === 2) {
                    clearInterval(id);
                }
            }, 1);
            "#,
        )
        .unwrap();

    let first_deadline = runtime.next_deadline().unwrap();
    assert!(runtime.tick(first_deadline));
    assert_eq!(text_content(&dom), "1");
    let second_deadline = runtime.next_deadline().unwrap();
    assert!(runtime.tick(second_deadline));
    assert_eq!(text_content(&dom), "2");
    assert!(!runtime.tick(second_deadline + Duration::from_millis(1)));
    assert_eq!(text_content(&dom), "2");
}

#[test]
fn cancel_idle_callback_prevents_callback() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const id = requestIdleCallback(() => {
                document.body.textContent = "should not run";
            });
            cancelIdleCallback(id);
            "#,
        )
        .unwrap();

    assert!(!runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "");
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}
