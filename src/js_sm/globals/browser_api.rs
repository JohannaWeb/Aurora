#![allow(unsafe_op_in_unsafe_fn)]
use std::ptr::NonNull;

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, JSObject, Value};
use mozjs::jsval::{BooleanValue, NullValue, ObjectValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::js_sm::utils::*;

pub(in crate::js_sm) unsafe fn install_browser_apis(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    define_fn(cx, global, c"alert", Some(noop), 1);
    define_fn(cx, global, c"confirm", Some(confirm_false), 1);
    define_fn(cx, global, c"prompt", Some(prompt_null), 1);
    define_fn(cx, global, c"print", Some(noop), 0);
    define_fn(cx, global, c"reportError", Some(report_error), 1);
    define_fn(cx, global, c"matchMedia", Some(match_media), 1);
    define_fn(cx, global, c"getComputedStyle", Some(get_computed_style), 2);
    define_fn(cx, global, c"getSelection", Some(get_selection_null), 0);
    define_fn(cx, global, c"structuredClone", Some(structured_clone), 1);
    define_fn(cx, global, c"atob", Some(atob), 1);
    define_fn(cx, global, c"btoa", Some(btoa), 1);
    define_fn(cx, global, c"open", Some(window_open), 3);

    // Storage — install minimal localStorage / sessionStorage
    let ls = new_storage_object(cx);
    set_prop_obj(cx, global, c"localStorage", ls);
    let ss = new_storage_object(cx);
    set_prop_obj(cx, global, c"sessionStorage", ss);

    // Minimal event target stubs for CustomEvent / Event constructors
    define_fn(cx, global, c"CustomEvent", Some(custom_event_ctor), 2);
    define_fn(cx, global, c"Event", Some(custom_event_ctor), 2);

    // ResizeObserver / MutationObserver / IntersectionObserver stubs
    for name in &[
        c"MutationObserver",
        c"IntersectionObserver",
        c"ResizeObserver",
        c"PerformanceObserver",
    ] {
        define_fn(cx, global, name, Some(observer_ctor), 1);
    }

    // URL constructor stub
    define_fn(cx, global, c"URL", Some(url_ctor), 2);
    // URLSearchParams stub
    define_fn(cx, global, c"URLSearchParams", Some(url_search_params_ctor), 1);
    // AbortController stub
    define_fn(cx, global, c"AbortController", Some(abort_controller_ctor), 0);
    // Headers stub
    define_fn(cx, global, c"Headers", Some(headers_ctor), 1);
    // FormData stub
    define_fn(cx, global, c"FormData", Some(noop_ctor), 1);

    // Blob / File stubs
    define_fn(cx, global, c"Blob", Some(blob_ctor), 2);
    define_fn(cx, global, c"File", Some(blob_ctor), 3);

    // Promise-based fetch — returns a rejected promise stub (sync fetch handled by __aurora_fetch_sync__)
    define_fn(cx, global, c"fetch", Some(fetch_stub), 1);

    // XHR constructor
    define_fn(cx, global, c"XMLHttpRequest", Some(xhr_ctor), 0);

    // WebSocket constructor stub
    define_fn(cx, global, c"WebSocket", Some(websocket_ctor), 2);
}

// ── Impl ─────────────────────────────────────────────────────────────────────

unsafe extern "C" fn noop(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn confirm_false(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(false));
    true
}

unsafe extern "C" fn prompt_null(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn report_error(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let msg = arg_to_string(&mut cx, &args, 0);
    eprintln!("JS reportError: {}", msg);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn match_media(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let media = arg_to_string(&mut cx, &args, 0);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);

    set_prop_bool(&mut cx, obj_root.handle(), c"matches", false);
    set_prop_str(&mut cx, obj_root.handle(), c"media", &media);
    define_fn(&mut cx, obj_root.handle(), c"addListener", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"removeListener", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeEventListener", Some(noop), 2);

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn get_computed_style(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"getPropertyValue", Some(return_empty_string), 1);
    define_fn(&mut cx, obj_root.handle(), c"setProperty", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeProperty", Some(return_empty_string), 1);

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn return_empty_string(cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);
    let js_str = new_js_string(&mut cx, "");
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval().set(mozjs::jsval::StringValue(unsafe { &*js_str }));
    }
    true
}

unsafe extern "C" fn get_selection_null(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn structured_clone(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    // Best-effort: pass through the first argument unchanged (deep clone not critical for a browser engine stub)
    let args = CallArgs::from_vp(vp, argc);
    if argc > 0 {
        args.rval().set(args.get(0).get());
    } else {
        args.rval().set(UndefinedValue());
    }
    true
}

unsafe extern "C" fn atob(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let encoded = arg_to_string(&mut cx, &args, 0);
    // Strip whitespace as per spec
    let cleaned: String = encoded.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let decoded = base64_decode(&cleaned);
    let js_str = new_js_string(&mut cx, &decoded);
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval().set(mozjs::jsval::StringValue(&*js_str));
    }
    true
}

unsafe extern "C" fn btoa(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let data = arg_to_string(&mut cx, &args, 0);
    let encoded = base64_encode(data.as_bytes());
    let js_str = new_js_string(&mut cx, &encoded);
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval().set(mozjs::jsval::StringValue(&*js_str));
    }
    true
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((b1 & 15) << 2 | b2 >> 6) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(b2 & 63) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> String {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let enc = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0usize;
        while i < enc.len() { t[enc[i] as usize] = i as i8; i += 1; }
        t
    };
    let bytes: Vec<u8> = s.bytes().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = TABLE[bytes[i] as usize];
        let b = TABLE[bytes[i+1] as usize];
        let c = TABLE[bytes[i+2] as usize];
        let d = TABLE[bytes[i+3] as usize];
        if a < 0 || b < 0 { break; }
        out.push(((a as u8) << 2) | ((b as u8) >> 4));
        if c >= 0 { out.push(((b as u8) << 4) | ((c as u8) >> 2)); }
        if d >= 0 { out.push(((c as u8) << 6) | d as u8); }
        i += 4;
    }
    String::from_utf8_lossy(&out).into_owned()
}

unsafe extern "C" fn window_open(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let url = arg_to_string(&mut cx, &args, 0);
    eprintln!("Aurora: window.open({}) suppressed", url);
    let stub = new_plain_object(&mut cx);
    rooted!(&in(cx) let stub_root = stub);
    define_fn(&mut cx, stub_root.handle(), c"close", Some(noop), 0);
    define_fn(&mut cx, stub_root.handle(), c"focus", Some(noop), 0);
    args.rval().set(ObjectValue(stub));
    true
}

unsafe fn new_storage_object(cx: &mut JSContext) -> *mut JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(cx, obj_root.handle(), c"getItem", Some(storage_get_item), 1);
    define_fn(cx, obj_root.handle(), c"setItem", Some(storage_set_item), 2);
    define_fn(cx, obj_root.handle(), c"removeItem", Some(noop), 1);
    define_fn(cx, obj_root.handle(), c"clear", Some(noop), 0);
    define_fn(cx, obj_root.handle(), c"key", Some(storage_key_null), 1);
    set_prop_i32(cx, obj_root.handle(), c"length", 0);
    obj
}

