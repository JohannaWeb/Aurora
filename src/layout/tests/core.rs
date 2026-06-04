use crate::css::Stylesheet;
use crate::dom::Node;
use crate::html::Parser;
use crate::identity::Identity;
use crate::layout::LayoutTree;
use crate::style::StyleTree;

#[test]
fn demo_flex_nav_items_stay_inside_header() {
    let dom = Parser::new(include_str!("../../../fixtures/demo/index.html")).parse_document();
    let identity = Identity::new("did:human:test", "Test", []);
    let mut stylesheet = Stylesheet::from_dom(&dom, None, &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 960.0);
    let rendered = layout.to_string();

    assert!(
        right_edge_of_text(&rendered, "About") <= 900.0,
        "nav item escaped the 960px viewport:\n{rendered}"
    );
}

#[test]
fn mixed_inline_elements_wrap_without_overlap() {
    let dom = Parser::new(include_str!("../../../fixtures/demo/index.html")).parse_document();
    let identity = Identity::new("did:human:test", "Test", []);
    let mut stylesheet = Stylesheet::from_dom(&dom, None, &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 960.0);
    let rendered = layout.to_string();

    let align = text_box(&rendered, "align-items: center");
    let suffix = text_box(&rendered, ". The navigation items utilize");
    assert_eq!(
        align.1, suffix.1,
        "inline siblings should share the same wrapped line:\n{rendered}"
    );
    assert!(
        align.0 + align.2 <= suffix.0,
        "inline siblings overlap after wrapping:\n{rendered}"
    );
}

#[test]
fn empty_td_does_not_crowd_out_sibling_cells() {
    // Mimics HN subtext row: <tr><td colspan="2"></td><td class="subtext">...</td></tr>
    // The empty td must not fill the entire flex row and push the subtext off-screen.
    let dom = Node::document(vec![Node::element(
        "table",
        vec![Node::element(
            "tr",
            vec![
                Node::element("td", vec![]),
                Node::element(
                    "td",
                    vec![Node::element(
                        "span",
                        vec![Node::text("123 points by user")],
                    )],
                ),
            ],
        )],
    )]);
    let stylesheet = Stylesheet::user_agent_stylesheet();
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);
    let rendered = layout.to_string();
    println!("EMPTY TD LAYOUT:\n{}", rendered);

    // The subtext content must be visible (x < 400, not pushed off-screen)
    let x_of = |label: &str| -> f32 {
        let prefix = format!("text(\"{label}\") [x:");
        rendered
            .find(&prefix)
            .and_then(|pos| {
                let after = rendered[pos + prefix.len()..].trim_start();
                after
                    .split(|c: char| !c.is_numeric() && c != '.')
                    .next()
                    .and_then(|s| s.parse::<f32>().ok())
            })
            .unwrap_or(9999.0)
    };

    let x = x_of("123 points by user");
    assert!(
        x < 400.0,
        "subtext content was pushed off-screen (x={x}); empty td is eating the row\n{rendered}"
    );
}

fn right_edge_of_text(rendered: &str, label: &str) -> f32 {
    let (x, _, width) = text_box(rendered, label);
    x + width
}

fn text_box(rendered: &str, label: &str) -> (f32, f32, f32) {
    let prefix = format!("text(\"{label}\") [x:");
    let pos = rendered
        .find(&prefix)
        .unwrap_or_else(|| panic!("text box not found for {label:?}\n{rendered}"));
    let line = rendered[pos + prefix.len()..]
        .lines()
        .next()
        .unwrap_or_default();
    let x = parse_metric(line, "");
    let y = parse_metric(line, ", y: ");
    let width = parse_metric(line, ", w: ");
    (x, y, width)
}

fn parse_metric(line: &str, marker: &str) -> f32 {
    let value = if marker.is_empty() {
        line
    } else {
        let pos = line
            .find(marker)
            .unwrap_or_else(|| panic!("metric marker {marker:?} not found in {line:?}"));
        &line[pos + marker.len()..]
    };
    value
        .trim_start()
        .split(|c: char| !c.is_numeric() && c != '.')
        .next()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or_else(|| panic!("metric value not found in {line:?}"))
}

