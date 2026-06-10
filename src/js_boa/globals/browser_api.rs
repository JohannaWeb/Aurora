use super::*;

pub(in crate::js_boa) fn install_browser_apis(
    context: &mut Context,
    global_obj: &JsObject,
    win_cap: &WindowCapture,
) {
    let _ = global_obj.set(
        js_string!("alert"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("confirm"),
        native_to_jsfn(context, return_bool(false)),
        false,
        context,
    );
    let prompt_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null()));
    let _ = global_obj.set(
        js_string!("prompt"),
        native_to_jsfn(context, prompt_fn),
        false,
        context,
    );

    // matchMedia returns an object that never matches.
    let match_media = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let media = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("matches"), false, Attribute::all())
            .property(js_string!("media"), JsString::from(media), Attribute::all())
            .function(noop_native(), js_string!("addListener"), 1)
            .function(noop_native(), js_string!("removeListener"), 1)
            .function(noop_native(), js_string!("addEventListener"), 2)
            .function(noop_native(), js_string!("removeEventListener"), 2)
            .function(return_bool(true), js_string!("dispatchEvent"), 1)
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("matchMedia"),
        native_to_jsfn(context, match_media),
        false,
        context,
    );

    // getComputedStyle returns an empty CSSStyleDeclaration-ish object.
    let gcs = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!("")))),
                js_string!("getPropertyValue"),
                1,
            )
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("getComputedStyle"),
        native_to_jsfn(context, gcs),
        false,
        context,
    );

    // Scrolling
    let _ = global_obj.set(
        js_string!("scrollTo"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("scrollBy"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("scroll"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );

    // structuredClone — JSON round-trip covers the 95% case YouTube uses it for.
    let structured_clone = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let val = args.get(0).cloned().unwrap_or(JsValue::undefined());
        let json_str = val.to_json(ctx)?;
        JsValue::from_json(&json_str, ctx)
    });
    let _ = global_obj.set(
        js_string!("structuredClone"),
        native_to_jsfn(context, structured_clone),
        false,
        context,
    );

    // reportError — surfaced errors, treat as console.error.
    let report_error = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let msg = args
            .get(0)
            .map(|v| v.display().to_string())
            .unwrap_or_default();
        eprintln!("JS reportError: {msg}");
        Ok(JsValue::undefined())
    });
    let _ = global_obj.set(
        js_string!("reportError"),
        native_to_jsfn(context, report_error),
        false,
        context,
    );

    // window.open() — returns a minimal stub window; scripts check the return value.
    let win_open = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let url = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        eprintln!("Aurora: window.open({url}) suppressed");
        let stub = ObjectInitializer::new(ctx)
            .property(js_string!("closed"), false, Attribute::all())
            .function(noop_native(), js_string!("close"), 0)
            .function(noop_native(), js_string!("focus"), 0)
            .function(noop_native(), js_string!("postMessage"), 3)
            .build();
        Ok(stub.into())
    });
    let _ = global_obj.set(
        js_string!("open"),
        native_to_jsfn(context, win_open),
        false,
        context,
    );

    // getSelection() — stub; YouTube reads .anchorNode etc.
    let get_selection = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let sel = ObjectInitializer::new(ctx)
            .property(js_string!("anchorNode"), JsValue::null(), Attribute::all())
            .property(js_string!("focusNode"), JsValue::null(), Attribute::all())
            .property(js_string!("rangeCount"), 0, Attribute::all())
            .property(js_string!("isCollapsed"), true, Attribute::all())
            .property(js_string!("type"), js_string!("None"), Attribute::all())
            .function(noop_native(), js_string!("removeAllRanges"), 0)
            .function(noop_native(), js_string!("collapse"), 2)
            .function(noop_native(), js_string!("addRange"), 1)
            .function(
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null())),
                js_string!("getRangeAt"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!("")))),
                js_string!("toString"),
                0,
            )
            .build();
        Ok(sel.into())
    });
    let _ = global_obj.set(
        js_string!("getSelection"),
        native_to_jsfn(context, get_selection),
        false,
        context,
    );

    // atob / btoa
    let atob = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let s = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let decoded = base64_decode(&s).unwrap_or_default();
        Ok(JsValue::from(JsString::from(decoded)))
    });
    let btoa = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let s = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        Ok(JsValue::from(JsString::from(base64_encode(s.as_bytes()))))
    });
    let _ = global_obj.set(
        js_string!("atob"),
        native_to_jsfn(context, atob),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("btoa"),
        native_to_jsfn(context, btoa),
        false,
        context,
    );

    // Storage: localStorage, sessionStorage.
    install_storage(
        context,
        &global_obj,
        "localStorage",
        win_cap.storage.clone(),
    );
    // Console is already installed; register remaining boa_runtime builtins directly.
    let _ = boa_runtime::TextDecoder::register(context);
    let _ = boa_runtime::TextEncoder::register(context);
    let _ = boa_runtime::url::Url::register(context);
    install_storage(
        context,
        &global_obj,
        "sessionStorage",
        win_cap.session.clone(),
    );
}
