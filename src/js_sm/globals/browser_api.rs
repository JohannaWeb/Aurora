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
    define_fn(cx, global, c"__shady_native_addEventListener", Some(noop), 2);
    define_fn(cx, global, c"__shady_native_removeEventListener", Some(noop), 2);
    define_fn(cx, global, c"__shady_native_dispatchEvent", Some(return_true), 1);

    install_promise_stub(cx, global);

    let node_filter = new_plain_object(cx);
    rooted!(&in(cx) let node_filter_root = node_filter);
    for (name, val) in [
        (c"FILTER_ACCEPT", 1),
        (c"FILTER_REJECT", 2),
        (c"FILTER_SKIP", 3),
        (c"SHOW_ALL", -1),
        (c"SHOW_ELEMENT", 1),
        (c"SHOW_ATTRIBUTE", 2),
        (c"SHOW_TEXT", 4),
        (c"SHOW_CDATA_SECTION", 8),
        (c"SHOW_ENTITY_REFERENCE", 16),
        (c"SHOW_ENTITY", 32),
        (c"SHOW_PROCESSING_INSTRUCTION", 64),
        (c"SHOW_COMMENT", 128),
        (c"SHOW_DOCUMENT", 256),
        (c"SHOW_DOCUMENT_TYPE", 512),
        (c"SHOW_DOCUMENT_FRAGMENT", 1024),
        (c"SHOW_NOTATION", 2048),
    ] {
        set_prop_i32(cx, node_filter_root.handle(), name, val);
    }
    set_prop_obj(cx, global, c"NodeFilter", node_filter);

    // Storage — install minimal localStorage / sessionStorage
    let ls = new_storage_object(cx);
    set_prop_obj(cx, global, c"localStorage", ls);
    let ss = new_storage_object(cx);
    set_prop_obj(cx, global, c"sessionStorage", ss);

    // Minimal event target stubs for CustomEvent / Event constructors.
    // Real constructors carry a `.prototype` object — sites polyfill via
    // `Event.prototype.foo = ...`, which throws on a bare define_fn function.
    for name in &[
        c"CustomEvent",
        c"Event",
        c"MouseEvent",
        c"KeyboardEvent",
        c"FocusEvent",
        c"InputEvent",
        c"UIEvent",
        c"TouchEvent",
        c"WheelEvent",
        c"PointerEvent",
    ] {
        let proto = define_ctor_with_prototype(cx, global, name, Some(custom_event_ctor), 2);
        rooted!(&in(cx) let proto_root = proto);
        set_prop_str(cx, proto_root.handle(), c"type", "");
        set_prop_bool(cx, proto_root.handle(), c"bubbles", false);
        set_prop_bool(cx, proto_root.handle(), c"cancelable", false);
        set_prop_bool(cx, proto_root.handle(), c"defaultPrevented", false);
        define_fn(cx, proto_root.handle(), c"initEvent", Some(init_event), 3);
        define_fn(cx, proto_root.handle(), c"initCustomEvent", Some(init_custom_event), 4);
        define_fn(cx, proto_root.handle(), c"initMouseEvent", Some(init_mouse_event), 15);
        define_fn(cx, proto_root.handle(), c"preventDefault", Some(noop), 0);
        define_fn(cx, proto_root.handle(), c"stopPropagation", Some(noop), 0);
        define_fn(cx, proto_root.handle(), c"stopImmediatePropagation", Some(noop), 0);
    }

    install_dom_constructor_prototypes(cx, global);

    // Image — `new Image(width, height)` returns an <img>-like object
    define_ctor(cx, global, c"Image", Some(image_ctor), 2);

    // ResizeObserver / MutationObserver / IntersectionObserver stubs
    for name in &[
        c"MutationObserver",
        c"IntersectionObserver",
        c"ResizeObserver",
        c"PerformanceObserver",
    ] {
        define_ctor(cx, global, name, Some(observer_ctor), 1);
    }

    // URL constructor stub
    define_ctor(cx, global, c"URL", Some(url_ctor), 2);
    // URLSearchParams stub
    define_ctor(cx, global, c"URLSearchParams", Some(url_search_params_ctor), 1);
    install_abort_signal(cx, global);
    // AbortController stub
    define_ctor(cx, global, c"AbortController", Some(abort_controller_ctor), 0);
    // Headers stub
    define_ctor(cx, global, c"Headers", Some(headers_ctor), 1);
    // FormData stub
    define_ctor(cx, global, c"FormData", Some(noop_ctor), 1);

    // Blob / File stubs
    define_ctor(cx, global, c"Blob", Some(blob_ctor), 2);
    define_ctor(cx, global, c"File", Some(blob_ctor), 3);

    // Promise-based fetch — returns a rejected promise stub (sync fetch handled by __aurora_fetch_sync__)
    define_fn(cx, global, c"fetch", Some(fetch_stub), 1);

    // XHR constructor
    let xhr_proto = define_ctor_with_prototype(cx, global, c"XMLHttpRequest", Some(xhr_ctor), 0);
    rooted!(&in(cx) let xhr_proto_root = xhr_proto);
    install_event_target_methods(cx, xhr_proto_root.handle());
    install_xhr_methods(cx, xhr_proto_root.handle());

    // WebSocket constructor stub
    define_ctor(cx, global, c"WebSocket", Some(websocket_ctor), 2);

    // MessageChannel / MessagePort scheduler stubs
    define_ctor_with_prototype(cx, global, c"MessageChannel", Some(message_channel_ctor), 0);
    define_ctor_with_prototype(cx, global, c"MessageEvent", Some(custom_event_ctor), 2);

    let custom_elements = new_plain_object(cx);
    rooted!(&in(cx) let custom_elements_root = custom_elements);
    define_fn(cx, custom_elements_root.handle(), c"define", Some(noop), 2);
    define_fn(cx, custom_elements_root.handle(), c"get", Some(get_selection_null), 1);
    define_fn(cx, custom_elements_root.handle(), c"upgrade", Some(noop), 1);
    define_fn(cx, custom_elements_root.handle(), c"whenDefined", Some(resolved_promise_stub), 1);
    define_fn(
        cx,
        custom_elements_root.handle(),
        c"polyfillWrapFlushCallback",
        Some(call_first_callback),
        1,
    );
    set_prop_obj(cx, global, c"customElements", custom_elements);
}