#[test]
fn nested_table_nav_cells_stay_horizontal() {
    // Mimics HN: outer-table > tr > td > inner-table > tr > [td:logo][td:nav][td:login]
    // nav links inside the middle td should all be on one Y line
    let dom = Node::document(vec![Node::element(
        "table",
        vec![Node::element(
            "tr",
            vec![Node::element(
                "td",
                vec![Node::element(
                    "table",
                    vec![Node::element(
                        "tr",
                        vec![
                            Node::element("td", vec![Node::text("*")]),
                            Node::element(
                                "td",
                                vec![Node::element(
                                    "span",
                                    vec![
                                        Node::element("b", vec![Node::text("Hacker News")]),
                                        Node::text(" "),
                                        Node::element("a", vec![Node::text("new")]),
                                        Node::text(" | "),
                                        Node::element("a", vec![Node::text("comments")]),
                                    ],
                                )],
                            ),
                            Node::element(
                                "td",
                                vec![Node::element("a", vec![Node::text("login")])],
                            ),
                        ],
                    )],
                )],
            )],
        )],
    )]);
    let stylesheet = Stylesheet::user_agent_stylesheet();
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);
    let rendered = layout.to_string();
    println!("NESTED NAV LAYOUT:\n{}", rendered);

    let y_of = |label: &str| -> f32 {
        let prefix = format!("text(\"{label}\") [x:");
        rendered
            .find(&prefix)
            .and_then(|pos| {
                let after = &rendered[pos..];
                after.find(", y: ").and_then(|y_pos| {
                    let y_str = &after[y_pos + 5..];
                    y_str
                        .split(|c: char| !c.is_numeric() && c != '.')
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                })
            })
            .unwrap_or(-1.0)
    };

    let y_hn = y_of("Hacker News");
    let y_new = y_of("new");
    let y_comments = y_of("comments");
    assert_eq!(
        y_hn, y_new,
        "'Hacker News' and 'new' must be on the same line (y={y_hn} vs {y_new})\n{rendered}"
    );
    assert_eq!(
        y_new, y_comments,
        "'new' and 'comments' must be on the same line (y={y_new} vs {y_comments})\n{rendered}"
    );
}

#[test]
fn table_row_keeps_inline_nav_on_one_line() {
    let dom = Node::document(vec![Node::element(
        "table",
        vec![Node::element(
            "tr",
            vec![Node::element(
                "td",
                vec![Node::element(
                    "span",
                    vec![
                        Node::element("b", vec![Node::text("HN")]),
                        Node::text(" "),
                        Node::element("a", vec![Node::text("new")]),
                        Node::text(" | "),
                        Node::element("a", vec![Node::text("past")]),
                    ],
                )],
            )],
        )],
    )]);
    let stylesheet = Stylesheet::user_agent_stylesheet();
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);
    let rendered = layout.to_string();
    println!("NAV LAYOUT:\n{}", rendered);

    // All three inline items should be at the same Y
    let y_of = |label: &str| -> f32 {
        let prefix = format!("text(\"{label}\") [x:");
        rendered
            .find(&prefix)
            .and_then(|pos| {
                let after = &rendered[pos..];
                after.find(", y: ").and_then(|y_pos| {
                    let y_str = &after[y_pos + 5..];
                    y_str
                        .split(|c: char| !c.is_numeric() && c != '.')
                        .next()
                        .and_then(|s| s.parse::<f32>().ok())
                })
            })
            .unwrap_or(-1.0)
    };

    let y_hn = y_of("HN");
    let y_new = y_of("new");
    let y_past = y_of("past");
    assert_eq!(
        y_hn, y_new,
        "HN and 'new' should be on the same line (y_hn={y_hn}, y_new={y_new})\n{rendered}"
    );
    assert_eq!(
        y_new, y_past,
        "'new' and 'past' should be on the same line (y_new={y_new}, y_past={y_past})\n{rendered}"
    );
}

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

    assert!(rendered.contains("block<div> {display: flex, width: 50%} [x:"));
    assert!(rendered.contains("w: 200"));
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
    for word in ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"] {
        assert!(
            rendered.contains(word),
            "missing wrapped word: {word}\n{rendered}"
        );
    }
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

#[test]
fn display_none_children_produce_no_layout_boxes() {
    // A display:none element (style) must not render its text children.
    // Build a DOM with <style> in both <head> (display:none via UA) and
    // a standalone display:none div, then confirm no CSS text appears.
    let dom = Node::document(vec![Node::element(
        "html",
        vec![
            Node::element(
                "head",
                vec![Node::element(
                    "style",
                    vec![Node::text("body{background:#eee;width:60vw}")],
                )],
            ),
            Node::element("body", vec![Node::element("p", vec![Node::text("Hello")])]),
        ],
    )]);

    let stylesheet = Stylesheet::user_agent_stylesheet();
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);
    let rendered = layout.to_string();

    assert!(
        !rendered.contains("background:#eee"),
        "CSS text from <style> leaked into layout tree:\n{rendered}"
    );
    // Sanity check: actual content is present
    assert!(
        rendered.contains("Hello"),
        "Expected body content to render"
    );
}

#[test]
fn parsed_html_style_text_does_not_reach_layout() {
    // Use the real html5ever parser, NOT a manually built DOM.
    use crate::html::Parser;
    let html = r#"<!DOCTYPE html>
<html><head>
<style>body{background:#eee;width:60vw}a:link,a:visited{color:#348}</style>
</head><body><p>Hello</p></body></html>"#;

    let dom = Parser::new(html).parse_document();
    let stylesheet = Stylesheet::user_agent_stylesheet();
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);
    let rendered = layout.to_string();

    println!("Layout tree:\n{rendered}");

    assert!(
        !rendered.contains("background:#eee"),
        "CSS text leaked into layout:\n{rendered}"
    );
    assert!(rendered.contains("Hello"));
}
