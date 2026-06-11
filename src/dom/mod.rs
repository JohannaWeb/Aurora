//! DOM node model.
//!
//! Public API: node types plus constructors and tree operations.

mod display;
mod node;
mod serialize_html;

pub use node::{DocumentMode, ElementNode, Node, NodePtr};
pub(crate) use serialize_html::serialize_outer_html;

/// Serialize an SVG DOM node back to an SVG markup string.
/// Used by the painter to render inline `<svg>` elements via usvg.
pub fn serialize_svg_node(node: &NodePtr) -> String {
    let mut out = String::new();
    serialize_node(node, &mut out);
    out
}

fn serialize_node(node: &NodePtr, out: &mut String) {
    match &*node.borrow() {
        Node::Element(el) => {
            out.push('<');
            out.push_str(&el.tag_name);
            for (name, value) in &el.attributes {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                out.push_str(&html_escape(value));
                out.push('"');
            }
            if el.children.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for child in &el.children {
                    serialize_node(child, out);
                }
                out.push_str("</");
                out.push_str(&el.tag_name);
                out.push('>');
            }
        }
        Node::Text(text) => {
            out.push_str(&html_escape(text));
        }
        Node::Document { children, .. } => {
            for child in children {
                serialize_node(child, out);
            }
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
