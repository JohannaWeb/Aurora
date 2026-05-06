use super::*;

pub(in crate::js_boa) fn install_query_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    // querySelector / querySelectorAll
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                match query_first(&cap.node, &sel) {
                    Some(n) => Ok(create_js_node(n, &cap.registry, &cap.document, ctx)),
                    None => Ok(JsValue::null()),
                }
            },
            cap.clone(),
        ),
        js_string!("querySelector"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let found = query_all(&cap.node, &sel);
                build_nodelist(found, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("querySelectorAll"),
        1,
    );

    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let tag = js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                let mut acc = Vec::new();
                collect_by_tag(&cap.node, &tag, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("getElementsByTagName"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut acc = Vec::new();
                collect_by_class(&cap.node, &cls, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("getElementsByClassName"),
        1,
    );

    // matches(selector) — best-effort.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                Ok(JsValue::from(selector_matches(&cap.node, &sel)))
            },
            cap.clone(),
        ),
        js_string!("matches"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut current = Some(cap.node.clone());
                while let Some(n) = current {
                    if selector_matches(&n, &sel) {
                        return Ok(create_js_node(n, &cap.registry, &cap.document, ctx));
                    }
                    current = find_parent(&cap.document, &n);
                }
                Ok(JsValue::null())
            },
            cap.clone(),
        ),
        js_string!("closest"),
        1,
    );
}
