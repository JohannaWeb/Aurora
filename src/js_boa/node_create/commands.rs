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
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, ctx: &mut Context| {
                let event_type = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let Some(callback) = args.get(1).and_then(|v| v.as_object()) else {
                    return Ok(JsValue::undefined());
                };
                let id = {
                    let obj = _this.as_object().ok_or_else(|| {
                        JsNativeError::typ()
                            .with_message("removeEventListener receiver must be an object")
                    })?;
                    obj.get(js_string!("__node_id"), ctx)?.to_number(ctx)? as u32
                };
                cap.registry
                    .remove_event_listener(id, &event_type, &callback);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("removeEventListener"),
        3,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, args: &[JsValue], cap: &NodeCapture, ctx: &mut Context| {
                let event = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let event_obj = event.as_object().ok_or_else(|| {
                    JsNativeError::typ().with_message("dispatchEvent expects an Event object")
                })?;
                let id = {
                    let obj = _this.as_object().ok_or_else(|| {
                        JsNativeError::typ()
                            .with_message("dispatchEvent receiver must be an object")
                    })?;
                    obj.get(js_string!("__node_id"), ctx)?.to_number(ctx)? as u32
                };
                let event_type = event_obj
                    .get(js_string!("type"), ctx)?
                    .as_string()
                    .map(|s| s.to_std_string().unwrap_or_default())
                    .unwrap_or_default();
                let _ = event_obj.set(js_string!("target"), _this.clone(), false, ctx);
                let _ = event_obj.set(js_string!("currentTarget"), _this.clone(), false, ctx);
                for listener in cap.registry.get_listeners(id, &event_type) {
                    listener.call(_this, &[event.clone()], ctx)?;
                }
                let default_prevented = event_obj
                    .get(js_string!("defaultPrevented"), ctx)?
                    .to_boolean();
                Ok(JsValue::from(!default_prevented))
            },
            cap.clone(),
        ),
        js_string!("dispatchEvent"),
        1,
    );

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
