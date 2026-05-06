use super::*;

pub(in crate::js_boa) fn install_navigator(context: &mut Context) {
    let navigator = ObjectInitializer::new(context)
        .property(
            js_string!("userAgent"),
            js_string!("Aurora/0.1"),
            Attribute::all(),
        )
        .property(
            js_string!("appName"),
            js_string!("Netscape"),
            Attribute::all(),
        )
        .property(
            js_string!("appVersion"),
            js_string!("5.0"),
            Attribute::all(),
        )
        .property(
            js_string!("platform"),
            js_string!("Linux x86_64"),
            Attribute::all(),
        )
        .property(
            js_string!("language"),
            js_string!("en-US"),
            Attribute::all(),
        )
        .property(js_string!("vendor"), js_string!(""), Attribute::all())
        .property(js_string!("onLine"), true, Attribute::all())
        .property(js_string!("cookieEnabled"), false, Attribute::all())
        .property(js_string!("doNotTrack"), js_string!("1"), Attribute::all())
        .property(js_string!("hardwareConcurrency"), 4, Attribute::all())
        .property(js_string!("maxTouchPoints"), 0, Attribute::all())
        .build();
    // languages array.
    if let Ok(langs) = JsArray::from_iter(
        [
            JsValue::from(js_string!("en-US")),
            JsValue::from(js_string!("en")),
        ],
        context,
    )
    .pipe(Ok::<_, JsError>)
    {
        let _ = navigator.set(js_string!("languages"), langs, false, context);
    }
    let _ = context.register_global_property(js_string!("navigator"), navigator, Attribute::all());
}
