use super::*;

pub(in crate::js_boa) fn selector_matches(node: &NodePtr, selector: &str) -> bool {
    // Only checks the final compound in each comma-group against `node` directly.
    for group in selector.split(',') {
        let parts: Vec<&str> = group.split_whitespace().collect();
        if let Some(last) = parts.last() {
            if let Some(sel) = parse_simple(last) {
                if simple_matches(node, &sel) {
                    return true;
                }
            }
        }
    }
    false
}

pub(in crate::js_boa) fn query_first(root: &NodePtr, selector: &str) -> Option<NodePtr> {
    let groups = parse_selector_groups(selector);
    query_first_rec(root, &groups, 0, true)
}

pub(in crate::js_boa) fn query_all(root: &NodePtr, selector: &str) -> Vec<NodePtr> {
    let groups = parse_selector_groups(selector);
    let mut out = Vec::new();
    query_all_rec(root, &groups, &mut out, true);
    out
}

pub(in crate::js_boa) fn parse_selector_groups(selector: &str) -> Vec<Vec<SimpleSel>> {
    selector
        .split(',')
        .filter_map(|g| {
            let parts: Vec<SimpleSel> = g.split_whitespace().filter_map(parse_simple).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts)
            }
        })
        .collect()
}

pub(in crate::js_boa) fn matches_any_group(
    node: &NodePtr,
    groups: &[Vec<SimpleSel>],
    root: &NodePtr,
) -> bool {
    for g in groups {
        if matches_group(node, g, root) {
            return true;
        }
    }
    false
}

pub(in crate::js_boa) fn matches_group(
    node: &NodePtr,
    group: &[SimpleSel],
    root: &NodePtr,
) -> bool {
    if group.is_empty() {
        return false;
    }
    let last = group.last().unwrap();
    if !simple_matches(node, last) {
        return false;
    }
    // Walk ancestors to verify descendant chain.
    let mut idx = group.len() as i32 - 2;
    let mut cursor = find_parent(root, node);
    while idx >= 0 {
        let sel = &group[idx as usize];
        let mut matched = None;
        while let Some(n) = cursor.clone() {
            if simple_matches(&n, sel) {
                matched = Some(n);
                break;
            }
            cursor = find_parent(root, &n);
        }
        match matched {
            Some(m) => {
                cursor = find_parent(root, &m);
                idx -= 1;
            }
            None => return false,
        }
    }
    true
}

pub(in crate::js_boa) fn query_first_rec(
    node: &NodePtr,
    groups: &[Vec<SimpleSel>],
    _depth: usize,
    skip_self: bool,
) -> Option<NodePtr> {
    if !skip_self {
        if matches_any_group(node, groups, node) {
            return Some(node.clone());
        }
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children } => children.clone(),
        _ => Vec::new(),
    };
    for c in kids {
        if matches_any_group(&c, groups, node) {
            return Some(c);
        }
        if let Some(found) = query_first_rec(&c, groups, _depth + 1, true) {
            return Some(found);
        }
    }
    None
}

pub(in crate::js_boa) fn query_all_rec(
    node: &NodePtr,
    groups: &[Vec<SimpleSel>],
    out: &mut Vec<NodePtr>,
    skip_self: bool,
) {
    if !skip_self && matches_any_group(node, groups, node) {
        out.push(node.clone());
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children } => children.clone(),
        _ => Vec::new(),
    };
    for c in kids {
        if matches_any_group(&c, groups, node) {
            out.push(c.clone());
        }
        query_all_rec(&c, groups, out, true);
    }
}
