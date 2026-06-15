use super::*;
use std::rc::Rc;

fn take_document_fragment_children(node: &NodePtr) -> Option<Vec<NodePtr>> {
    let mut borrow = node.borrow_mut();
    match &mut *borrow {
        Node::Element(el) if el.tag_name == "#document-fragment" => {
            Some(std::mem::take(&mut el.children))
        }
        _ => None,
    }
}

pub(crate) fn collect_text(node: &NodePtr) -> String {
    let b = node.borrow();
    match &*b {
        Node::Text(t) => t.clone(),
        Node::Element(el) => el
            .children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
        Node::Document { children, .. } => children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(crate) fn set_text_content(node: &NodePtr, text: &str) {
    let new_text = Node::text(text.to_string());
    if let Node::Element(el) = &mut *node.borrow_mut() {
        el.children = vec![new_text];
    }
}

pub(crate) fn prepend_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(child) {
        for frag_child in children.into_iter().rev() {
            detach_from_parent(&frag_child);
            prepend_child_ptr(parent, &frag_child);
        }
        return;
    }
    detach_from_parent(child);
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children, .. } => children,
        _ => return,
    };
    kids.insert(0, child.clone());
    drop(p);
    crate::dom::set_parent(child, parent);
}

/// Remove `child` from its current parent's child list, if it has one.
///
/// Insertion is a *move* in the DOM: appending/inserting a node that already
/// lives somewhere first detaches it. Skipping this left the node parented in
/// two places at once (e.g. `fragment.appendChild(div.firstChild)` never emptied
/// the div), which spun YouTube's icon clear-and-rebuild loop forever.
fn detach_from_parent(child: &NodePtr) {
    let Some(parent) = crate::dom::parent_ptr(child) else {
        return;
    };
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children, .. } => children,
        _ => return,
    };
    kids.retain(|c| !Rc::ptr_eq(c, child));
}

pub(crate) fn append_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(child) {
        for frag_child in children {
            append_child_ptr(parent, &frag_child);
        }
        return;
    }
    detach_from_parent(child);
    let mut appended = false;
    if let Node::Element(el) = &mut *parent.borrow_mut() {
        el.children.push(child.clone());
        appended = true;
    } else if let Node::Document { children, .. } = &mut *parent.borrow_mut() {
        children.push(child.clone());
        appended = true;
    }
    if appended {
        crate::dom::set_parent(child, parent);
    }
}

pub(crate) fn insert_before_ptr(
    parent: &NodePtr,
    new_child: &NodePtr,
    ref_child: Option<&NodePtr>,
) {
    if let Some(children) = take_document_fragment_children(new_child) {
        let mut ref_cursor = ref_child.cloned();
        for frag_child in children {
            insert_before_ptr(parent, &frag_child, ref_cursor.as_ref());
            ref_cursor = Some(frag_child);
        }
        return;
    }
    // Detach first (move semantics), then resolve the ref position so indices are
    // correct even when moving a node within its current parent.
    detach_from_parent(new_child);
    {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        match ref_child.and_then(|rc| kids.iter().position(|c| Rc::ptr_eq(c, rc))) {
            Some(pos) => kids.insert(pos, new_child.clone()),
            None => kids.push(new_child.clone()),
        }
    }
    crate::dom::set_parent(new_child, parent);
}

pub(crate) fn remove_child_ptr(parent: &NodePtr, child: &NodePtr) {
    let removed = {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        let before = kids.len();
        kids.retain(|c| !Rc::ptr_eq(c, child));
        kids.len() != before
    };
    if removed {
        crate::dom::clear_parent(child);
    }
}

pub(crate) fn replace_child_ptr(parent: &NodePtr, new_child: &NodePtr, old_child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(new_child) {
        let mut replaced = false;
        {
            let mut p = parent.borrow_mut();
            let kids: &mut Vec<NodePtr> = match &mut *p {
                Node::Element(el) => &mut el.children,
                Node::Document { children, .. } => children,
                _ => return,
            };
            if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
                kids.remove(pos);
                for (idx, frag_child) in children.into_iter().enumerate() {
                    kids.insert(pos + idx, frag_child.clone());
                    crate::dom::set_parent(&frag_child, parent);
                }
                replaced = true;
            }
        }
        if replaced {
            crate::dom::clear_parent(old_child);
        }
        return;
    }
    detach_from_parent(new_child);
    let replaced = {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        match kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
            Some(pos) => {
                kids[pos] = new_child.clone();
                true
            }
            None => false,
        }
    };
    if replaced {
        crate::dom::set_parent(new_child, parent);
        crate::dom::clear_parent(old_child);
    }
}

pub(crate) fn clone_node(node: &NodePtr, deep: bool) -> NodePtr {
    let cloned = {
        let b = node.borrow();
        match &*b {
            Node::Text(t) => Node::text(t.clone()),
            Node::Element(el) => {
                let children = if deep {
                    el.children.iter().map(|c| clone_node(c, true)).collect()
                } else {
                    vec![]
                };
                Node::element_with_attributes(el.tag_name.clone(), el.attributes.clone(), children)
            }
            Node::Document { children, mode } => {
                let children = if deep {
                    children.iter().map(|c| clone_node(c, true)).collect()
                } else {
                    vec![]
                };
                Node::document_with_mode(children, *mode)
            }
        }
    };
    if deep {
        crate::dom::reparent_subtree(&cloned);
    }
    cloned
}

/// Whether `node` is reachable from `document` by walking parent pointers.
///
/// Walks up via `find_parent` (which uses the O(depth) parent back-pointer with
/// a self-healing scan fallback) instead of scanning the whole document subtree
/// downward, which made `isConnected` an O(N)-per-call hot spot during boot.
pub(crate) fn is_connected_to(document: &NodePtr, node: &NodePtr) -> bool {
    let mut current = node.clone();
    // Bounded to guard against a cycle introduced by a stale parent pointer.
    for _ in 0..100_000 {
        if Rc::ptr_eq(&current, document) {
            return true;
        }
        match crate::js_v8::selectors::query::find_parent(document, &current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

pub(crate) fn contains_ptr(parent: &NodePtr, other: &NodePtr) -> bool {
    if Rc::ptr_eq(parent, other) {
        return true;
    }
    // Borrow and recurse by reference; cloning the children `Vec` at every level
    // turned descendant checks into an allocation-heavy hot path. Children are
    // distinct `RefCell`s, so holding `parent`'s borrow across the recursion is
    // safe for an (acyclic) DOM tree.
    let borrow = parent.borrow();
    let kids: &[NodePtr] = match &*borrow {
        Node::Element(el) => &el.children,
        Node::Document { children, .. } => children,
        _ => return false,
    };
    kids.iter().any(|child| contains_ptr(child, other))
}
