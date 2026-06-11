use crate::css::selectors_impl::{AuroraSelectorImpl, element_matches, parse_selector_list};
use crate::css::ElementData;
use crate::dom::{Node, NodePtr};
use ::selectors::parser::Selector;
use std::rc::Rc;

// ─── Public API ───────────────────────────────────────────────────────────────

pub(crate) fn selector_matches(node: &NodePtr, selector: &str, root: &NodePtr) -> bool {
    let selectors = parse_selectors(selector);
    node_matches_any(&selectors, node, root)
}

pub(crate) fn query_first(root: &NodePtr, selector: &str, start_node: &NodePtr) -> Option<NodePtr> {
    let selectors = parse_selectors(selector);
    query_first_rec(start_node, &selectors, root, true)
}

pub(crate) fn query_all(root: &NodePtr, selector: &str, start_node: &NodePtr) -> Vec<NodePtr> {
    let selectors = parse_selectors(selector);
    let mut out = Vec::new();
    query_all_rec(start_node, &selectors, root, &mut out, true);
    out
}

pub(crate) fn find_by_id(node: &NodePtr, id: &str) -> Option<NodePtr> {
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
        Node::Document { children, .. } => {
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

pub(crate) fn collect_by_tag(node: &NodePtr, tag: &str, out: &mut Vec<NodePtr>) {
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
        Node::Document { children, .. } => {
            for c in children {
                collect_by_tag(c, tag, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn collect_by_class(node: &NodePtr, class: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if let Some(cls) = el.attributes.get("class") {
                if cls.split_whitespace().any(|c| c == class) {
                    out.push(node.clone());
                }
            }
            for c in &el.children {
                collect_by_class(c, class, out);
            }
        }
        Node::Document { children, .. } => {
            for c in children {
                collect_by_class(c, class, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn find_parent(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
    let kids: Vec<NodePtr> = match &*root.borrow() {
        Node::Document { children, .. } => children.clone(),
        Node::Element(el) => el.children.clone(),
        _ => return None,
    };
    for child in &kids {
        if Rc::ptr_eq(child, target) {
            return Some(root.clone());
        }
        if let Some(found) = find_parent(child, target) {
            return Some(found);
        }
    }
    None
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn parse_selectors(selector: &str) -> Vec<Selector<AuroraSelectorImpl>> {
    parse_selector_list(selector)
        .map(|list| list.slice().to_vec())
        .unwrap_or_default()
}

fn node_matches_any(
    selectors: &[Selector<AuroraSelectorImpl>],
    node: &NodePtr,
    root: &NodePtr,
) -> bool {
    let element_data = match &*node.borrow() {
        Node::Element(el) => ElementData {
            tag_name: el.tag_name.clone(),
            attributes: el.attributes.clone(),
        },
        _ => return false,
    };
    let ancestors = build_ancestor_chain(root, node);
    let siblings = build_sibling_list(root, node);
    let sibling_index = sibling_index_of(root, node);

    selectors
        .iter()
        .any(|sel| element_matches(sel, &element_data, &ancestors, &siblings, sibling_index))
}

fn build_ancestor_chain(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let mut chain = Vec::new();
    if let Some(parent) = find_parent(root, target) {
        chain = build_ancestor_chain(root, &parent);
        if let Node::Element(el) = &*parent.borrow() {
            chain.push(ElementData {
                tag_name: el.tag_name.clone(),
                attributes: el.attributes.clone(),
            });
        }
    }
    chain
}

fn build_sibling_list(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let Some(parent) = find_parent(root, target) else {
        return vec![];
    };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return vec![],
    };
    children
        .iter()
        .filter_map(|child| {
            if let Node::Element(el) = &*child.borrow() {
                Some(ElementData {
                    tag_name: el.tag_name.clone(),
                    attributes: el.attributes.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn sibling_index_of(root: &NodePtr, target: &NodePtr) -> usize {
    let Some(parent) = find_parent(root, target) else {
        return 0;
    };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return 0,
    };
    let mut idx = 0;
    for child in &children {
        if Rc::ptr_eq(child, target) {
            return idx;
        }
        if matches!(&*child.borrow(), Node::Element(_)) {
            idx += 1;
        }
    }
    0
}

fn query_first_rec(
    node: &NodePtr,
    selectors: &[Selector<AuroraSelectorImpl>],
    root: &NodePtr,
    skip_self: bool,
) -> Option<NodePtr> {
    if !skip_self && node_matches_any(selectors, node, root) {
        return Some(node.clone());
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    };
    for child in kids {
        if let Some(found) = query_first_rec(&child, selectors, root, false) {
            return Some(found);
        }
    }
    None
}

fn query_all_rec(
    node: &NodePtr,
    selectors: &[Selector<AuroraSelectorImpl>],
    root: &NodePtr,
    out: &mut Vec<NodePtr>,
    skip_self: bool,
) {
    if !skip_self && node_matches_any(selectors, node, root) {
        out.push(node.clone());
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    };
    for child in kids {
        query_all_rec(&child, selectors, root, out, false);
    }
}
