use crate::dom::{Node, NodePtr};

pub(super) fn collect_styles(
    node_ptr: &NodePtr,
    base_url: Option<&str>,
    identity: &opus::domain::Identity,
    output: &mut String,
) {
    let node = node_ptr.borrow();
    match &*node {
        Node::Document { children } => {
            for child in children {
                collect_styles(child, base_url, identity, output);
            }
        }
        Node::Element(element) => {
            collect_element_styles(element, base_url, identity, output);
            for child in &element.children {
                collect_styles(child, base_url, identity, output);
            }
        }
        Node::Text(_) => {}
    }
}

fn collect_element_styles(
    element: &crate::dom::ElementNode,
    base_url: Option<&str>,
    identity: &opus::domain::Identity,
    output: &mut String,
) {
    if element.tag_name == "style" {
        for child in &element.children {
            if let Node::Text(text) = &*child.borrow() {
                output.push_str(text);
                output.push('\n');
            }
        }
    } else if element.tag_name == "link"
        && element.attributes.get("rel").map(String::as_str) == Some("stylesheet")
    {
        collect_link_styles(element, base_url, identity, output);
    }
}

fn collect_link_styles(
    element: &crate::dom::ElementNode,
    base_url: Option<&str>,
    identity: &opus::domain::Identity,
    output: &mut String,
) {
    let (Some(base), Some(href)) = (base_url, element.attributes.get("href")) else {
        return;
    };
    if let Ok(url) = crate::fetch::resolve_relative_url(base, href) {
        if let Ok(css) = crate::fetch::fetch_string(&url, identity) {
            output.push_str(&css);
            output.push('\n');
        }
    }
}
