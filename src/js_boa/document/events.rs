use super::*;

pub(in crate::js_boa) fn add_document_event_methods(init: &mut ObjectInitializer<'_>) {
    init.function(noop_native(), js_string!("addEventListener"), 2)
        .function(noop_native(), js_string!("removeEventListener"), 2)
        .function(return_bool(true), js_string!("dispatchEvent"), 1)
        .function(noop_native(), js_string!("open"), 0)
        .function(noop_native(), js_string!("close"), 0)
        .function(noop_native(), js_string!("write"), 1)
        .function(noop_native(), js_string!("writeln"), 1)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!("")))),
            js_string!("execCommand"),
            3,
        )
        .function(return_bool(false), js_string!("hasFocus"), 0);
}
