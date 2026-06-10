#![allow(unsafe_op_in_unsafe_fn)]
use std::ptr::NonNull;

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, JSObject, Value};
use mozjs::jsval::{BooleanValue, DoubleValue, ObjectValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::js_sm::utils::*;

pub(in crate::js_sm) unsafe fn install_core_globals(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    // globalThis / window / self / top / parent all point to the global itself
    rooted!(&in(cx) let gval = ObjectValue(*global));
    for name in &[c"globalThis", c"window", c"self", c"top", c"parent"] {
        wrappers2::JS_SetProperty(cx, global, name.as_ptr(), gval.handle());
    }

    // console
    let console = new_plain_object(cx);
    rooted!(&in(cx) let console_root = console);
    for name in &[
        c"log",
        c"info",
        c"warn",
        c"error",
        c"debug",
        c"trace",
        c"dir",
        c"dirxml",
        c"table",
        c"group",
        c"groupCollapsed",
        c"groupEnd",
        c"time",
        c"timeEnd",
        c"timeLog",
        c"timeStamp",
        c"clear",
        c"count",
        c"countReset",
        c"profile",
        c"profileEnd",
    ] {
        define_fn(cx, console_root.handle(), name, Some(console_log), 1);
    }
    define_fn(
        cx,
        console_root.handle(),
        c"assert",
        Some(console_assert),
        2,
    );
    set_prop_obj(cx, global, c"console", console);

    // Viewport / screen numbers
    for (name, val) in [
        (c"innerWidth", 1200.0_f64),
        (c"innerHeight", 800.0),
        (c"outerWidth", 1200.0),
        (c"outerHeight", 800.0),
        (c"devicePixelRatio", 1.0),
        (c"scrollX", 0.0),
        (c"scrollY", 0.0),
        (c"pageXOffset", 0.0),
        (c"pageYOffset", 0.0),
    ] {
        set_prop_f64(cx, global, name, val);
    }

    let screen = new_plain_object(cx);
    rooted!(&in(cx) let screen_root = screen);
    for (name, val) in [
        (c"width", 1200.0_f64),
        (c"height", 800.0),
        (c"availWidth", 1200.0),
        (c"availHeight", 800.0),
        (c"colorDepth", 24.0),
        (c"pixelDepth", 24.0),
    ] {
        set_prop_f64(cx, screen_root.handle(), name, val);
    }
    set_prop_obj(cx, global, c"screen", screen);

    // Window event listeners share the document-level listener bucket for
    // startup events such as DOMContentLoaded.
    define_fn(
        cx,
        global,
        c"addEventListener",
        Some(window_add_event_listener),
        2,
    );
    define_fn(cx, global, c"removeEventListener", Some(noop), 2);
    define_fn(cx, global, c"dispatchEvent", Some(return_true), 1);
    define_fn(cx, global, c"focus", Some(noop), 0);
    define_fn(cx, global, c"blur", Some(noop), 0);
    define_fn(cx, global, c"close", Some(noop), 0);
    define_fn(cx, global, c"postMessage", Some(noop), 3);
    define_fn(cx, global, c"scrollTo", Some(noop), 2);
    define_fn(cx, global, c"scrollBy", Some(noop), 2);
    define_fn(cx, global, c"scroll", Some(noop), 2);
    define_fn(cx, global, c"resizeTo", Some(noop), 2);
    define_fn(cx, global, c"moveTo", Some(noop), 2);

    // performance
    let perf = new_plain_object(cx);
    rooted!(&in(cx) let perf_root = perf);
    define_fn(cx, perf_root.handle(), c"now", Some(perf_now), 0);
    define_fn(cx, perf_root.handle(), c"mark", Some(noop), 1);
    define_fn(cx, perf_root.handle(), c"measure", Some(noop), 1);
    define_fn(cx, perf_root.handle(), c"clearMarks", Some(noop), 0);
    define_fn(cx, perf_root.handle(), c"clearMeasures", Some(noop), 0);
    define_fn(
        cx,
        perf_root.handle(),
        c"getEntriesByType",
        Some(return_empty_array),
        1,
    );
    define_fn(
        cx,
        perf_root.handle(),
        c"getEntriesByName",
        Some(return_empty_array),
        1,
    );
    define_fn(
        cx,
        perf_root.handle(),
        c"getEntries",
        Some(return_empty_array),
        0,
    );
    set_prop_f64(cx, perf_root.handle(), c"timeOrigin", 0.0);

    // performance.timing (Navigation Timing API) — sites read these to compute
    // load metrics; populate with a single "everything happened now" timestamp
    // so arithmetic on them produces sane (zero-ish) numbers instead of NaN.
    let now = epoch_millis();
    let timing = new_plain_object(cx);
    rooted!(&in(cx) let timing_root = timing);
    for name in &[
        c"navigationStart",
        c"unloadEventStart",
        c"unloadEventEnd",
        c"redirectStart",
        c"redirectEnd",
        c"fetchStart",
        c"domainLookupStart",
        c"domainLookupEnd",
        c"connectStart",
        c"connectEnd",
        c"secureConnectionStart",
        c"requestStart",
        c"responseStart",
        c"responseEnd",
        c"domLoading",
        c"domInteractive",
        c"domContentLoadedEventStart",
        c"domContentLoadedEventEnd",
        c"domComplete",
        c"loadEventStart",
        c"loadEventEnd",
    ] {
        set_prop_f64(cx, timing_root.handle(), name, now);
    }
    set_prop_obj(cx, perf_root.handle(), c"timing", timing);
    set_prop_obj(cx, global, c"performance", perf);

    // navigator
    let navigator = new_plain_object(cx);
    rooted!(&in(cx) let nav_root = navigator);
    set_prop_str(
        cx,
        nav_root.handle(),
        c"userAgent",
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Aurora/0.3 Safari/537.36",
    );
    set_prop_str(cx, nav_root.handle(), c"platform", "Linux x86_64");
    set_prop_str(cx, nav_root.handle(), c"language", "en-US");
    set_prop_str(cx, nav_root.handle(), c"vendor", "");
    set_prop_str(cx, nav_root.handle(), c"appName", "Netscape");
    set_prop_str(cx, nav_root.handle(), c"appVersion", "5.0 (X11)");
    set_prop_str(cx, nav_root.handle(), c"product", "Gecko");
    set_prop_bool(cx, nav_root.handle(), c"onLine", true);
    set_prop_bool(cx, nav_root.handle(), c"cookieEnabled", true);
    set_prop_f64(cx, nav_root.handle(), c"hardwareConcurrency", 4.0);
    set_prop_f64(cx, nav_root.handle(), c"maxTouchPoints", 0.0);
    define_fn(cx, nav_root.handle(), c"sendBeacon", Some(return_true), 2);
    define_fn(cx, nav_root.handle(), c"vibrate", Some(return_true), 1);
    define_fn(
        cx,
        nav_root.handle(),
        c"registerProtocolHandler",
        Some(noop),
        3,
    );

    let geolocation = new_plain_object(cx);
    rooted!(&in(cx) let geo_root = geolocation);
    define_fn(cx, geo_root.handle(), c"getCurrentPosition", Some(noop), 3);
    define_fn(cx, geo_root.handle(), c"watchPosition", Some(noop), 3);
    define_fn(cx, geo_root.handle(), c"clearWatch", Some(noop), 1);
    set_prop_obj(cx, nav_root.handle(), c"geolocation", geolocation);

    set_prop_obj(cx, global, c"navigator", navigator);

    // crypto stub
    let crypto = new_plain_object(cx);
    rooted!(&in(cx) let crypto_root = crypto);
    define_fn(
        cx,
        crypto_root.handle(),
        c"getRandomValues",
        Some(crypto_get_random_values),
        1,
    );
    define_fn(
        cx,
        crypto_root.handle(),
        c"randomUUID",
        Some(crypto_random_uuid),
        0,
    );
    set_prop_obj(cx, global, c"crypto", crypto);

    // history stub
    let history = new_plain_object(cx);
    rooted!(&in(cx) let hist_root = history);
    define_fn(cx, hist_root.handle(), c"pushState", Some(noop), 3);
    define_fn(cx, hist_root.handle(), c"replaceState", Some(noop), 3);
    define_fn(cx, hist_root.handle(), c"back", Some(noop), 0);
    define_fn(cx, hist_root.handle(), c"forward", Some(noop), 0);
    define_fn(cx, hist_root.handle(), c"go", Some(noop), 1);
    set_prop_i32(cx, hist_root.handle(), c"length", 1);
    set_prop_obj(cx, global, c"history", history);

    // location
    let location = new_plain_object(cx);
    rooted!(&in(cx) let location_root = location);
    set_prop_str(cx, location_root.handle(), c"href", "https://youtube.com/");
    set_prop_str(cx, location_root.handle(), c"origin", "https://youtube.com");
    set_prop_str(cx, location_root.handle(), c"protocol", "https:");
    set_prop_str(cx, location_root.handle(), c"host", "youtube.com");
    set_prop_str(cx, location_root.handle(), c"hostname", "youtube.com");
    set_prop_str(cx, location_root.handle(), c"port", "");
    set_prop_str(cx, location_root.handle(), c"pathname", "/");
    set_prop_str(cx, location_root.handle(), c"search", "");
    set_prop_str(cx, location_root.handle(), c"hash", "");
    define_fn(cx, location_root.handle(), c"assign", Some(noop), 1);
    define_fn(cx, location_root.handle(), c"replace", Some(noop), 1);
    define_fn(cx, location_root.handle(), c"reload", Some(noop), 0);
    set_prop_obj(cx, global, c"location", location);

    // Internal sync fetch helper
    define_fn(
        cx,
        global,
        c"__aurora_fetch_sync__",
        Some(aurora_fetch_sync),
        1,
    );
}

// ── Native implementations ────────────────────────────────────────────────────

unsafe extern "C" fn noop(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn return_true(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(true));
    true
}

unsafe extern "C" fn return_empty_array(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    use mozjs::jsapi::HandleValueArray;
    let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
    true
}

unsafe extern "C" fn console_log(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let parts: Vec<String> = (0..args.argc_)
        .map(|i| {
            rooted!(&in(cx) let v = args.get(i).get());
            value_to_string(&mut cx, v.handle())
        })
        .collect();
    log::info!(target: "aurora::js", "[JS] {}", parts.join(" "));
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn console_assert(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let ok = args.argc_ > 0 && args.get(0).get().to_boolean();
    if !ok {
        let msg = if args.argc_ > 1 {
            rooted!(&in(cx) let v = args.get(1).get());
            value_to_string(&mut cx, v.handle())
        } else {
            "Assertion failed".to_string()
        };
        eprintln!("JS assert: {}", msg);
    }
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn window_add_event_listener(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc < 2 {
        args.rval().set(UndefinedValue());
        return true;
    }

    let event_type = arg_to_string(&mut cx, &args, 0);
    let cb_val = args.get(1).get();
    if !cb_val.is_object() {
        args.rval().set(UndefinedValue());
        return true;
    }

    let state = &mut *get_state_ptr(&cx);
    let cb_id = state.window.next_id();
    rooted!(&in(cx) let cb_handle = cb_val);
    rooted!(&in(cx) let global = state.global);
    store_callback(&mut cx, global.handle(), cb_id, cb_handle.handle());
    state.registry.add_listener(0, event_type, cb_id);

    args.rval().set(UndefinedValue());
    true
}

fn epoch_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

unsafe extern "C" fn perf_now(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(DoubleValue(epoch_millis()));
    true
}

unsafe extern "C" fn crypto_get_random_values(
    _cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    if args.argc_ > 0 {
        let buf = args.get(0).get();
        args.rval().set(buf);
    } else {
        args.rval().set(UndefinedValue());
    }
    true
}

unsafe extern "C" fn crypto_random_uuid(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    // Generate a v4-ish UUID using random bytes from the OS
    use std::fmt::Write as _;
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            & 0xFF) as u8;
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let mut uuid = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            uuid.push('-');
        }
        let _ = write!(uuid, "{:02x}", b);
    }
    let js_str = new_js_string(&mut cx, &uuid);
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval().set(mozjs::jsval::StringValue(&*js_str));
    }
    true
}

unsafe extern "C" fn aurora_fetch_sync(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let url = arg_to_string(&mut cx, &args, 0);

    let result_obj = wrappers2::JS_NewPlainObject(&mut cx);
    if result_obj.is_null() {
        args.rval().set(UndefinedValue());
        return true;
    }
    rooted!(&in(cx) let result_root = result_obj);

    match crate::fetch::http::fetch_string(&url) {
        Ok(body) => {
            set_prop_bool(&mut cx, result_root.handle(), c"ok", true);
            set_prop_i32(&mut cx, result_root.handle(), c"status", 200);
            set_prop_str(&mut cx, result_root.handle(), c"body", &body);
        }
        Err(e) => {
            set_prop_bool(&mut cx, result_root.handle(), c"ok", false);
            set_prop_i32(&mut cx, result_root.handle(), c"status", 0);
            set_prop_str(&mut cx, result_root.handle(), c"body", "");
            set_prop_str(&mut cx, result_root.handle(), c"error", &e.to_string());
        }
    }
    args.rval().set(ObjectValue(result_obj));
    true
}
