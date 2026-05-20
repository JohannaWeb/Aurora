use crate::css::{CascadeElement, ElementData};
use crate::css::selectors_impl::{element_matches, parse_selector_list, AuroraSelectorImpl};
use selectors::parser::Selector;

use super::*;

// ─── Public API ───────────────────────────────────────────────────────────────

pub(in crate::js_boa) fn selector_matches(
    node: &NodePtr,
    selector: &str,
    root: &NodePtr,
) -> bool {
    let selectors = parse_selectors(selector);
    node_matches_any(&selectors, node, root)
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

/// Build the ancestor ElementData chain outermost→immediate parent.
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

/// Collect ElementData for all element siblings at the same level.
fn build_sibling_list(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let Some(parent) = find_parent(root, target) else { return vec![] };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return vec![],
    };
    children
        .iter()
        .filter_map(|child| {
            if let Node::Element(el) = &*child.borrow() {
                Some(ElementData { tag_name: el.tag_name.clone(), attributes: el.attributes.clone() })
            } else {
                None
            }
        })
        .collect()
}

/// 0-based index of `target` among its element siblings.
fn sibling_index_of(root: &NodePtr, target: &NodePtr) -> usize {
    let Some(parent) = find_parent(root, target) else { return 0 };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return 0,
    };
    let mut idx = 0;
    for child in &children {
        if std::rc::Rc::ptr_eq(child, target) {
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
