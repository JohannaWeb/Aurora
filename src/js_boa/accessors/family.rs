use super::*;

pub(in crate::js_boa) fn install_family_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    // parentNode / parentElement
    let p_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match find_parent(&cap.document, &cap.node) {
            Some(p) => Ok(create_js_node(p, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "parentNode", Some(p_get), None);
    let p_get2 = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match find_parent(&cap.document, &cap.node) {
            Some(p) => {
                let is_elem = matches!(&*p.borrow(), Node::Element(_));
                if is_elem {
                    Ok(create_js_node(p, &cap.registry, &cap.document, ctx))
                } else {
                    Ok(JsValue::null())
                }
            }
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "parentElement", Some(p_get2), None);

    // children (elements only), childNodes (all)
    let ch_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kids: Vec<NodePtr> = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el
                        .children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .cloned()
                        .collect(),
                    Node::Document { children } => children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .cloned()
                        .collect(),
                    _ => Vec::new(),
                }
            };
            build_nodelist(kids, &cap.registry, &cap.document, ctx)
        },
        cap.clone(),
    );
    install_accessor(obj, context, "children", Some(ch_get), None);

    let cn2_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kids: Vec<NodePtr> = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el.children.clone(),
                    Node::Document { children } => children.clone(),
                    _ => Vec::new(),
                }
            };
            build_nodelist(kids, &cap.registry, &cap.document, ctx)
        },
        cap.clone(),
    );
    install_accessor(obj, context, "childNodes", Some(cn2_get), None);

    // firstChild / lastChild / firstElementChild / lastElementChild
    let fc = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kid = first_child(&cap.node);
            match kid {
                Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );
    install_accessor(obj, context, "firstChild", Some(fc), None);

    let lc = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match last_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "lastChild", Some(lc), None);

    let fec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match first_element_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "firstElementChild", Some(fec), None);

    let lec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match last_element_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "lastElementChild", Some(lec), None);

    // nextSibling / previousSibling / nextElementSibling / previousElementSibling
    let ns = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, 1, false) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "nextSibling", Some(ns), None);

    let ps = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, -1, false) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "previousSibling", Some(ps), None);

    let nes = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, 1, true) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "nextElementSibling", Some(nes), None);

    let pes = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, -1, true) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "previousElementSibling", Some(pes), None);

    // childElementCount
    let cec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let count = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el
                        .children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .count(),
                    Node::Document { children } => children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .count(),
                    _ => 0,
                }
            };
            Ok(JsValue::from(count as u32))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "childElementCount", Some(cec), None);
}
