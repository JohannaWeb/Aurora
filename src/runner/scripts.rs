pub(crate) fn extract_scripts(node: &crate::dom::NodePtr) -> Vec<(String, bool)> {
    let mut scripts = Vec::new();
    walk(node, &mut scripts);
    scripts
}

fn walk(node: &crate::dom::NodePtr, scripts: &mut Vec<(String, bool)>) {
    let node_borrow = node.borrow();

    match &*node_borrow {
        crate::dom::Node::Element(el) if el.tag_name == "script" => {
            collect_script(el, scripts);
        }
        crate::dom::Node::Element(el) => {
            for child in &el.children {
                walk(child, scripts);
            }
        }
        crate::dom::Node::Document { children } => {
            for child in children {
                walk(child, scripts);
            }
        }
        crate::dom::Node::Text(_) => {}
    }
}

fn collect_script(el: &crate::dom::ElementNode, scripts: &mut Vec<(String, bool)>) {
    if let Some(src) = el.attributes.get("src") {
        scripts.push((src.clone(), true));
        return;
    }

    let mut content = String::new();
    for child in &el.children {
        let child_borrow = child.borrow();
        if let crate::dom::Node::Text(t) = &*child_borrow {
            content.push_str(t);
        }
    }

    if !content.is_empty() {
        scripts.push((content, false));
    }
}
