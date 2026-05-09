use super::*;

pub(in crate::js_boa) fn install_command_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
        js_string!("getClientRects"),
        0,
    );

    // insertAdjacentHTML / insertAdjacentElement / insertAdjacentText — stubs that append.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, _ctx: &mut Context| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                cap.registry.mark_layout_dirty(&cap.node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentHTML"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, _ctx: &mut Context| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                cap.registry.mark_layout_dirty(&cap.node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentText"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, ctx: &mut Context| {
                if let Some(el) = node_from_js(
                    args.get(1).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    append_child_ptr(&cap.node, &el);
                    cap.registry.mark_layout_dirty(&cap.node);
                }
                Ok(args.get(1).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentElement"),
        2,
    );

    // Event listeners / dispatch — stubs.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, ctx: &mut Context| {
                let event_type = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let callback = args.get(1).and_then(|v| v.as_object());
                if let Some(callback) = callback {
                    let id = {
                        let obj = _this.as_object().expect("this must be an object");
                        obj.get(js_string!("__node_id"), ctx)
                            .expect("must have __node_id")
                            .to_number(ctx)
                            .expect("__node_id must be a number") as u32
                    };
                    cap.registry
                        .add_event_listener(id, event_type, callback.clone());
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("addEventListener"),
        2,
    );
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
