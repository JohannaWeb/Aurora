use super::*;

pub(in crate::js_boa) fn install_identity_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    // id
    let id_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            let v = if let Node::Element(el) = &*b {
                el.attributes.get("id").cloned().unwrap_or_default()
            } else {
                String::new()
            };
            Ok(JsValue::from(JsString::from(v)))
        },
        cap.clone(),
    );
    let id_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let v = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert("id".to_string(), v);
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "id", Some(id_get), Some(id_set));

    // className
    let cn_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            let v = if let Node::Element(el) = &*b {
                el.attributes.get("class").cloned().unwrap_or_default()
            } else {
                String::new()
            };
            Ok(JsValue::from(JsString::from(v)))
        },
        cap.clone(),
    );
    let cn_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let v = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert("class".to_string(), v);
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "className", Some(cn_get), Some(cn_set));
}
