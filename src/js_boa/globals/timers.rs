use super::*;
use std::time::{Duration, Instant};

pub(in crate::js_boa) fn install_timers(
    context: &mut Context,
    global_obj: &JsObject,
) -> WindowCapture {
    let win_cap = WindowCapture {
        storage: Rc::new(RefCell::new(BTreeMap::new())),
        session: Rc::new(RefCell::new(BTreeMap::new())),
        next_timer: Rc::new(RefCell::new(1)),
        timers: Rc::new(RefCell::new(Vec::new())),
        animation_frames: Rc::new(RefCell::new(Vec::new())),
        microtasks: Rc::new(RefCell::new(Vec::new())),
        time_origin: Instant::now(),
    };

    let timeout_fn = |is_interval: bool| {
        NativeFunction::from_copy_closure_with_captures(
            move |_this: &JsValue, args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
                let id = next_timer_id(cap);
                let Some(callback) = args.get(0).and_then(|value| value.as_callable()).cloned()
                else {
                    return Ok(JsValue::from(id));
                };
                let delay = delay_arg(args.get(1));
                cap.timers.borrow_mut().push(TimerEntry {
                    id,
                    deadline: Instant::now() + delay,
                    interval: is_interval.then_some(delay),
                    callback,
                });
                Ok(JsValue::from(id))
            },
            win_cap.clone(),
        )
    };
    let _ = global_obj.set(
        js_string!("setTimeout"),
        native_to_jsfn(context, timeout_fn(false)),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("setInterval"),
        native_to_jsfn(context, timeout_fn(true)),
        false,
        context,
    );
    let raf_fn = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            let id = next_timer_id(cap);
            if let Some(callback) = args.get(0).and_then(|value| value.as_callable()).cloned() {
                cap.animation_frames
                    .borrow_mut()
                    .push(AnimationFrameEntry { id, callback });
            }
            Ok(JsValue::from(id))
        },
        win_cap.clone(),
    );
    let _ = global_obj.set(
        js_string!("requestAnimationFrame"),
        native_to_jsfn(context, raf_fn),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("requestIdleCallback"),
        native_to_jsfn(context, timeout_fn(false)),
        false,
        context,
    );
    let clear_timer = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            let id = timer_id_arg(args.get(0));
            cap.timers.borrow_mut().retain(|entry| entry.id != id);
            Ok(JsValue::undefined())
        },
        win_cap.clone(),
    );
    let _ = global_obj.set(
        js_string!("clearTimeout"),
        native_to_jsfn(context, clear_timer.clone()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("clearInterval"),
        native_to_jsfn(context, clear_timer.clone()),
        false,
        context,
    );
    let cancel_raf = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            let id = timer_id_arg(args.get(0));
            cap.animation_frames
                .borrow_mut()
                .retain(|entry| entry.id != id);
            Ok(JsValue::undefined())
        },
        win_cap.clone(),
    );
    let _ = global_obj.set(
        js_string!("cancelAnimationFrame"),
        native_to_jsfn(context, cancel_raf),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("cancelIdleCallback"),
        native_to_jsfn(context, clear_timer.clone()),
        false,
        context,
    );

    let queue_micro = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            if let Some(callback) = args.get(0).and_then(|value| value.as_callable()).cloned() {
                cap.microtasks.borrow_mut().push(callback);
            }
            Ok(JsValue::undefined())
        },
        win_cap.clone(),
    );
    let _ = global_obj.set(
        js_string!("queueMicrotask"),
        native_to_jsfn(context, queue_micro),
        false,
        context,
    );
    win_cap
}

fn next_timer_id(cap: &WindowCapture) -> u32 {
    let mut next = cap.next_timer.borrow_mut();
    let id = *next;
    *next += 1;
    id
}

fn timer_id_arg(value: Option<&JsValue>) -> u32 {
    value
        .and_then(|value| value.as_number())
        .map(|value| value as u32)
        .unwrap_or(0)
}

fn delay_arg(value: Option<&JsValue>) -> Duration {
    let millis = value.and_then(|value| value.as_number()).unwrap_or(0.0);
    Duration::from_millis(millis.max(0.0) as u64)
}
