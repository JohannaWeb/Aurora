use crate::dom::{Node, NodePtr};

pub(crate) fn serialize_outer_html(node: &NodePtr) -> String {
    let mut buf = String::with_capacity(4096);
    serialize_node(node, &mut buf);
    buf
}

fn serialize_node(node: &NodePtr, out: &mut String) {
    match &*node.borrow() {
        Node::Document { children, .. } => {
            for child in children {
                serialize_node(child, out);
            }
        }
        Node::Element(el) => {
            out.push('<');
            out.push_str(&el.tag_name);
            for (k, v) in &el.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(&html_escape(v));
                out.push('"');
            }
            out.push('>');
            for child in &el.children {
                serialize_node(child, out);
            }
            if !is_void(&el.tag_name) {
                out.push_str("</");
                out.push_str(&el.tag_name);
                out.push('>');
            }
        }
        Node::Text(t) => {
            out.push_str(&html_escape(t));
        }
    }
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input"
            | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
