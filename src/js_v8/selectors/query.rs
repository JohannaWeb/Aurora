use crate::css::ElementData;
use crate::css::selectors_impl::{AuroraSelectorImpl, element_matches, parse_selector_list};
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
    // Seed the ancestor path once (outermost→parent of `start_node`); the
    // traversal threads it downward instead of re-deriving each node's
    // ancestors via a full-tree `find_parent` walk.
    let mut ancestors = build_ancestor_chain(root, start_node);
    let mut found = None;
    collect_matches(start_node, &selectors, &mut ancestors, &[], 0, true, &mut |n| {
        found = Some(n.clone());
        true
    });
    found
}

pub(crate) fn query_all(root: &NodePtr, selector: &str, start_node: &NodePtr) -> Vec<NodePtr> {
    let selectors = parse_selectors(selector);
    let mut ancestors = build_ancestor_chain(root, start_node);
    let mut out = Vec::new();
    collect_matches(start_node, &selectors, &mut ancestors, &[], 0, true, &mut |n| {
        out.push(n.clone());
        false
    });
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
    // The parent back-pointer is authoritative: the runtime links the initial
    // tree and the mutation primitives maintain it, so `None` means `target` has
    // no parent (e.g. a freshly created, not-yet-attached node) and we must NOT
    // fall back to an O(N) document scan — that scan, run per `isConnected`
    // check on detached nodes during boot, was the quadratic hot spot.
    let Some(parent) = crate::dom::parent_ptr(target) else {
        return None;
    };
    if is_direct_child(&parent, target) {
        return Some(parent);
    }
    // Safety net: the pointer is set but no longer lists `target` (a move that
    // missed a maintenance site). Scan once and repair so we self-correct.
    let found = find_parent_scan(root, target);
    match &found {
        Some(parent) => crate::dom::set_parent(target, parent),
        None => crate::dom::clear_parent(target),
    }
    found
}

fn is_direct_child(parent: &NodePtr, target: &NodePtr) -> bool {
    let borrow = parent.borrow();
    let kids: &[NodePtr] = match &*borrow {
        Node::Document { children, .. } => children,
        Node::Element(el) => &el.children,
        _ => return false,
    };
    kids.iter().any(|child| Rc::ptr_eq(child, target))
}

fn find_parent_scan(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
    let kids: Vec<NodePtr> = match &*root.borrow() {
        Node::Document { children, .. } => children.clone(),
        Node::Element(el) => el.children.clone(),
        _ => return None,
    };
    for child in &kids {
        if Rc::ptr_eq(child, target) {
            return Some(root.clone());
        }
        if let Some(found) = find_parent_scan(child, target) {
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

fn element_data_of(node: &NodePtr) -> Option<ElementData> {
    match &*node.borrow() {
        Node::Element(el) => Some(ElementData {
            tag_name: el.tag_name.clone(),
            attributes: el.attributes.clone(),
        }),
        _ => None,
    }
}

/// Depth-first traversal that matches every element against `selectors` while
/// threading the matching context down the tree.
///
/// `ancestors` is the live outermost→parent path (pushed on descent, popped on
/// return); `siblings`/`sibling_index` describe `node`'s position among its
/// element-siblings. Computing these on the way down makes the whole query
/// linear in the node count, instead of re-deriving each node's ancestors and
/// siblings with repeated full-tree `find_parent` scans.
///
/// `on_match` is invoked for each matching element; returning `true` stops the
/// traversal early (used by `query_first`). Returns `true` if it stopped.
fn collect_matches(
    node: &NodePtr,
    selectors: &[Selector<AuroraSelectorImpl>],
    ancestors: &mut Vec<ElementData>,
    siblings: &[ElementData],
    sibling_index: usize,
    skip_self: bool,
    on_match: &mut dyn FnMut(&NodePtr) -> bool,
) -> bool {
    if !skip_self {
        if let Some(data) = element_data_of(node) {
            let matched = selectors
                .iter()
                .any(|sel| element_matches(sel, &data, ancestors, siblings, sibling_index));
            if matched && on_match(node) {
                return true;
            }
        }
    }

    let children: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return false,
    };
    let child_siblings: Vec<ElementData> = children.iter().filter_map(element_data_of).collect();

    let pushed = element_data_of(node).map(|data| ancestors.push(data)).is_some();
    let mut stopped = false;
    let mut element_index = 0usize;
    for child in &children {
        let is_element = matches!(&*child.borrow(), Node::Element(_));
        let index = if is_element { element_index } else { 0 };
        if collect_matches(child, selectors, ancestors, &child_siblings, index, false, on_match) {
            stopped = true;
            break;
        }
        if is_element {
            element_index += 1;
        }
    }
    if pushed {
        ancestors.pop();
    }
    stopped
}
