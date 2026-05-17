use crate::css::{ElementData, Selector};
use super::*;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Returns true if `node` matches `selector` (considering its ancestors via `root`).
pub(in crate::js_boa) fn selector_matches(
    node: &NodePtr,
    selector: &str,
    root: &NodePtr,
) -> bool {
    parse_selectors(selector)
        .iter()
        .any(|sel| node_matches(sel, node, root))
}

pub(in crate::js_boa) fn query_first(root: &NodePtr, selector: &str) -> Option<NodePtr> {
    let selectors = parse_selectors(selector);
    query_first_rec(root, &selectors, root, true)
}

pub(in crate::js_boa) fn query_all(root: &NodePtr, selector: &str) -> Vec<NodePtr> {
    let selectors = parse_selectors(selector);
    let mut out = Vec::new();
    query_all_rec(root, &selectors, root, &mut out, true);
    out
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn parse_selectors(selector: &str) -> Vec<Selector> {
    selector
        .split(',')
        .filter_map(|s| Selector::parse(s.trim()))
        .collect()
}

fn node_matches(selector: &Selector, node: &NodePtr, root: &NodePtr) -> bool {
    let element_data = match &*node.borrow() {
        Node::Element(el) => ElementData {
            tag_name: el.tag_name.clone(),
            attributes: el.attributes.clone(),
        },
        _ => return false,
    };
    let ancestors = build_ancestor_chain(root, node);
    selector.matches(&element_data, &ancestors)
}

/// Build the ancestor ElementData chain from outermost to immediate parent.
fn build_ancestor_chain(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let mut chain = Vec::new();
    let mut cursor = find_parent(root, target);
    while let Some(node) = cursor {
        if let Node::Element(el) = &*node.borrow() {
            chain.push(ElementData {
                tag_name: el.tag_name.clone(),
                attributes: el.attributes.clone(),
            });
        }
        let parent = find_parent(root, &node);
        cursor = parent;
    }
    chain.reverse(); // root → immediate parent order
    chain
}

fn query_first_rec(
    node: &NodePtr,
    selectors: &[Selector],
    root: &NodePtr,
    skip_self: bool,
) -> Option<NodePtr> {
    if !skip_self && selectors.iter().any(|s| node_matches(s, node, root)) {
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
    selectors: &[Selector],
    root: &NodePtr,
    out: &mut Vec<NodePtr>,
    skip_self: bool,
) {
    if !skip_self && selectors.iter().any(|s| node_matches(s, node, root)) {
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