// ── Impl ─────────────────────────────────────────────────────────────────────

unsafe fn install_abort_signal(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    let proto = define_ctor_with_prototype(cx, global, c"AbortSignal", Some(abort_signal_ctor), 0);
    rooted!(&in(cx) let proto_root = proto);
    install_event_target_methods(cx, proto_root.handle());

    rooted!(&in(cx) let mut ctor_val = UndefinedValue());
    if wrappers2::JS_GetProperty(cx, global, c"AbortSignal".as_ptr(), ctor_val.handle_mut())
        && ctor_val.get().is_object()
    {
        let ctor = ctor_val.get().to_object_or_null();
        rooted!(&in(cx) let ctor_root = ctor);
        define_fn(cx, ctor_root.handle(), c"abort", Some(abort_signal_ctor), 1);
        define_fn(cx, ctor_root.handle(), c"timeout", Some(abort_signal_ctor), 1);
        define_fn(cx, ctor_root.handle(), c"any", Some(abort_signal_ctor), 1);
    }
}

unsafe fn install_promise_stub(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    let proto = define_ctor_with_prototype(cx, global, c"Promise", Some(promise_ctor), 1);
    rooted!(&in(cx) let proto_root = proto);
    define_fn(cx, proto_root.handle(), c"then", Some(promise_then), 2);
    define_fn(cx, proto_root.handle(), c"catch", Some(promise_then), 1);
    define_fn(cx, proto_root.handle(), c"finally", Some(promise_then), 1);

    rooted!(&in(cx) let mut ctor_val = UndefinedValue());
    if wrappers2::JS_GetProperty(cx, global, c"Promise".as_ptr(), ctor_val.handle_mut())
        && ctor_val.get().is_object()
    {
        let ctor = ctor_val.get().to_object_or_null();
        rooted!(&in(cx) let ctor_root = ctor);
        define_fn(cx, ctor_root.handle(), c"resolve", Some(promise_static_resolve), 1);
        define_fn(cx, ctor_root.handle(), c"reject", Some(promise_static_resolve), 1);
        define_fn(cx, ctor_root.handle(), c"all", Some(promise_static_resolve), 1);
        define_fn(cx, ctor_root.handle(), c"race", Some(promise_static_resolve), 1);
        define_fn(cx, ctor_root.handle(), c"allSettled", Some(promise_static_resolve), 1);
        define_fn(cx, ctor_root.handle(), c"any", Some(promise_static_resolve), 1);
    }
}

