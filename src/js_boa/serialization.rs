use super::*;

pub(super) fn serialize_outer_html(node: &NodePtr) -> String {
    let mut out = String::new();
    serialize(node, &mut out);
    out
}

pub(super) fn serialize_inner_html(node: &NodePtr) -> String {
    let mut out = String::new();
    match &*node.borrow() {
        Node::Element(el) => {
            for c in &el.children {
                serialize(c, &mut out);
            }
        }
        Node::Document { children } => {
            for c in children {
                serialize(c, &mut out);
            }
        }
        _ => {}
    }
    out
}

pub(super) fn serialize(node: &NodePtr, out: &mut String) {
    match &*node.borrow() {
        Node::Text(t) => out.push_str(t),
        Node::Element(el) => {
            out.push('<');
            out.push_str(&el.tag_name);
            for (k, v) in &el.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(v);
                out.push('"');
            }
            out.push('>');
            for c in &el.children {
                serialize(c, out);
            }
            out.push_str("</");
            out.push_str(&el.tag_name);
            out.push('>');
        }
        Node::Document { children } => {
            for c in children {
                serialize(c, out);
            }
        }
    }
}
