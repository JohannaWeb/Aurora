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

#[test]
fn forced_sync_reflow_via_offset_width() {
    use crate::css::Stylesheet;
    use crate::layout::{LayoutTree, ViewportSize};
    use crate::style::StyleTree;
    use crate::identity::{Identity, IdentityKind};

    let dom = Parser::new(
        r#"<html><body><div id="box" style="width: 100px; height: 50px;"></div></body></html>"#,
    )
    .parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    let identity = Identity::new("did:human:test", "Test", IdentityKind::Human, []);

    // Setup shared state
    let mut stylesheet_val = Stylesheet::from_dom(&dom, None, &identity);
    stylesheet_val.merge(Stylesheet::user_agent_stylesheet());
    let stylesheet = Rc::new(RefCell::new(stylesheet_val));

    let viewport = Rc::new(RefCell::new(ViewportSize {
        width: 800.0,
        height: 600.0,
    }));
    let style_tree = StyleTree::from_dom(&dom, &stylesheet.borrow());
    let layout_tree = Rc::new(RefCell::new(LayoutTree::from_style_tree_with_viewport(
        &style_tree,
        *viewport.borrow(),
    )));

    runtime.set_shared_state(layout_tree.clone(), stylesheet.clone(), viewport.clone());

    runtime
        .execute(
            r#"
            const box = document.getElementById("box");
            const initialWidth = box.offsetWidth;
            box.style.width = "200px";
            const newWidth = box.offsetWidth;
            document.body.textContent = `${initialWidth},${newWidth}`;
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "100,200");
}

#[test]
fn input_triggered_event_dispatch() {
    let dom =
        Parser::new(r#"<html><body><div id="btn">click me</div></body></html>"#).parse_document();
    let mut runtime = BoaRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const btn = document.getElementById("btn");
            btn.addEventListener("click", () => {
                document.body.textContent = "clicked";
            });
            "#,
        )
        .unwrap();

    let btn_node = runtime
        .execute("document.getElementById('btn')")
        .unwrap()
        .as_object()
        .map(|obj| {
            let registry = &runtime.registry;
            let id = obj
                .get(js_string!("__node_id"), &mut runtime.context)
                .unwrap()
                .as_number()
                .unwrap() as u32;
            registry.lookup(id).unwrap()
        })
        .unwrap();

    assert_eq!(text_content(&dom), "click me");
    runtime.dispatch_event(&btn_node, "click");
    assert_eq!(text_content(&dom), "clicked");
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}
