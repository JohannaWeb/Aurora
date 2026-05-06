use super::{DebugPainter, Painter};
use crate::css::Stylesheet;
use crate::dom::Node;
use crate::layout::LayoutTree;
use crate::style::StyleTree;

fn layout_for(dom: crate::dom::NodePtr, css: &str, width: Option<f32>) -> LayoutTree {
    let stylesheet = Stylesheet::parse(css);
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    match width {
        Some(width) => LayoutTree::from_style_tree_with_viewport_width(&style_tree, width),
        None => LayoutTree::from_style_tree(&style_tree),
    }
}

#[test]
fn paints_text_into_framebuffer() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Hello")])],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "p { display: inline; color: blue; }",
        None,
    ))
    .to_string();

    assert!(rendered.contains("Hello"));
}

#[test]
fn paints_colored_boxes_with_different_fill_chars() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![
            Node::element("h1", vec![Node::text("Title")]),
            Node::element("p", vec![Node::text("Body")]),
        ],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "body { color: cyan; } p { display: inline; color: paper-white; }",
        None,
    ))
    .to_string();

    assert!(rendered.contains("c"));
    assert!(rendered.contains("p"));
}

#[test]
fn paints_backgrounds_and_borders_as_distinct_layers() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Box")])],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "section { margin: 8px; padding: 8px; background-color: sand; border: 16px solid ember; }",
        Some(160.0),
    ))
    .to_string();

    assert!(rendered.contains("E"));
    assert!(rendered.contains("s"));
    assert!(rendered.contains("Box"));
}

#[test]
fn draws_underline_for_text_decoration() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Link")])],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "p { display: inline; text-decoration: underline; line-height: 28px; }",
        None,
    ))
    .to_string();

    assert!(rendered.contains("Link"));
    assert!(rendered.contains("_"));
}

#[test]
fn skips_box_when_opacity_is_zero() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Invisible")])],
    )]);
    let rendered =
        Painter::paint(&layout_for(dom, "section { opacity: 0; }", Some(160.0))).to_string();

    assert!(!rendered.contains("Invisible"));
}

#[test]
fn hides_box_when_visibility_is_hidden() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Hidden")])],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "section { visibility: hidden; background-color: sand; height: 40px; }",
        Some(160.0),
    ))
    .to_string();

    assert!(!rendered.contains("Hidden"));
    assert!(rendered.contains(":"));
}

#[test]
fn debug_painter_draws_box_outlines() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Hello")])],
    )]);
    let rendered = DebugPainter::paint(&layout_for(
        dom,
        "p { display: inline; padding: 20px; }",
        None,
    ))
    .to_string();

    assert!(rendered.contains("+"));
    assert!(rendered.contains("-"));
    assert!(rendered.contains("|"));
}

#[test]
fn debug_painter_lists_all_boxes() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element(
            "section",
            vec![Node::element("p", vec![Node::text("Text")])],
        )],
    )]);
    let rendered = DebugPainter::paint(&layout_for(dom, "", None)).to_string();

    assert!(rendered.contains("viewport"));
    assert!(rendered.contains("block<body>"));
    assert!(rendered.contains("block<section>"));
    assert!(rendered.contains("block<p>"));
    assert!(rendered.contains("Boxes:"));
}

#[test]
fn debug_painter_shows_coordinates() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("p", vec![Node::text("Hi")])],
    )]);
    let rendered = DebugPainter::paint(&layout_for(
        dom,
        "p { width: 120px; height: 48px; }",
        Some(200.0),
    ))
    .to_string();

    assert!(rendered.contains("x="));
    assert!(rendered.contains("y="));
    assert!(rendered.contains("w="));
    assert!(rendered.contains("h="));
}

#[test]
fn paints_image_placeholders_with_alt_text() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element_with_attributes(
            "img",
            [
                ("alt".to_string(), "cat loaf".to_string()),
                ("src".to_string(), "cat.txt".to_string()),
                ("width".to_string(), "96".to_string()),
                ("height".to_string(), "48".to_string()),
            ]
            .into_iter()
            .collect(),
            Vec::new(),
        )],
    )]);
    let rendered = Painter::paint(&layout_for(
        dom,
        "img { display: inline; border: 2px solid ember; }",
        Some(200.0),
    ))
    .to_string();

    assert!(rendered.contains("@"));
    assert!(rendered.contains("[cat loaf]"));
}
