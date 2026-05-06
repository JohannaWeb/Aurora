use super::*;

pub(in crate::js_boa) fn collect_text(node: &NodePtr) -> String {
    let b = node.borrow();
    match &*b {
        Node::Text(t) => t.clone(),
        Node::Element(el) => el
            .children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
        Node::Document { children } => children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(in crate::js_boa) fn set_text_content(node: &NodePtr, text: &str) {
    let new_text = Node::text(text.to_string());
    if let Node::Element(el) = &mut *node.borrow_mut() {
        el.children = vec![new_text];
    }
}

pub(in crate::js_boa) fn append_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Node::Element(el) = &mut *parent.borrow_mut() {
        el.children.push(child.clone());
    } else if let Node::Document { children } = &mut *parent.borrow_mut() {
        children.push(child.clone());
    }
}

pub(in crate::js_boa) fn insert_before_ptr(
    parent: &NodePtr,
    new_child: &NodePtr,
    ref_child: Option<&NodePtr>,
) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    if let Some(rc) = ref_child {
        if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, rc)) {
            kids.insert(pos, new_child.clone());
            return;
        }
    }
    kids.push(new_child.clone());
}

pub(in crate::js_boa) fn remove_child_ptr(parent: &NodePtr, child: &NodePtr) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    kids.retain(|c| !Rc::ptr_eq(c, child));
}

pub(in crate::js_boa) fn replace_child_ptr(
    parent: &NodePtr,
    new_child: &NodePtr,
    old_child: &NodePtr,
) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
        kids[pos] = new_child.clone();
    }
}
