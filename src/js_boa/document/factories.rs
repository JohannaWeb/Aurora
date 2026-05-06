use super::*;

pub(in crate::js_boa) fn add_document_factory_methods(
    init: &mut ObjectInitializer<'_>,
    doc_cap: &DocCapture,
) {
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let tag = js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                let node = Node::element(tag, vec![]);
                Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
            },
            doc_cap.clone(),
        ),
        js_string!("createElement"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                // Namespaced element creation — ignore the namespace.
                let tag = js_string_of(args.get(1).unwrap_or(&JsValue::undefined())).to_lowercase();
                let node = Node::element(tag, vec![]);
                Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
            },
            doc_cap.clone(),
        ),
        js_string!("createElementNS"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let node = Node::text(text);
                Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
            },
            doc_cap.clone(),
        ),
        js_string!("createTextNode"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &DocCapture, ctx| {
                let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let node = Node::Text(text);
                let node = Rc::new(RefCell::new(node));
                Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
            },
            doc_cap.clone(),
        ),
        js_string!("createComment"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &DocCapture, ctx| {
                let node = Node::element("#document-fragment", vec![]);
                Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
            },
            doc_cap.clone(),
        ),
        js_string!("createDocumentFragment"),
        0,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, _cap: &DocCapture, ctx| {
                let obj = ObjectInitializer::new(ctx)
                    .property(js_string!("type"), js_string!(""), Attribute::all())
                    .function(noop_native(), js_string!("preventDefault"), 0)
                    .function(noop_native(), js_string!("stopPropagation"), 0)
                    .function(noop_native(), js_string!("initEvent"), 3)
                    .build();
                Ok(obj.into())
            },
            doc_cap.clone(),
        ),
        js_string!("createEvent"),
        1,
    );
}
