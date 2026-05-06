use super::*;

pub(in crate::js_boa) fn install_attribute_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    // setAttribute(name, value)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let value = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.insert(name, value);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("setAttribute"),
        2,
    );

    // getAttribute(name)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    match el.attributes.get(&name) {
                        Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                        None => Ok(JsValue::null()),
                    }
                } else {
                    Ok(JsValue::null())
                }
            },
            cap.clone(),
        ),
        js_string!("getAttribute"),
        1,
    );

    // removeAttribute
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.remove(&name);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("removeAttribute"),
        1,
    );

    // hasAttribute
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    Ok(JsValue::from(el.attributes.contains_key(&name)))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("hasAttribute"),
        1,
    );

    // hasAttributes
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, _ctx| {
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    Ok(JsValue::from(!el.attributes.is_empty()))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("hasAttributes"),
        0,
    );

    // getAttributeNames → array of strings
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, ctx| {
                let names: Vec<JsValue> = {
                    let b = cap.node.borrow();
                    if let Node::Element(el) = &*b {
                        el.attributes
                            .keys()
                            .map(|k| JsValue::from(JsString::from(k.clone())))
                            .collect()
                    } else {
                        Vec::new()
                    }
                };
                Ok(JsArray::from_iter(names, ctx).into())
            },
            cap.clone(),
        ),
        js_string!("getAttributeNames"),
        0,
    );
}
