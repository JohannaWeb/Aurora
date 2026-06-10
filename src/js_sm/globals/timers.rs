#![allow(unsafe_op_in_unsafe_fn)]
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, JSObject, Value};
use mozjs::jsval::{Int32Value, UndefinedValue};
use mozjs::rooted;

use crate::js_sm::capture::{AnimationFrameEntry, TimerEntry};
use crate::js_sm::utils::*;

pub(in crate::js_sm) unsafe fn install_timers(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    define_fn(cx, global, c"setTimeout", Some(set_timeout), 2);
    define_fn(cx, global, c"setInterval", Some(set_interval), 2);
    define_fn(cx, global, c"clearTimeout", Some(clear_timer), 1);
    define_fn(cx, global, c"clearInterval", Some(clear_timer), 1);
    define_fn(
        cx,
        global,
        c"requestAnimationFrame",
        Some(request_animation_frame),
        1,
    );
    define_fn(
        cx,
        global,
        c"cancelAnimationFrame",
        Some(cancel_animation_frame),
        1,
    );
    define_fn(
        cx,
        global,
        c"requestIdleCallback",
        Some(request_idle_callback),
        2,
    );
    define_fn(cx, global, c"cancelIdleCallback", Some(clear_timer), 1);
    define_fn(cx, global, c"queueMicrotask", Some(queue_microtask), 1);
}

unsafe extern "C" fn set_timeout(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    register_timer(cx, argc, vp, false, false)
}

unsafe extern "C" fn set_interval(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    register_timer(cx, argc, vp, true, false)
}

unsafe extern "C" fn request_idle_callback(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    register_timer(cx, argc, vp, false, true)
}

unsafe fn register_timer(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
    is_interval: bool,
    is_idle: bool,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ == 0 || !args.get(0).get().is_object() {
        args.rval().set(Int32Value(0));
        return true;
    }

    let state = &mut *get_state_ptr(&cx);
    let id = state.window.next_id();

    // Store callback on global to prevent GC
    rooted!(&in(cx) let cb_val = args.get(0).get());
    rooted!(&in(cx) let global = state.global);
    store_callback(&mut cx, global.handle(), id, cb_val.handle());

    let idle_timeout = if is_idle {
        if args.argc_ > 1 && args.get(1).get().is_object() {
            rooted!(&in(cx) let opts = args.get(1).get().to_object_or_null());
            let delay_ms = get_prop_f64(&mut cx, opts.handle(), c"timeout").max(0.0);
            Some(Duration::from_millis(delay_ms as u64))
        } else {
            None
        }
    } else {
        None
    };
    let delay_ms = if is_idle {
        idle_timeout.map(|d| d.as_millis() as f64).unwrap_or(0.0)
    } else {
        arg_to_f64(&args, 1).max(0.0)
    };
    let delay = Duration::from_millis(delay_ms as u64);

    state.window.timers.push(TimerEntry {
        id,
        deadline: Instant::now() + delay,
        interval: is_interval.then_some(delay),
        is_idle,
        idle_timeout,
    });

    args.rval().set(Int32Value(id as i32));
    true
}

unsafe extern "C" fn clear_timer(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let id = arg_to_f64(&args, 0) as u32;

    let state = &mut *get_state_ptr(&cx);
    state.window.timers.retain(|t| t.id != id);
    state.window.animation_frames.retain(|r| r.id != id);

    rooted!(&in(cx) let global = state.global);
    delete_callback(&mut cx, global.handle(), id);

    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn request_animation_frame(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ == 0 || !args.get(0).get().is_object() {
        args.rval().set(Int32Value(0));
        return true;
    }

    let state = &mut *get_state_ptr(&cx);
    let id = state.window.next_id();

    rooted!(&in(cx) let cb_val = args.get(0).get());
    rooted!(&in(cx) let global = state.global);
    store_callback(&mut cx, global.handle(), id, cb_val.handle());

    state
        .window
        .animation_frames
        .push(AnimationFrameEntry { id });
    args.rval().set(Int32Value(id as i32));
    true
}

unsafe extern "C" fn cancel_animation_frame(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    clear_timer(cx, argc, vp)
}

unsafe extern "C" fn queue_microtask(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ > 0 && args.get(0).get().is_object() {
        let state = &mut *get_state_ptr(&cx);
        let id = state.window.next_id();

        rooted!(&in(cx) let cb_val = args.get(0).get());
        rooted!(&in(cx) let global = state.global);
        store_callback(&mut cx, global.handle(), id, cb_val.handle());

        state.window.microtask_ids.push(id);
    }

    args.rval().set(UndefinedValue());
    true
}
