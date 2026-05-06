use crate::css::Stylesheet;
use crate::dom::Node;
use crate::layout::LayoutTree;
use crate::style::StyleTree;

#[test]
fn clamps_block_width_and_height_with_min_and_max() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element("section", vec![Node::text("Min")]),
            Node::element("article", vec![Node::text("Max")]),
        ],
    )]);
    let stylesheet = Stylesheet::parse(
            "section { width: 40px; min-width: 80px; height: 12px; min-height: 24px; padding: 4px; } article { width: 180px; max-width: 96px; height: 120px; max-height: 40px; padding: 4px; }",
        );
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
    let rendered = layout.to_string();

    assert!(rendered.contains("block<section> {height: 12px, min-height: 24px, min-width: 80px, padding: 4px, width: 40px} [x: 0, y: 0, w: 88, h: 32]"));
    assert!(rendered.contains("block<article> {height: 120px, max-height: 40px, max-width: 96px, padding: 4px, width: 180px} [x: 0, y: 32, w: 104, h: 48]"));
}

#[test]
fn collapses_vertical_margins_between_block_siblings() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element("section", vec![Node::text("One")]),
            Node::element("section", vec![Node::text("Two")]),
        ],
    )]);
    let stylesheet =
        Stylesheet::parse("section { margin-top: 12px; margin-bottom: 18px; padding: 4px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
    let rendered = layout.to_string();

    // section h=18. body h = 12 (top) + 18 + 18 + 18 (collapsed bottom if?) = wait.
    // section 1 margin-top 12. y starts at 12.
    // section 1 bottom 18, section 2 top 12. collapsed to 18.
    // section 2 starts at 12 + 18 + 18 = 48.
    assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 12, w: 240, h: 33]"));
    assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 63, w: 240, h: 33]"));
}

#[test]
fn clamps_inline_width_before_wrapping() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "p",
            vec![Node::text("one two three four five")],
        )],
    )]);
    let stylesheet = Stylesheet::parse(
        "p { display: inline; width: 140px; max-width: 64px; min-height: 60px; padding: 4px; }",
    );
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
    let rendered = layout.to_string();
    println!("DEBUG clamps_inline_width_before_wrapping:\n{}", rendered);

    assert!(rendered.contains("inline<p> {display: inline, max-width: 64px, min-height: 60px, padding: 4px, width: 140px}"));
    assert!(rendered.contains("text(\"one\")"));
    assert!(rendered.contains("text(\"two\")"));
    assert!(rendered.contains("text(\"three\")"));
    assert!(rendered.contains("text(\"four\")"));
    assert!(rendered.contains("text(\"five\")"));
}

#[test]
fn omits_nodes_with_display_none() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Hidden")])],
    )]);
    let stylesheet = Stylesheet::parse("p { display: none; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree(&style_tree);
    let rendered = layout.to_string();

    assert!(!rendered.contains("<p>"));
    assert!(!rendered.contains("Hidden"));
}

#[test]
fn includes_border_width_in_inline_box_geometry() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("span", vec![Node::text("Hi")])],
    )]);
    let stylesheet =
        Stylesheet::parse("span { display: inline; border: 4px solid ember; padding: 2px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
    let rendered = layout.to_string();

    assert!(rendered.contains(
            "inline<span> {border: 4px solid ember, display: inline, padding: 2px} [x: 0, y: 0, w: 44, h: 31]"
        ));
    assert!(rendered.contains("text(\"Hi\") [x: 6, y: 6, w: 32, h: 19]"));
}

#[test]
fn lays_out_images_with_attributes_as_replaced_boxes() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element_with_attributes(
            "img",
            [
                ("alt".to_string(), "grumpy cat".to_string()),
                ("src".to_string(), "cat.txt".to_string()),
                ("width".to_string(), "120".to_string()),
                ("height".to_string(), "80".to_string()),
            ]
            .into_iter()
            .collect(),
            Vec::new(),
        )],
    )]);
    let stylesheet =
        Stylesheet::parse("img { display: inline; padding: 4px; border: 2px solid ember; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
    let rendered = layout.to_string();

    assert!(rendered.contains(
            "inline<img alt=Some(\"grumpy cat\") src=Some(\"cat.txt\")> {border: 2px solid ember, display: inline, padding: 4px} [x: 0, y: 0, w: 132, h: 92]"
        ));
}
