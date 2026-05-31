use super::*;

pub(in crate::js_boa) fn install_platform_objects(context: &mut Context, global_obj: &JsObject) {
    let history = ObjectInitializer::new(context)
        .property(js_string!("length"), 1, Attribute::all())
        .property(js_string!("state"), JsValue::null(), Attribute::all())
        .function(noop_native(), js_string!("pushState"), 3)
        .function(noop_native(), js_string!("replaceState"), 3)
        .function(noop_native(), js_string!("back"), 0)
        .function(noop_native(), js_string!("forward"), 0)
        .function(noop_native(), js_string!("go"), 1)
        .build();
    let _ = context.register_global_property(js_string!("history"), history, Attribute::all());

    // Performance
    let perf = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(0.0))),
            js_string!("now"),
            0,
        )
        .function(noop_native(), js_string!("mark"), 1)
        .function(noop_native(), js_string!("measure"), 3)
        .function(noop_native(), js_string!("clearMarks"), 0)
        .function(noop_native(), js_string!("clearMeasures"), 0)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntries"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntriesByType"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntriesByName"),
            2,
        )
        .build();
    let _ = perf.set(js_string!("timeOrigin"), 0.0, false, context);
    let _ = context.register_global_property(js_string!("performance"), perf, Attribute::all());

    // Crypto is intentionally absent-for-now rather than fake-random.
    let crypto = ObjectInitializer::new(context)
        .function(unsupported_crypto(), js_string!("randomUUID"), 0)
        .function(unsupported_crypto(), js_string!("getRandomValues"), 1)
        .build();
    let _ = context.register_global_property(js_string!("crypto"), crypto, Attribute::all());

    // Event constructor (minimal).
    let event_ctor = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsString::from(type_name),
                Attribute::all(),
            )
            .property(js_string!("bubbles"), false, Attribute::all())
            .property(js_string!("cancelable"), false, Attribute::all())
            .property(js_string!("defaultPrevented"), false, Attribute::all())
            .function(noop_native(), js_string!("stopPropagation"), 0)
            .function(noop_native(), js_string!("stopImmediatePropagation"), 0)
            .build();
        install_prevent_default(ctx, &obj);
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("Event"),
        native_to_jsfn(context, event_ctor),
        false,
        context,
    );
    let custom_event = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsString::from(type_name),
                Attribute::all(),
            )
            .property(js_string!("detail"), JsValue::null(), Attribute::all())
            .function(noop_native(), js_string!("stopPropagation"), 0)
            .build();
        install_prevent_default(ctx, &obj);
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("CustomEvent"),
        native_to_jsfn(context, custom_event),
        false,
        context,
    );
}

fn unsupported_crypto() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
        Err(JsNativeError::typ()
            .with_message("crypto randomness is not implemented in Aurora")
            .into())
    })
}

fn install_prevent_default(ctx: &mut Context, event: &JsObject) {
    let event_clone = event.clone();
    let prevent = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, event: &JsObject, ctx| {
            let _ = event.set(
                js_string!("defaultPrevented"),
                JsValue::from(true),
                false,
                ctx,
            );
            Ok(JsValue::undefined())
        },
        event_clone,
    );
    let prevent_fn = NativeFunction::to_js_function(prevent, ctx.realm());
    let _ = event.set(js_string!("preventDefault"), prevent_fn, false, ctx);
}
