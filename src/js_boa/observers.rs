use super::*;

pub(super) fn install_observers(context: &mut Context) {
    let global = context.global_object().clone();

    for name in [
        "MutationObserver",
        "IntersectionObserver",
        "ResizeObserver",
        "PerformanceObserver",
    ] {
        let ctor = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let obj = ObjectInitializer::new(ctx)
                .function(noop_native(), js_string!("observe"), 2)
                .function(noop_native(), js_string!("unobserve"), 1)
                .function(noop_native(), js_string!("disconnect"), 0)
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
                    js_string!("takeRecords"),
                    0,
                )
                .build();
            Ok(obj.into())
        });
        let _ = global.set(
            JsString::from(name),
            native_to_jsfn(context, ctor),
            false,
            context,
        );
    }
}
