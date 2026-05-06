use super::*;

pub(super) fn install_element_reflection_properties(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    let tag_name = {
        let b = cap.node.borrow();
        match &*b {
            Node::Element(el) => el.tag_name.clone(),
            _ => return,
        }
    };

    for attr in [
        "type", "name", "value", "href", "src", "rel", "target", "alt",
    ] {
        install_attribute_reflector(obj, cap, context, attr, attr, "");
    }

    if tag_name == "input" {
        install_bool_attribute_reflector(obj, cap, context, "checked", "checked");
        install_bool_attribute_reflector(obj, cap, context, "disabled", "disabled");
        install_bool_attribute_reflector(obj, cap, context, "selected", "selected");
    }
}

pub(super) fn install_attribute_reflector(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
    property: &str,
    attribute: &str,
    fallback: &'static str,
) {
    #[derive(Clone)]
    struct AttrCap {
        node: NodePtr,
        attribute: String,
        fallback: &'static str,
    }
    unsafe impl Trace for AttrCap {
        empty_trace!();
    }
    impl Finalize for AttrCap {}

    let attr_cap = AttrCap {
        node: cap.node.clone(),
        attribute: attribute.to_string(),
        fallback,
    };
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &AttrCap, _ctx| {
            let b = cap.node.borrow();
            let value = match &*b {
                Node::Element(el) => el
                    .attributes
                    .get(&cap.attribute)
                    .cloned()
                    .unwrap_or_else(|| cap.fallback.to_string()),
                _ => cap.fallback.to_string(),
            };
            Ok(JsValue::from(JsString::from(value)))
        },
        attr_cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &AttrCap, _ctx| {
            let value = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert(cap.attribute.clone(), value);
            }
            Ok(JsValue::undefined())
        },
        attr_cap,
    );
    install_accessor(obj, context, property, Some(getter), Some(setter));
}

pub(super) fn install_bool_attribute_reflector(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
    property: &str,
    attribute: &str,
) {
    #[derive(Clone)]
    struct BoolAttrCap {
        node: NodePtr,
        attribute: String,
    }
    unsafe impl Trace for BoolAttrCap {
        empty_trace!();
    }
    impl Finalize for BoolAttrCap {}

    let attr_cap = BoolAttrCap {
        node: cap.node.clone(),
        attribute: attribute.to_string(),
    };
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &BoolAttrCap, _ctx| {
            let b = cap.node.borrow();
            let present = match &*b {
                Node::Element(el) => el.attributes.contains_key(&cap.attribute),
                _ => false,
            };
            Ok(JsValue::from(present))
        },
        attr_cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &BoolAttrCap, _ctx| {
            let enabled = args.get(0).map(|v| v.to_boolean()).unwrap_or(false);
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                if enabled {
                    el.attributes
                        .insert(cap.attribute.clone(), cap.attribute.clone());
                } else {
                    el.attributes.remove(&cap.attribute);
                }
            }
            Ok(JsValue::undefined())
        },
        attr_cap,
    );
    install_accessor(obj, context, property, Some(getter), Some(setter));
}