unsafe fn install_dom_constructor_prototypes(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    // Modern bundles feature-detect and patch these constructor prototypes even
    // when Aurora only returns plain DOM wrapper objects internally.
    let event_target_proto =
        define_ctor_with_prototype(cx, global, c"EventTarget", Some(noop_ctor), 0);
    rooted!(&in(cx) let event_target_proto_root = event_target_proto);
    install_event_target_methods(cx, event_target_proto_root.handle());

    let node_proto = define_ctor_with_prototype(cx, global, c"Node", Some(noop_ctor), 0);
    rooted!(&in(cx) let node_proto_root = node_proto);
    install_event_target_methods(cx, node_proto_root.handle());
    install_node_methods(cx, node_proto_root.handle());
    set_prop_i32(cx, node_proto_root.handle(), c"ELEMENT_NODE", 1);
    set_prop_i32(cx, node_proto_root.handle(), c"TEXT_NODE", 3);
    set_prop_i32(cx, node_proto_root.handle(), c"DOCUMENT_NODE", 9);
    set_prop_i32(cx, node_proto_root.handle(), c"DOCUMENT_FRAGMENT_NODE", 11);

    let element_proto = define_ctor_with_prototype(cx, global, c"Element", Some(noop_ctor), 0);
    rooted!(&in(cx) let element_proto_root = element_proto);
    install_event_target_methods(cx, element_proto_root.handle());
    install_node_methods(cx, element_proto_root.handle());
    install_element_methods(cx, element_proto_root.handle());

    for name in &[
        c"SVGElement",
        c"SVGGraphicsElement",
        c"SVGSVGElement",
        c"SVGPathElement",
        c"SVGRectElement",
        c"SVGCircleElement",
        c"SVGUseElement",
        c"SVGDefsElement",
        c"SVGSymbolElement",
        c"SVGTextElement",
    ] {
        let proto = define_ctor_with_prototype(cx, global, name, Some(noop_ctor), 0);
        rooted!(&in(cx) let proto_root = proto);
        install_event_target_methods(cx, proto_root.handle());
        install_node_methods(cx, proto_root.handle());
        install_element_methods(cx, proto_root.handle());
    }

    for name in &[
        c"HTMLElement",
        c"HTMLAnchorElement",
        c"HTMLAudioElement",
        c"HTMLBodyElement",
        c"HTMLButtonElement",
        c"HTMLCanvasElement",
        c"HTMLDivElement",
        c"HTMLDocument",
        c"HTMLFormElement",
        c"HTMLHeadElement",
        c"HTMLHtmlElement",
        c"HTMLIFrameElement",
        c"HTMLImageElement",
        c"HTMLInputElement",
        c"HTMLLIElement",
        c"HTMLLinkElement",
        c"HTMLMediaElement",
        c"HTMLMetaElement",
        c"HTMLParagraphElement",
        c"HTMLScriptElement",
        c"HTMLSlotElement",
        c"HTMLSpanElement",
        c"HTMLStyleElement",
        c"HTMLTemplateElement",
        c"HTMLTextAreaElement",
        c"HTMLUListElement",
        c"HTMLUnknownElement",
        c"HTMLVideoElement",
        c"Document",
        c"DocumentFragment",
        c"DocumentType",
        c"CharacterData",
        c"Comment",
        c"CDATASection",
        c"ProcessingInstruction",
        c"Text",
        c"ShadowRoot",
        c"Range",
        c"Window",
    ] {
        let proto = define_ctor_with_prototype(cx, global, name, Some(noop_ctor), 0);
        rooted!(&in(cx) let proto_root = proto);
        install_event_target_methods(cx, proto_root.handle());
        install_node_methods(cx, proto_root.handle());
        install_element_methods(cx, proto_root.handle());
    }

    define_ctor_with_prototype(cx, global, c"DOMException", Some(dom_exception_ctor), 2);
}

