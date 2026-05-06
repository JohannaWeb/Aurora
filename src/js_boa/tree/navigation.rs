use super::*;

pub(in crate::js_boa) fn first_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.first().cloned(),
        Node::Document { children } => children.first().cloned(),
        _ => None,
    }
}
pub(in crate::js_boa) fn last_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.last().cloned(),
        Node::Document { children } => children.last().cloned(),
        _ => None,
    }
}
pub(in crate::js_boa) fn first_element_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el
            .children
            .iter()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        Node::Document { children } => children
            .iter()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        _ => None,
    }
}
pub(in crate::js_boa) fn last_element_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el
            .children
            .iter()
            .rev()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        Node::Document { children } => children
            .iter()
            .rev()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        _ => None,
    }
}

pub(in crate::js_boa) fn find_parent(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
    let b = root.borrow();
    match &*b {
        Node::Element(el) => {
            for c in &el.children {
                if Rc::ptr_eq(c, target) {
                    drop(b);
                    return Some(root.clone());
                }
                if let Some(p) = find_parent(c, target) {
                    return Some(p);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if Rc::ptr_eq(c, target) {
                    drop(b);
                    return Some(root.clone());
                }
                if let Some(p) = find_parent(c, target) {
                    return Some(p);
                }
            }
            None
        }
        _ => None,
    }
}

pub(in crate::js_boa) fn sibling(
    root: &NodePtr,
    target: &NodePtr,
    delta: i32,
    element_only: bool,
) -> Option<NodePtr> {
    let parent = find_parent(root, target)?;
    let b = parent.borrow();
    let kids: &Vec<NodePtr> = match &*b {
        Node::Element(el) => &el.children,
        Node::Document { children } => children,
        _ => return None,
    };
    let idx = kids.iter().position(|c| Rc::ptr_eq(c, target))?;
    let mut i = idx as i32 + delta;
    while i >= 0 && (i as usize) < kids.len() {
        let c = &kids[i as usize];
        if !element_only || matches!(&*c.borrow(), Node::Element(_)) {
            return Some(c.clone());
        }
        i += delta;
    }
    None
}

pub(in crate::js_boa) fn contains_ptr(root: &NodePtr, needle: &NodePtr) -> bool {
    if Rc::ptr_eq(root, needle) {
        return true;
    }
    match &*root.borrow() {
        Node::Element(el) => el.children.iter().any(|c| contains_ptr(c, needle)),
        Node::Document { children } => children.iter().any(|c| contains_ptr(c, needle)),
        _ => false,
    }
}

pub(in crate::js_boa) fn clone_node(node: &NodePtr, deep: bool) -> NodePtr {
    match &*node.borrow() {
        Node::Text(t) => Node::text(t.clone()),
        Node::Document { children } => {
            let kids = if deep {
                children.iter().map(|c| clone_node(c, true)).collect()
            } else {
                Vec::new()
            };
            Node::document(kids)
        }
        Node::Element(el) => {
            let kids = if deep {
                el.children.iter().map(|c| clone_node(c, true)).collect()
            } else {
                Vec::new()
            };
            Node::element_with_attributes(el.tag_name.clone(), el.attributes.clone(), kids)
        }
    }
}
