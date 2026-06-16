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
                let node = Node::text(text);
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
    )
    .function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let range = ObjectInitializer::new(ctx)
                .property(js_string!("collapsed"), true, Attribute::all())
                .property(js_string!("startOffset"), 0, Attribute::all())
                .property(js_string!("endOffset"), 0, Attribute::all())
                .property(
                    js_string!("startContainer"),
                    JsValue::null(),
                    Attribute::all(),
                )
                .property(
                    js_string!("endContainer"),
                    JsValue::null(),
                    Attribute::all(),
                )
                .function(noop_native(), js_string!("setStart"), 2)
                .function(noop_native(), js_string!("setEnd"), 2)
                .function(noop_native(), js_string!("setStartBefore"), 1)
                .function(noop_native(), js_string!("setEndAfter"), 1)
                .function(noop_native(), js_string!("collapse"), 1)
                .function(noop_native(), js_string!("selectNode"), 1)
                .function(noop_native(), js_string!("selectNodeContents"), 1)
                .function(noop_native(), js_string!("deleteContents"), 0)
                .function(noop_native(), js_string!("detach"), 0)
                .function(noop_native(), js_string!("surroundContents"), 1)
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, ctx| {
                        let r = ObjectInitializer::new(ctx)
                            .property(js_string!("top"), 0, Attribute::all())
                            .property(js_string!("left"), 0, Attribute::all())
                            .property(js_string!("width"), 0, Attribute::all())
                            .property(js_string!("height"), 0, Attribute::all())
                            .property(js_string!("right"), 0, Attribute::all())
                            .property(js_string!("bottom"), 0, Attribute::all())
                            .build();
                        Ok(r.into())
                    }),
                    js_string!("getBoundingClientRect"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
                    js_string!("getClientRects"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
                        Ok(JsValue::from(js_string!("")))
                    }),
                    js_string!("toString"),
                    0,
                )
                .build();
            Ok(range.into())
        }),
        js_string!("createRange"),
        0,
    )
    .function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let walker = ObjectInitializer::new(ctx)
                .property(js_string!("currentNode"), JsValue::null(), Attribute::all())
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("nextNode"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("previousNode"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("parentNode"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("firstChild"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("lastChild"),
                    0,
                )
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                    js_string!("nextSibling"),
                    0,
                )
                .build();
            Ok(walker.into())
        }),
        js_string!("createTreeWalker"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, _cap: &DocCapture, _ctx| Ok(JsValue::null()),
            doc_cap.clone(),
        ),
        js_string!("elementFromPoint"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, _cap: &DocCapture, ctx| Ok(JsArray::new(ctx).into()),
            doc_cap.clone(),
        ),
        js_string!("elementsFromPoint"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, _cap: &DocCapture, _ctx| {
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            doc_cap.clone(),
        ),
        js_string!("adoptNode"),
        1,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, _cap: &DocCapture, _ctx| {
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            doc_cap.clone(),
        ),
        js_string!("importNode"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, _cap: &DocCapture, _ctx| Ok(JsValue::null()),
            doc_cap.clone(),
        ),
        js_string!("caretRangeFromPoint"),
        2,
    )
    .function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, _cap: &DocCapture, _ctx| Ok(JsValue::null()),
            doc_cap.clone(),
        ),
        js_string!("caretPositionFromPoint"),
        2,
    );
}