unsafe extern "C" fn storage_get_item(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn storage_set_item(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn storage_key_null(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn custom_event_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let type_str = arg_to_string(&mut cx, &args, 0);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(&mut cx, obj_root.handle(), c"type", &type_str);
    set_prop_bool(&mut cx, obj_root.handle(), c"bubbles", false);
    set_prop_bool(&mut cx, obj_root.handle(), c"cancelable", false);
    set_prop_bool(&mut cx, obj_root.handle(), c"defaultPrevented", false);
    define_fn(&mut cx, obj_root.handle(), c"preventDefault", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"stopPropagation", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"stopImmediatePropagation", Some(noop), 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn observer_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"observe", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"unobserve", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"disconnect", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"takeRecords", Some(return_empty_array), 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn return_empty_array(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let arr = wrappers2::NewArrayObject(&mut cx, &mozjs::jsapi::HandleValueArray::empty());
    args.rval().set(if arr.is_null() { UndefinedValue() } else { ObjectValue(arr) });
    true
}

unsafe extern "C" fn url_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let href = arg_to_string(&mut cx, &args, 0);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(&mut cx, obj_root.handle(), c"href", &href);
    set_prop_str(&mut cx, obj_root.handle(), c"origin", "http://localhost");
    set_prop_str(&mut cx, obj_root.handle(), c"protocol", "http:");
    set_prop_str(&mut cx, obj_root.handle(), c"host", "localhost");
    set_prop_str(&mut cx, obj_root.handle(), c"hostname", "localhost");
    set_prop_str(&mut cx, obj_root.handle(), c"port", "");
    set_prop_str(&mut cx, obj_root.handle(), c"pathname", "/");
    set_prop_str(&mut cx, obj_root.handle(), c"search", "");
    set_prop_str(&mut cx, obj_root.handle(), c"hash", "");
    define_fn(&mut cx, obj_root.handle(), c"toString", Some(url_to_string), 0);

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn url_to_string(cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);
    let js_str = new_js_string(&mut cx, "http://localhost/");
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval().set(mozjs::jsval::StringValue(&*js_str));
    }
    true
}

unsafe extern "C" fn url_search_params_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"get", Some(storage_get_item), 1);
    define_fn(&mut cx, obj_root.handle(), c"set", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"append", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"delete", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"has", Some(confirm_false), 1);
    define_fn(&mut cx, obj_root.handle(), c"toString", Some(return_empty_string), 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn abort_controller_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let signal = new_plain_object(&mut cx);
    rooted!(&in(cx) let sig_root = signal);
    set_prop_bool(&mut cx, sig_root.handle(), c"aborted", false);
    define_fn(&mut cx, sig_root.handle(), c"addEventListener", Some(noop), 2);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_obj(&mut cx, obj_root.handle(), c"signal", signal);
    define_fn(&mut cx, obj_root.handle(), c"abort", Some(noop), 0);

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn headers_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"get", Some(storage_get_item), 1);
    define_fn(&mut cx, obj_root.handle(), c"set", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"append", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"delete", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"has", Some(confirm_false), 1);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn noop_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn blob_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_i32(&mut cx, obj_root.handle(), c"size", 0);
    set_prop_str(&mut cx, obj_root.handle(), c"type", "");
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn fetch_stub(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    // We don't have async, so return a minimal thenable that always rejects
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"then", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"catch", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"finally", Some(noop), 1);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn xhr_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_i32(&mut cx, obj_root.handle(), c"status", 0);
    set_prop_i32(&mut cx, obj_root.handle(), c"readyState", 0);
    set_prop_str(&mut cx, obj_root.handle(), c"responseText", "");
    set_prop_str(&mut cx, obj_root.handle(), c"responseURL", "");
    set_prop_str(&mut cx, obj_root.handle(), c"statusText", "");
    define_fn(&mut cx, obj_root.handle(), c"open", Some(noop), 5);
    define_fn(&mut cx, obj_root.handle(), c"send", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"abort", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"setRequestHeader", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"getResponseHeader", Some(storage_get_item), 1);
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeEventListener", Some(noop), 2);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn websocket_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_i32(&mut cx, obj_root.handle(), c"readyState", 3); // CLOSED
    define_fn(&mut cx, obj_root.handle(), c"send", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"close", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    args.rval().set(ObjectValue(obj));
    true
}
