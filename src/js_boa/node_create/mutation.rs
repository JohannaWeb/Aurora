use super::*;

pub(in crate::js_boa) fn install_mutation_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let Some(child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) else {
                    return Ok(args.get(0).cloned().unwrap_or(JsValue::null()));
                };
                append_child_ptr(&cap.node, &child);
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("appendChild"),
        1,
    );

    // insertBefore(newChild, refChild)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let Some(new_child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) else {
                    return Ok(JsValue::null());
                };
                let ref_child = args
                    .get(1)
                    .and_then(|v| node_from_js(v, &cap.registry, ctx));
                insert_before_ptr(&cap.node, &new_child, ref_child.as_ref());
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("insertBefore"),
        2,
    );

    // removeChild(child)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    remove_child_ptr(&cap.node, &child);
                }
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("removeChild"),
        1,
    );

    // replaceChild(newChild, oldChild)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let new_child = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                );
                let old_child = node_from_js(
                    args.get(1).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                );
                if let (Some(new_c), Some(old_c)) = (new_child, old_child) {
                    replace_child_ptr(&cap.node, &new_c, &old_c);
                }
                Ok(args.get(1).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("replaceChild"),
        2,
    );

    // remove() — detach from parent.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, _ctx| {
                if let Some(parent) = find_parent(&cap.document, &cap.node) {
                    remove_child_ptr(&parent, &cap.node);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("remove"),
        0,
    );

    // cloneNode(deep)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let deep = args.get(0).map(|v| v.to_boolean()).unwrap_or(false);
                let cloned = clone_node(&cap.node, deep);
                Ok(create_js_node(cloned, &cap.registry, &cap.document, ctx))
            },
            cap.clone(),
        ),
        js_string!("cloneNode"),
        1,
    );

    // contains(other)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(other) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    Ok(JsValue::from(contains_ptr(&cap.node, &other)))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("contains"),
        1,
    );
}
