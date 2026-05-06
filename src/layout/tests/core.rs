use crate::css::Stylesheet;
use crate::dom::Node;
use crate::layout::LayoutTree;
use crate::style::StyleTree;

#[test]
fn builds_layout_boxes_with_geometry() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Hello")])],
    )]);
    let stylesheet = Stylesheet::parse("p { display: inline; color: blue; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree(&style_tree);
    let rendered = layout.to_string();

    assert!(rendered.contains("viewport [x: 0, y: 0, w: 1200"));
    assert!(rendered.contains("block<body> {} [x: 0, y: 0, w: 1200"));
    assert!(rendered.contains("inline<p> {color: blue, display: inline}"));
    assert!(rendered.contains("text(\"Hello\") [x: 0, y: 0"));
}

#[test]
fn stacks_block_children_vertically() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element("section", vec![Node::text("One")]),
            Node::element("section", vec![Node::text("Two")]),
        ],
    )]);
    let stylesheet = Stylesheet::parse("");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree(&style_tree);
    let rendered = layout.to_string();

    assert_eq!(rendered.matches("block<section> {}").count(), 2);
    assert!(rendered.contains("text(\"One\") [x: 0, y: 0"));
    assert!(rendered.contains("text(\"Two\")"));
}

#[test]
fn treats_flex_children_as_block_flow_items() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element("nav", vec![Node::element("a", vec![Node::text("One")])]),
            Node::element("section", vec![Node::text("Two")]),
        ],
    )]);
    let stylesheet =
        Stylesheet::parse("nav { display: flex; height: 40px; } section { display: flex; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 300.0);
    let rendered = layout.to_string();

    assert!(rendered.contains("block<nav> {display: flex, height: 40px} [x: 0, y: 0"));
    assert!(rendered.contains("block<section> {display: flex} [x: 0, y: 40"));
}

#[test]
fn keeps_percentage_width_flex_items_explicit() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "nav",
            vec![
                Node::element("span", vec![Node::text("A")]),
                Node::element("div", vec![Node::text("Wide")]),
            ],
        )],
    )]);
    let stylesheet = Stylesheet::parse("nav { display: flex; } div { display: flex; width: 50%; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 400.0);
    let rendered = layout.to_string();

    assert!(rendered.contains("block<div> {display: flex, width: 50%} [x: 16, y: 0, w: 200"));
}

#[test]
fn wraps_inline_text_across_multiple_lines() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "p",
            vec![Node::text("alpha beta gamma delta epsilon zeta")],
        )],
    )]);
    let stylesheet = Stylesheet::parse("p { display: inline; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 96.0);
    let rendered = layout.to_string();
    println!(
        "DEBUG wraps_inline_text_across_multiple_lines:\n{}",
        rendered
    );

    assert!(rendered.contains("inline<p> {display: inline}"));
    assert!(rendered.contains("text(\"alpha\") [x: 0, y: 0, w: 80, h: 19]"));
    assert!(rendered.contains("text(\"beta\") [x: 0, y: 19, w: 64, h: 19]"));
    assert!(rendered.contains("text(\"gamma\") [x: 0, y: 38, w: 80, h: 19]"));
}

#[test]
fn keeps_nowrap_inline_text_on_one_line() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("sign in")])],
    )]);
    let stylesheet = Stylesheet::parse("p { display: inline; white-space: nowrap; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 48.0);
    let rendered = layout.to_string();

    assert!(rendered.contains("text(\"sign in\") [x: 0, y: 0"));
    assert!(!rendered.contains("text(\"sign\")"));
    assert!(!rendered.contains("text(\"in\")"));
}

#[test]
fn inherits_nowrap_to_child_text() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "nav",
            vec![Node::element("span", vec![Node::text("dom 412")])],
        )],
    )]);
    let stylesheet = Stylesheet::parse(
        "nav { display: inline; white-space: nowrap; } span { display: inline; }",
    );
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 48.0);
    let rendered = layout.to_string();

    assert!(rendered.contains("text(\"dom 412\") [x: 0, y: 0"));
    assert!(!rendered.contains("text(\"dom\")"));
    assert!(!rendered.contains("text(\"412\")"));
}

#[test]
fn wraps_inline_children_when_the_row_fills() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "span",
            vec![
                Node::element("em", vec![Node::text("hello")]),
                Node::element("strong", vec![Node::text("world")]),
            ],
        )],
    )]);
    let stylesheet = Stylesheet::parse(
        "span { display: inline; } em { display: inline; } strong { display: inline; }",
    );
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 72.0);
    let rendered = layout.to_string();
    println!(
        "DEBUG wraps_inline_children_when_the_row_fills:\n{}",
        rendered
    );

    assert!(rendered.contains("inline<em> {display: inline}"));
    assert!(rendered.contains("inline<strong> {display: inline}"));
    assert!(rendered.contains("text(\"hello\")"));
    assert!(rendered.contains("text(\"world\")"));
}
