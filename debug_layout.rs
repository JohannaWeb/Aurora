use aurora::dom::Node;
use aurora::css::Stylesheet;
use aurora::style::StyleTree;
use aurora::layout::LayoutTree;

fn main() {
    let dom = Node::document(vec![Node::element(
        "body",
        vec![Node::element("section", vec![Node::text("Border")])],
    )]);
    let stylesheet =
        Stylesheet::parse("section { border: 4px solid ember; padding: 6px; width: 80px; }");
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);

    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 220.0);
    println!("{}", layout);
}