unsafe fn install_event_target_methods(
    cx: &mut JSContext,
    proto: mozjs::gc::Handle<*mut JSObject>,
) {
    define_fn(cx, proto, c"addEventListener", Some(noop), 2);
    define_fn(cx, proto, c"removeEventListener", Some(noop), 2);
    define_fn(cx, proto, c"dispatchEvent", Some(return_true), 1);
}

unsafe fn install_node_methods(cx: &mut JSContext, proto: mozjs::gc::Handle<*mut JSObject>) {
    define_fn(cx, proto, c"appendChild", Some(node_return_first_arg), 1);
    define_fn(cx, proto, c"insertBefore", Some(node_return_first_arg), 2);
    define_fn(cx, proto, c"removeChild", Some(node_return_first_arg), 1);
    define_fn(cx, proto, c"replaceChild", Some(node_return_first_arg), 2);
    define_fn(cx, proto, c"cloneNode", Some(noop_ctor), 1);
    define_fn(cx, proto, c"contains", Some(confirm_false), 1);
    define_fn(cx, proto, c"normalize", Some(noop), 0);
}

unsafe fn install_element_methods(cx: &mut JSContext, proto: mozjs::gc::Handle<*mut JSObject>) {
    define_fn(cx, proto, c"getAttribute", Some(get_selection_null), 1);
    define_fn(cx, proto, c"setAttribute", Some(noop), 2);
    define_fn(cx, proto, c"removeAttribute", Some(noop), 1);
    define_fn(cx, proto, c"hasAttribute", Some(confirm_false), 1);
    define_fn(cx, proto, c"matches", Some(confirm_false), 1);
    define_fn(cx, proto, c"closest", Some(get_selection_null), 1);
    define_fn(cx, proto, c"querySelector", Some(get_selection_null), 1);
    define_fn(cx, proto, c"querySelectorAll", Some(return_empty_array), 1);
    define_fn(cx, proto, c"getBoundingClientRect", Some(get_bounding_client_rect), 0);
    define_fn(cx, proto, c"getClientRects", Some(return_empty_array), 0);
    define_fn(cx, proto, c"scrollIntoView", Some(noop), 1);
    define_fn(cx, proto, c"focus", Some(noop), 0);
    define_fn(cx, proto, c"blur", Some(noop), 0);
    define_fn(cx, proto, c"click", Some(noop), 0);
    define_fn(cx, proto, c"remove", Some(noop), 0);
    define_fn(cx, proto, c"append", Some(noop), 1);
    define_fn(cx, proto, c"prepend", Some(noop), 1);
    define_fn(cx, proto, c"before", Some(noop), 1);
    define_fn(cx, proto, c"after", Some(noop), 1);
}

unsafe fn install_xhr_methods(cx: &mut JSContext, proto: mozjs::gc::Handle<*mut JSObject>) {
    set_prop_i32(cx, proto, c"status", 0);
    set_prop_i32(cx, proto, c"readyState", 0);
    set_prop_str(cx, proto, c"responseText", "");
    set_prop_str(cx, proto, c"responseURL", "");
    set_prop_str(cx, proto, c"statusText", "");
    define_fn(cx, proto, c"open", Some(noop), 5);
    define_fn(cx, proto, c"send", Some(noop), 1);
    define_fn(cx, proto, c"abort", Some(noop), 0);
    define_fn(cx, proto, c"setRequestHeader", Some(noop), 2);
    define_fn(cx, proto, c"getResponseHeader", Some(storage_get_item), 1);
}

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

unsafe extern "C" fn return_true(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(true));
    true
}

unsafe extern "C" fn prompt_null(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn node_return_first_arg(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    if argc > 0 {
        args.rval().set(args.get(0).get());
    } else {
        args.rval().set(NullValue());
    }
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
    set_prop_str(&mut cx, obj_root.handle(), c"fontSize", "16px");
    set_prop_str(&mut cx, obj_root.handle(), c"fontFamily", "Arial, sans-serif");
    set_prop_str(&mut cx, obj_root.handle(), c"display", "block");
    set_prop_str(&mut cx, obj_root.handle(), c"position", "static");
    set_prop_str(&mut cx, obj_root.handle(), c"visibility", "visible");
    set_prop_str(&mut cx, obj_root.handle(), c"opacity", "1");

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

unsafe fn new_promise_like(cx: &mut JSContext) -> *mut JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(cx, obj_root.handle(), c"then", Some(promise_then), 2);
    define_fn(cx, obj_root.handle(), c"catch", Some(promise_then), 1);
    define_fn(cx, obj_root.handle(), c"finally", Some(promise_then), 1);
    obj
}

unsafe extern "C" fn promise_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(ObjectValue(new_promise_like(&mut cx)));
    true
}

