use super::*;

pub(super) fn install_accessor(
    obj: &JsObject,
    context: &mut Context,
    name: &str,
    getter: Option<NativeFunction>,
    setter: Option<NativeFunction>,
) {
    let desc = ObjectInitializer::new(context)
        .property(js_string!("enumerable"), true, Attribute::all())
        .property(js_string!("configurable"), true, Attribute::all())
        .build();

    if let Some(g) = getter {
        let _ = desc.set(
            js_string!("get"),
            native_to_jsfn(context, g),
            false,
            context,
        );
    }
    if let Some(s) = setter {
        let _ = desc.set(
            js_string!("set"),
            native_to_jsfn(context, s),
            false,
            context,
        );
    }

    let define = context
        .global_object()
        .get(js_string!("Object"), context)
        .ok()
        .and_then(|o| o.as_object().cloned())
        .and_then(|o| o.get(js_string!("defineProperty"), context).ok())
        .and_then(|v| v.as_callable().cloned());

    if let Some(define) = define {
        let _ = define.call(
            &JsValue::undefined(),
            &[obj.clone().into(), JsString::from(name).into(), desc.into()],
            context,
        );
    }
}
