use super::*;

pub(in crate::js_boa) fn add_document_properties(init: &mut ObjectInitializer<'_>) {
    init.property(
        js_string!("readyState"),
        js_string!("complete"),
        Attribute::all(),
    )
    .property(
        js_string!("compatMode"),
        js_string!("CSS1Compat"),
        Attribute::all(),
    )
    .property(js_string!("charset"), js_string!("UTF-8"), Attribute::all())
    .property(
        js_string!("contentType"),
        js_string!("text/html"),
        Attribute::all(),
    )
    .property(js_string!("cookie"), js_string!(""), Attribute::all())
    .property(js_string!("title"), js_string!(""), Attribute::all())
    .property(js_string!("referrer"), js_string!(""), Attribute::all())
    .property(
        js_string!("URL"),
        js_string!("http://localhost/"),
        Attribute::all(),
    )
    .property(
        js_string!("domain"),
        js_string!("localhost"),
        Attribute::all(),
    )
    .property(js_string!("hidden"), false, Attribute::all())
    .property(js_string!("nodeType"), 9, Attribute::all())
    .property(
        js_string!("nodeName"),
        js_string!("#document"),
        Attribute::all(),
    )
    .property(
        js_string!("visibilityState"),
        js_string!("visible"),
        Attribute::all(),
    );
}
