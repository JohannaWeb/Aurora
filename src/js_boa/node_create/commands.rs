use super::*;

pub(in crate::js_boa) fn install_command_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    // getBoundingClientRect
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("x"), 0, Attribute::all())
                .property(js_string!("y"), 0, Attribute::all())
                .property(js_string!("top"), 0, Attribute::all())
                .property(js_string!("right"), 0, Attribute::all())
                .property(js_string!("bottom"), 0, Attribute::all())
                .property(js_string!("left"), 0, Attribute::all())
                .property(js_string!("width"), 0, Attribute::all())
                .property(js_string!("height"), 0, Attribute::all())
                .build();
            Ok(obj.into())
        }),
        js_string!("getBoundingClientRect"),
        0,
    );
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
        js_string!("getClientRects"),
        0,
    );

    // insertAdjacentHTML / insertAdjacentElement / insertAdjacentText — stubs that append.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentHTML"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentText"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(el) = node_from_js(
                    args.get(1).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    append_child_ptr(&cap.node, &el);
                }
                Ok(args.get(1).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentElement"),
        2,
    );

    // Event listeners / dispatch — stubs.
    init.function(noop_native(), js_string!("addEventListener"), 3);
    init.function(noop_native(), js_string!("removeEventListener"), 3);
    init.function(return_bool(true), js_string!("dispatchEvent"), 1);

    // focus / blur / click — stubs.
    init.function(noop_native(), js_string!("focus"), 0);
    init.function(noop_native(), js_string!("blur"), 0);
    init.function(noop_native(), js_string!("click"), 0);
    init.function(noop_native(), js_string!("scrollIntoView"), 0);
    init.function(noop_native(), js_string!("scrollTo"), 0);
    init.function(noop_native(), js_string!("scrollBy"), 0);

    // normalize — no-op.
    init.function(noop_native(), js_string!("normalize"), 0);

    // Read/write property hacks for JS code that assigns .textContent = "x".
    // Because we can't easily install accessors here without more plumbing,
}
