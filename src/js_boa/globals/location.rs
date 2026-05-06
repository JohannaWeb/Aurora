use super::*;

pub(in crate::js_boa) fn install_location(context: &mut Context) {
    let location = ObjectInitializer::new(context)
        .property(
            js_string!("href"),
            js_string!("http://localhost/"),
            Attribute::all(),
        )
        .property(
            js_string!("origin"),
            js_string!("http://localhost"),
            Attribute::all(),
        )
        .property(
            js_string!("protocol"),
            js_string!("http:"),
            Attribute::all(),
        )
        .property(
            js_string!("host"),
            js_string!("localhost"),
            Attribute::all(),
        )
        .property(
            js_string!("hostname"),
            js_string!("localhost"),
            Attribute::all(),
        )
        .property(js_string!("port"), js_string!(""), Attribute::all())
        .property(js_string!("pathname"), js_string!("/"), Attribute::all())
        .property(js_string!("search"), js_string!(""), Attribute::all())
        .property(js_string!("hash"), js_string!(""), Attribute::all())
        .function(noop_native(), js_string!("assign"), 1)
        .function(noop_native(), js_string!("replace"), 1)
        .function(noop_native(), js_string!("reload"), 0)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
                Ok(JsValue::from(js_string!("http://localhost/")))
            }),
            js_string!("toString"),
            0,
        )
        .build();
    let _ = context.register_global_property(js_string!("location"), location, Attribute::all());
}
