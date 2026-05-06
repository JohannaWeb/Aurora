use super::*;

pub(in crate::js_boa) fn find_by_id(node: &NodePtr, id: &str) -> Option<NodePtr> {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.attributes.get("id").map(|s| s.as_str()) == Some(id) {
                drop(b);
                return Some(node.clone());
            }
            for c in &el.children {
                if let Some(found) = find_by_id(c, id) {
                    return Some(found);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if let Some(found) = find_by_id(c, id) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

pub(in crate::js_boa) fn find_by_tag(node: &NodePtr, tag: &str) -> Option<NodePtr> {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.tag_name.eq_ignore_ascii_case(tag) {
                drop(b);
                return Some(node.clone());
            }
            for c in &el.children {
                if let Some(found) = find_by_tag(c, tag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if let Some(found) = find_by_tag(c, tag) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

pub(in crate::js_boa) fn collect_by_tag(node: &NodePtr, tag: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if tag == "*" || el.tag_name.eq_ignore_ascii_case(tag) {
                out.push(node.clone());
            }
            for c in &el.children {
                collect_by_tag(c, tag, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_tag(c, tag, out);
            }
        }
        _ => {}
    }
}

pub(in crate::js_boa) fn collect_by_class(node: &NodePtr, cls: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if let Some(v) = el.attributes.get("class") {
                if v.split_whitespace().any(|c| c == cls) {
                    out.push(node.clone());
                }
            }
            for c in &el.children {
                collect_by_class(c, cls, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_class(c, cls, out);
            }
        }
        _ => {}
    }
}

pub(in crate::js_boa) fn collect_by_attr(
    node: &NodePtr,
    key: &str,
    value: &str,
    out: &mut Vec<NodePtr>,
) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.attributes.get(key).map(|s| s.as_str()) == Some(value) {
                out.push(node.clone());
            }
            for c in &el.children {
                collect_by_attr(c, key, value, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_attr(c, key, value, out);
            }
        }
        _ => {}
    }
}
