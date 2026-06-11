use super::*;
use std::rc::Rc;

pub(crate) fn sibling(root: &NodePtr, node: &NodePtr, delta: i32, elements_only: bool) -> Option<NodePtr> {
    let parent = find_parent(root, node)?;
    let kids: Vec<NodePtr> = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    let pos = kids.iter().position(|c| Rc::ptr_eq(c, node))?;
    
    let mut current = pos as i32;
    let step = if delta > 0 { 1 } else { -1 };
    let mut remaining = delta.abs();
    
    while remaining > 0 {
        current += step;
        if current < 0 || current >= kids.len() as i32 {
            return None;
        }
        let cand = &kids[current as usize];
        if !elements_only || matches!(&*cand.borrow(), Node::Element(_)) {
            remaining -= 1;
            if remaining == 0 {
                return Some(cand.clone());
            }
        }
    }
    None
}

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

fn find_parent(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
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
