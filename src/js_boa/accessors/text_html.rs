use super::*;

pub(in crate::js_boa) fn install_text_html_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    // textContent
    let tc_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(collect_text(&cap.node))))
        },
        cap.clone(),
    );
    let tc_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            set_text_content(&cap.node, &text);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "textContent", Some(tc_get), Some(tc_set));

    // innerText (alias — simplified)
    let it_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(collect_text(&cap.node))))
        },
        cap.clone(),
    );
    let it_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            set_text_content(&cap.node, &text);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "innerText", Some(it_get), Some(it_set));

    // innerHTML: getter returns textual serialization; setter parses as plain text.
    let ih_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(serialize_inner_html(
                &cap.node,
            ))))
        },
        cap.clone(),
    );
    let ih_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            // Minimal: treat as HTML fragment via the existing Parser.
            let parsed = crate::html::Parser::new(&text).parse_document();
            let new_children: Vec<NodePtr> = match &*parsed.borrow() {
                Node::Document { children } => children.clone(),
                _ => Vec::new(),
            };
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.children = new_children;
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "innerHTML", Some(ih_get), Some(ih_set));

    // outerHTML — readonly-ish.
    let oh_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(serialize_outer_html(
                &cap.node,
            ))))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "outerHTML", Some(oh_get), None);
}