unsafe extern "C" fn promise_static_resolve(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    promise_ctor(cx, argc, vp)
}

unsafe extern "C" fn promise_then(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(ObjectValue(new_promise_like(&mut cx)));
    true
}

unsafe fn new_message_port(cx: &mut JSContext) -> *mut JSObject {
    let port = new_plain_object(cx);
    rooted!(&in(cx) let port_root = port);
    define_fn(cx, port_root.handle(), c"postMessage", Some(noop), 1);
    define_fn(cx, port_root.handle(), c"start", Some(noop), 0);
    define_fn(cx, port_root.handle(), c"close", Some(noop), 0);
    define_fn(cx, port_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(cx, port_root.handle(), c"removeEventListener", Some(noop), 2);
    set_prop_null(cx, port_root.handle(), c"onmessage");
    port
}

unsafe extern "C" fn message_channel_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let channel = new_plain_object(&mut cx);
    rooted!(&in(cx) let channel_root = channel);
    let port1 = new_message_port(&mut cx);
    let port2 = new_message_port(&mut cx);
    set_prop_obj(&mut cx, channel_root.handle(), c"port1", port1);
    set_prop_obj(&mut cx, channel_root.handle(), c"port2", port2);
    args.rval().set(ObjectValue(channel));
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

unsafe extern "C" fn abort_signal_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_bool(&mut cx, obj_root.handle(), c"aborted", false);
    set_prop_null(&mut cx, obj_root.handle(), c"reason");
    set_prop_null(&mut cx, obj_root.handle(), c"onabort");
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"dispatchEvent", Some(return_true), 1);
    define_fn(&mut cx, obj_root.handle(), c"throwIfAborted", Some(noop), 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn resolved_promise_stub(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let thenable = new_plain_object(&mut cx);
    rooted!(&in(cx) let thenable_root = thenable);
    define_fn(&mut cx, thenable_root.handle(), c"then", Some(call_first_callback), 1);
    define_fn(&mut cx, thenable_root.handle(), c"catch", Some(noop), 1);
    define_fn(&mut cx, thenable_root.handle(), c"finally", Some(call_first_callback), 1);
    args.rval().set(ObjectValue(thenable));
    true
}

unsafe extern "C" fn call_first_callback(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc > 0 && args.get(0).get().is_object() {
        let callback = args.get(0).get();
        rooted!(&in(cx) let callback_root = callback);
        rooted!(&in(cx) let mut ignored = UndefinedValue());
        rooted!(&in(cx) let global = wrappers2::CurrentGlobalOrNull(&mut cx));
        let _ = wrappers2::JS_CallFunctionValue(
            &mut cx,
            global.handle(),
            callback_root.handle(),
            &mozjs::jsapi::HandleValueArray::empty(),
            ignored.handle_mut(),
        );
    }

    args.rval().set(args.thisv().get());
    true
}

unsafe extern "C" fn init_event(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    rooted!(&in(cx) let this_val = args.thisv().get());
    init_event_object(&mut cx, this_val.handle(), &args, false);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn init_custom_event(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    rooted!(&in(cx) let this_val = args.thisv().get());
    init_event_object(&mut cx, this_val.handle(), &args, true);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn init_mouse_event(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    rooted!(&in(cx) let this_val = args.thisv().get());
    init_event_object(&mut cx, this_val.handle(), &args, false);

    if this_val.get().is_object() {
        let obj = this_val.get().to_object_or_null();
        if !obj.is_null() {
            rooted!(&in(cx) let obj_root = obj);
            set_prop_f64(&mut cx, obj_root.handle(), c"screenX", arg_to_f64(&args, 5));
            set_prop_f64(&mut cx, obj_root.handle(), c"screenY", arg_to_f64(&args, 6));
            set_prop_f64(&mut cx, obj_root.handle(), c"clientX", arg_to_f64(&args, 7));
            set_prop_f64(&mut cx, obj_root.handle(), c"clientY", arg_to_f64(&args, 8));
            set_prop_bool(
                &mut cx,
                obj_root.handle(),
                c"ctrlKey",
                args.argc_ > 9 && args.get(9).get().to_boolean(),
            );
            set_prop_bool(
                &mut cx,
                obj_root.handle(),
                c"altKey",
                args.argc_ > 10 && args.get(10).get().to_boolean(),
            );
            set_prop_bool(
                &mut cx,
                obj_root.handle(),
                c"shiftKey",
                args.argc_ > 11 && args.get(11).get().to_boolean(),
            );
            set_prop_bool(
                &mut cx,
                obj_root.handle(),
                c"metaKey",
                args.argc_ > 12 && args.get(12).get().to_boolean(),
            );
            set_prop_f64(&mut cx, obj_root.handle(), c"button", arg_to_f64(&args, 13));
        }
    }

    args.rval().set(UndefinedValue());
    true
}

unsafe fn init_event_object(
    cx: &mut JSContext,
    this_val: mozjs::gc::Handle<Value>,
    args: &CallArgs,
    include_detail: bool,
) {
    if !this_val.get().is_object() {
        return;
    }

    let obj = this_val.get().to_object_or_null();
    if obj.is_null() {
        return;
    }
    rooted!(&in(cx) let obj_root = obj);

    let type_str = arg_to_string(cx, args, 0);
    set_prop_str(cx, obj_root.handle(), c"type", &type_str);
    set_prop_bool(cx, obj_root.handle(), c"bubbles", args.argc_ > 1 && args.get(1).get().to_boolean());
    set_prop_bool(
        cx,
        obj_root.handle(),
        c"cancelable",
        args.argc_ > 2 && args.get(2).get().to_boolean(),
    );
    if include_detail && args.argc_ > 3 {
        rooted!(&in(cx) let detail = args.get(3).get());
        wrappers2::JS_SetProperty(cx, obj_root.handle(), c"detail".as_ptr(), detail.handle());
    }
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
    set_prop_null(&mut cx, obj_root.handle(), c"detail");
    define_fn(&mut cx, obj_root.handle(), c"initEvent", Some(init_event), 3);
    define_fn(&mut cx, obj_root.handle(), c"initCustomEvent", Some(init_custom_event), 4);
    define_fn(&mut cx, obj_root.handle(), c"preventDefault", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"stopPropagation", Some(noop), 0);
    define_fn(&mut cx, obj_root.handle(), c"stopImmediatePropagation", Some(noop), 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn dom_exception_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let message = arg_to_string(&mut cx, &args, 0);
    let name = if argc > 1 {
        arg_to_string(&mut cx, &args, 1)
    } else {
        "Error".to_string()
    };

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(&mut cx, obj_root.handle(), c"message", &message);
    set_prop_str(&mut cx, obj_root.handle(), c"name", &name);
    set_prop_i32(&mut cx, obj_root.handle(), c"code", 0);
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn get_bounding_client_rect(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    for name in &[c"x", c"y", c"top", c"left", c"right", c"bottom", c"width", c"height"] {
        set_prop_f64(&mut cx, obj_root.handle(), name, 0.0);
    }
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn image_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let width = arg_to_f64(&args, 0);
    let height = arg_to_f64(&args, 1);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(&mut cx, obj_root.handle(), c"src", "");
    set_prop_str(&mut cx, obj_root.handle(), c"alt", "");
    set_prop_f64(&mut cx, obj_root.handle(), c"width", width);
    set_prop_f64(&mut cx, obj_root.handle(), c"height", height);
    set_prop_f64(&mut cx, obj_root.handle(), c"naturalWidth", width);
    set_prop_f64(&mut cx, obj_root.handle(), c"naturalHeight", height);
    set_prop_bool(&mut cx, obj_root.handle(), c"complete", true);
    set_prop_null(&mut cx, obj_root.handle(), c"onload");
    set_prop_null(&mut cx, obj_root.handle(), c"onerror");
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"setAttribute", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"getAttribute", Some(get_selection_null), 1);
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
    // We don't have async, so return a chainable promise-like that never settles.
    // Must support indefinite .then().catch().finally() chaining like a real Promise.
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(ObjectValue(new_promise_like(&mut cx)));
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
    define_fn(&mut cx, obj_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"removeEventListener", Some(noop), 2);
    install_xhr_methods(&mut cx, obj_root.handle());
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
