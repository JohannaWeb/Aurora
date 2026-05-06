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
    install_storage(
        context,
        &global_obj,
        "sessionStorage",
        win_cap.session.clone(),
    );
}
