use super::*;

pub(crate) fn first_child(node: &NodePtr, elements_only: bool) -> Option<NodePtr> {
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    for kid in kids {
        if !elements_only || matches!(&*kid.borrow(), Node::Element(_)) {
            return Some(kid);
        }
    }
    None
}

pub(crate) fn last_child(node: &NodePtr, elements_only: bool) -> Option<NodePtr> {
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    for kid in kids.into_iter().rev() {
        if !elements_only || matches!(&*kid.borrow(), Node::Element(_)) {
            return Some(kid);
        }
    }
    None
}
