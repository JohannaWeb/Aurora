use super::*;

pub(in crate::js_boa) fn install_timers(
    context: &mut Context,
    global_obj: &JsObject,
) -> WindowCapture {
    let win_cap = WindowCapture {
        storage: Rc::new(RefCell::new(BTreeMap::new())),
        session: Rc::new(RefCell::new(BTreeMap::new())),
        next_timer: Rc::new(RefCell::new(1)),
    };

    let timer_id_fn =
        |_this: &JsValue, _args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            let mut next = cap.next_timer.borrow_mut();
            let id = *next;
            *next += 1;
            Ok(JsValue::from(id))
        };
    let _ = global_obj.set(
        js_string!("setTimeout"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("setInterval"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("requestAnimationFrame"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("requestIdleCallback"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("clearTimeout"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("clearInterval"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("cancelAnimationFrame"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("cancelIdleCallback"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );

    // queueMicrotask: invoke the callback right now.
    let queue_micro = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        if let Some(cb) = args.get(0).and_then(|v| v.as_callable()).cloned() {
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(JsValue::undefined())
    });
    let _ = global_obj.set(
        js_string!("queueMicrotask"),
        native_to_jsfn(context, queue_micro),
        false,
        context,
    );
    win_cap
}
