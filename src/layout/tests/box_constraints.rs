use crate::css::Stylesheet;
use crate::dom::Node;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;

#[test]
fn applies_margin_and_padding_to_block_layout() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Box")])],
    )]);
    let stylesheet = Stylesheet::parse("section { margin: 10px 12px; padding: 4px 6px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
    let rendered = layout.to_string();
    println!(
        "DEBUG applies_margin_and_padding_to_block_layout:\n{}",
        rendered
    );

    assert!(rendered.contains(
        "block<section> {margin: 10px 12px, padding: 4px 6px} [x: 12, y: 10, w: 176, h: 33]"
    ));
    // Box is text content. 3 chars * 16 = 48.
    // x = 12 (margin) + 6 (padding) = 18.
    // y = 10 (margin) + 4 (padding) = 14.
    assert!(rendered.contains("text(\"Box\") [x: 18, y: 14, w: 48, h: 19]"));
}

#[test]
fn includes_border_width_in_box_geometry() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Border")])],
    )]);
    let stylesheet =
        Stylesheet::parse("section { border: 4px solid ember; padding: 6px; width: 80px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 220.0);
    let rendered = layout.to_string();
    println!("DEBUG includes_border_width_in_box_geometry:\n{}", rendered);

    assert!(rendered.contains("block<section> {border: 4px solid ember, padding: 6px, width: 80px} [x: 0, y: 0, w: 100, h: 45]"));
    assert!(rendered.contains("text(\"Border\") [x: 10, y: 10, w: 96, h: 19]"));
}

#[test]
fn applies_fixed_width_and_height_to_block_boxes() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Sized")])],
    )]);
    let stylesheet = Stylesheet::parse("section { width: 120px; height: 48px; padding: 4px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 300.0);
    let rendered = layout.to_string();

    // h = 48 + 8 = 56. w = 120 + 8 = 128.
    assert!(rendered.contains(
        "block<section> {height: 48px, padding: 4px, width: 120px} [x: 0, y: 0, w: 128, h: 56]"
    ));
    assert!(rendered.contains("text(\"Sized\") [x: 4, y: 4, w: 80, h: 19]"));
}

#[test]
fn resolves_viewport_height_units() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("main", vec![Node::text("Tall")])],
    )]);
    let stylesheet = Stylesheet::parse("main { min-height: 50vh; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport(
        &style_tree,
        ViewportSize {
            width: 300.0,
            height: 800.0,
        },
    );
    let rendered = layout.to_string();

    assert!(rendered.contains("block<main> {min-height: 50vh} [x: 0, y: 0, w: 300, h: 400]"));
}

#[test]
fn rebuilds_layout_with_new_viewport_width() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("main", vec![Node::text("Reflow")])],
    )]);
    let stylesheet = Stylesheet::parse("main { width: 50%; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let wide = LayoutTree::from_style_tree_with_viewport(
        &style_tree,
        ViewportSize {
            width: 800.0,
            height: 600.0,
        },
    )
    .to_string();
    let narrow = LayoutTree::from_style_tree_with_viewport(
        &style_tree,
        ViewportSize {
            width: 400.0,
            height: 600.0,
        },
    )
    .to_string();

    assert!(wide.contains("block<main> {width: 50%} [x: 0, y: 0, w: 400"));
    assert!(narrow.contains("block<main> {width: 50%} [x: 0, y: 0, w: 200"));
}

#[test]
fn constrains_inline_wrapping_with_fixed_width() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "p",
            vec![Node::text("one two three four five")],
        )],
    )]);
    let stylesheet = Stylesheet::parse("p { display: inline; width: 64px; padding: 4px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
    let rendered = layout.to_string();
    println!(
        "DEBUG constrains_inline_wrapping_with_fixed_width:\n{}",
        rendered
    );

    assert!(rendered.contains("inline<p> {display: inline, padding: 4px, width: 64px}"));
    assert!(rendered.contains("text(\"one\") [x: 4, y: 4, w: 48, h: 19]"));
    assert!(rendered.contains("text(\"two\")"));
    assert!(rendered.contains("text(\"three\")"));
    assert!(rendered.contains("text(\"four\")"));
    assert!(rendered.contains("text(\"five\")"));
}

#[test]
fn aligns_inline_text_horizontally() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Center")])],
    )]);
    let stylesheet = Stylesheet::parse("p { display: inline; text-align: center; width: 100px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
    let rendered = layout.to_string();

    // "Center" is 6 chars. 6 * 16.0 = 96.0 px.
    // alignment offset = (100 - 96) / 2 = 2.0
    assert!(rendered.contains("text(\"Center\") [x: 2"));
}
