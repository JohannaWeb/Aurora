use super::*;

pub(in crate::js_boa) fn add_document_query_methods(
    init: &mut ObjectInitializer<'_>,
    doc_cap: &DocCapture,
) {
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let id = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                if let Some(node) = find_by_id(&cap.document, &id) {
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                } else {
                    Ok(JsValue::null())
                }
            },
            doc_cap.clone(),
        ),
        js_string!("getElementById"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let tag = js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                let mut acc = Vec::new();
                collect_by_tag(&cap.document, &tag, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            doc_cap.clone(),
        ),
        js_string!("getElementsByTagName"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut acc = Vec::new();
                collect_by_class(&cap.document, &cls, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            doc_cap.clone(),
        ),
        js_string!("getElementsByClassName"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut acc = Vec::new();
                collect_by_attr(&cap.document, "name", &name, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            doc_cap.clone(),
        ),
        js_string!("getElementsByName"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                match query_first(&cap.document, &sel) {
                    Some(node) => Ok(create_js_node(node, &cap.registry, &cap.document, ctx)),
                    None => Ok(JsValue::null()),
                }
            },
            doc_cap.clone(),
        ),
        js_string!("querySelector"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let found = query_all(&cap.document, &sel);
                build_nodelist(found, &cap.registry, &cap.document, ctx)
            },
            doc_cap.clone(),
        ),
        js_string!("querySelectorAll"),
        1,
    );
}
