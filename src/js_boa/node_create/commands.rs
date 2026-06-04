use super::*;

pub(in crate::js_boa) fn install_command_methods(
    init: &mut ObjectInitializer<'_>,
    cap: &NodeCapture,
) {
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
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

    // Modern DOM: prepend / append / after / before / replaceWith / replaceChildren.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                for arg in args {
                    if let Some(child) = node_from_js(arg, &cap.registry, ctx) {
                        prepend_child_ptr(&cap.node, &child);
                    } else {
                        prepend_child_ptr(&cap.node, &Node::text(js_string_of(arg)));
                    }
                }
                cap.registry.mark_layout_dirty(&cap.node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("prepend"),
        0,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                for arg in args {
                    if let Some(child) = node_from_js(arg, &cap.registry, ctx) {
                        append_child_ptr(&cap.node, &child);
                    } else {
                        append_child_ptr(&cap.node, &Node::text(js_string_of(arg)));
                    }
                }
                cap.registry.mark_layout_dirty(&cap.node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("append"),
        0,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, _ctx| {
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.children.clear();
                }
                cap.registry.mark_layout_dirty(&cap.node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("replaceChildren"),
        0,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                // insert all args after this node in its parent, then remove this node
                if let Some(parent) = find_parent(&cap.document, &cap.node) {
                    for arg in args.iter().rev() {
                        if let Some(new_node) = node_from_js(arg, &cap.registry, ctx) {
                            insert_before_ptr(&parent, &new_node, Some(&cap.node));
                        }
                    }
                    remove_child_ptr(&parent, &cap.node);
                    cap.registry.mark_layout_dirty(&parent);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("replaceWith"),
        0,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(parent) = find_parent(&cap.document, &cap.node) {
                    for arg in args.iter().rev() {
                        if let Some(new_node) = node_from_js(arg, &cap.registry, ctx) {
                            insert_before_ptr(&parent, &new_node, Some(&cap.node));
                        }
                    }
                    cap.registry.mark_layout_dirty(&parent);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("before"),
        0,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(parent) = find_parent(&cap.document, &cap.node) {
                    let next = sibling(&cap.document, &cap.node, 1, false);
                    for arg in args {
                        if let Some(new_node) = node_from_js(arg, &cap.registry, ctx) {
                            insert_before_ptr(&parent, &new_node, next.as_ref());
                        }
                    }
                    cap.registry.mark_layout_dirty(&parent);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("after"),
        0,
    );

    // attachShadow — stub that returns a document-fragment-like object.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, ctx| {
                let frag = Node::element("#document-fragment", vec![]);
                Ok(create_js_node(frag, &cap.registry, &cap.document, ctx))
            },
            cap.clone(),
        ),
        js_string!("attachShadow"),
        1,
    );

    // toggleAttribute(name, force)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let force = args.get(1).map(|v| v.to_boolean());
                let result = if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    let has = el.attributes.contains_key(&name);
                    let add = force.unwrap_or(!has);
                    if add {
                        el.attributes.insert(name.clone(), String::new());
                        true
                    } else {
                        el.attributes.remove(&name);
                        false
                    }
                } else { false };
                cap.registry.mark_style_dirty(&cap.node);
                Ok(JsValue::from(result))
            },
            cap.clone(),
        ),
        js_string!("toggleAttribute"),
        2,
    );

    // Namespaced attribute variants (ignore namespace, delegate to plain attrs).
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let value = js_string_of(args.get(2).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.insert(name, value);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("setAttributeNS"),
        3,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    match el.attributes.get(&name) {
                        Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                        None => Ok(JsValue::null()),
                    }
                } else { Ok(JsValue::null()) }
            },
            cap.clone(),
        ),
        js_string!("getAttributeNS"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.remove(&name);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("removeAttributeNS"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    Ok(JsValue::from(el.attributes.contains_key(&name)))
                } else { Ok(JsValue::from(false)) }
            },
            cap.clone(),
        ),
        js_string!("hasAttributeNS"),
        2,
    );

    // getAttributeNode / setAttributeNode — return null/noop (not worth wiring up).
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
        js_string!("getAttributeNode"),
        1,
    );
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
        js_string!("setAttributeNode"),
        1,
    );
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
        js_string!("removeAttributeNode"),
        1,
    );

    // animate() — WAAPI stub that returns a minimal Animation object.
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let anim = ObjectInitializer::new(ctx)
                .property(js_string!("playState"), js_string!("idle"), Attribute::all())
                .property(js_string!("currentTime"), JsValue::null(), Attribute::all())
                .property(js_string!("finished"),
                    JsValue::undefined(), // ideally a Promise, but undefined is safe
                    Attribute::all())
                .property(js_string!("ready"),
                    JsValue::undefined(),
                    Attribute::all())
                .function(noop_native(), js_string!("play"), 0)
                .function(noop_native(), js_string!("pause"), 0)
                .function(noop_native(), js_string!("cancel"), 0)
                .function(noop_native(), js_string!("finish"), 0)
                .function(noop_native(), js_string!("reverse"), 0)
                .function(noop_native(), js_string!("addEventListener"), 2)
                .function(noop_native(), js_string!("removeEventListener"), 2)
                .build();
            Ok(anim.into())
        }),
        js_string!("animate"),
        2,
    );

    // getAnimations() — returns empty array.
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
        js_string!("getAnimations"),
        0,
    );

    // checkVisibility() — always true.
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(true))),
        js_string!("checkVisibility"),
        0,
    );

    // setPointerCapture / releasePointerCapture / hasPointerCapture — stubs.
    init.function(noop_native(), js_string!("setPointerCapture"), 1);
    init.function(noop_native(), js_string!("releasePointerCapture"), 1);
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(false))),
        js_string!("hasPointerCapture"),
        1,
    );

    // computedStyleMap() — returns empty map-like object.
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let map = ObjectInitializer::new(ctx)
                .property(js_string!("size"), 0, Attribute::all())
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
                    js_string!("get"),
                    1,
                )
                .build();
            Ok(map.into())
        }),
        js_string!("computedStyleMap"),
        0,
    );
}
