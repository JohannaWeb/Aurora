use super::*;

pub(in crate::js_boa) fn install_window_core(context: &mut Context, global_obj: &JsObject) {
    let _ = context.register_global_property(
        js_string!("globalThis"),
        global_obj.clone(),
        Attribute::all(),
    );
    let _ = context.register_global_property(
        js_string!("window"),
        global_obj.clone(),
        Attribute::all(),
    );
    let _ =
        context.register_global_property(js_string!("self"), global_obj.clone(), Attribute::all());
    let _ =
        context.register_global_property(js_string!("top"), global_obj.clone(), Attribute::all());
    let _ = context.register_global_property(
        js_string!("parent"),
        global_obj.clone(),
        Attribute::all(),
    );

    // Console with a handful of methods.
    let console = ObjectInitializer::new(context)
        .function(log_native(), js_string!("log"), 1)
        .function(log_native(), js_string!("info"), 1)
        .function(log_native(), js_string!("warn"), 1)
        .function(log_native(), js_string!("error"), 1)
        .function(log_native(), js_string!("debug"), 1)
        .function(log_native(), js_string!("trace"), 1)
        .function(noop_native(), js_string!("group"), 0)
        .function(noop_native(), js_string!("groupEnd"), 0)
        .function(noop_native(), js_string!("time"), 0)
        .function(noop_native(), js_string!("timeEnd"), 0)
        .build();
    let _ = context.register_global_property(js_string!("console"), console, Attribute::all());

    // Window event listeners (no-op).
    let _ = global_obj.set(
        js_string!("addEventListener"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("removeEventListener"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("dispatchEvent"),
        native_to_jsfn(context, return_bool(true)),
        false,
        context,
    );

    // Viewport & screen stubs.
    for (name, val) in [
        ("innerWidth", 1200.0),
        ("innerHeight", 800.0),
        ("outerWidth", 1200.0),
        ("outerHeight", 800.0),
        ("devicePixelRatio", 1.0),
        ("scrollX", 0.0),
        ("scrollY", 0.0),
        ("pageXOffset", 0.0),
        ("pageYOffset", 0.0),
    ] {
        let _ = context.register_global_property(JsString::from(name), val, Attribute::all());
    }

    let screen = ObjectInitializer::new(context)
        .property(js_string!("width"), 1200, Attribute::all())
        .property(js_string!("height"), 800, Attribute::all())
        .property(js_string!("availWidth"), 1200, Attribute::all())
        .property(js_string!("availHeight"), 800, Attribute::all())
        .property(js_string!("colorDepth"), 24, Attribute::all())
        .property(js_string!("pixelDepth"), 24, Attribute::all())
        .build();
    let _ = context.register_global_property(js_string!("screen"), screen, Attribute::all());
}
