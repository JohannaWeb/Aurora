#![allow(unsafe_op_in_unsafe_fn)]
use std::ptr::NonNull;

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, JSObject, Value};
use mozjs::jsval::{BooleanValue, DoubleValue, NullValue, ObjectValue, StringValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::js_sm::mutation_observer;
use crate::js_sm::state::SmState;
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
    define_fn(
        cx,
        global,
        c"__shady_native_addEventListener",
        Some(noop),
        2,
    );
    define_fn(
        cx,
        global,
        c"__shady_native_removeEventListener",
        Some(noop),
        2,
    );
    define_fn(
        cx,
        global,
        c"__shady_native_dispatchEvent",
        Some(return_true),
        1,
    );
    set_prop_bool(
        cx,
        global,
        c"__aurora_debug_youtube__",
        debug_youtube_enabled(),
    );
    set_prop_bool(
        cx,
        global,
        c"__aurora_debug_youtube_verbose__",
        debug_youtube_verbose_enabled(),
    );

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

    // Storage — in-memory, per-origin localStorage / sessionStorage
    let ls = new_storage_object(cx, false);
    set_prop_obj(cx, global, c"localStorage", ls);
    let ss = new_storage_object(cx, true);
    set_prop_obj(cx, global, c"sessionStorage", ss);

    // Minimal event target stubs for the Event subtypes that
    // install_youtube_polyfills's JS-based Event/CustomEvent don't cover.
    // Real constructors carry a `.prototype` object — sites polyfill via
    // `Event.prototype.foo = ...`, which throws on a bare define_fn function.
    for name in &[
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
        define_fn(
            cx,
            proto_root.handle(),
            c"initCustomEvent",
            Some(init_custom_event),
            4,
        );
        define_fn(
            cx,
            proto_root.handle(),
            c"initMouseEvent",
            Some(init_mouse_event),
            15,
        );
        define_fn(cx, proto_root.handle(), c"preventDefault", Some(noop), 0);
        define_fn(cx, proto_root.handle(), c"stopPropagation", Some(noop), 0);
        define_fn(
            cx,
            proto_root.handle(),
            c"stopImmediatePropagation",
            Some(noop),
            0,
        );
    }

    install_dom_constructor_prototypes(cx, global);

    // Event / CustomEvent / Image / trustedTypes / customElements / CSS —
    // expressed as JS so constructors get real prototype chains (`instanceof`,
    // inheritance) that YouTube's bootstrap relies on. See install_youtube_polyfills.
    install_youtube_polyfills(cx, global);

    // ResizeObserver / IntersectionObserver stubs; MutationObserver is real.
    mutation_observer::install_mutation_observer(cx, global);
    define_ctor(cx, global, c"IntersectionObserver", Some(intersection_observer_ctor), 1);
    define_ctor(cx, global, c"ResizeObserver",       Some(resize_observer_ctor),       1);
    define_ctor(cx, global, c"PerformanceObserver",  Some(performance_observer_ctor),  1);

    // URL constructor stub
    define_ctor(cx, global, c"URL", Some(url_ctor), 2);
    // URLSearchParams stub
    define_ctor(
        cx,
        global,
        c"URLSearchParams",
        Some(url_search_params_ctor),
        1,
    );
    install_abort_signal(cx, global);
    // AbortController stub
    define_ctor(
        cx,
        global,
        c"AbortController",
        Some(abort_controller_ctor),
        0,
    );
    // Headers stub
    define_ctor(cx, global, c"Headers", Some(headers_ctor), 1);
    // FormData stub
    define_ctor(cx, global, c"FormData", Some(noop_ctor), 1);

    // Blob / File stubs
    define_ctor(cx, global, c"Blob", Some(blob_ctor), 2);
    define_ctor(cx, global, c"File", Some(blob_ctor), 3);

    // fetch — performs the request synchronously through __aurora_fetch_sync__
    // (already wired to the real HTTP client) and wraps the result in a real
    // Promise/Response, settled immediately. Real async would need a non-blocking
    // fetch path; this at least delivers data instead of hanging forever.
    install_fetch(cx, global);

    // XHR — like fetch, built in JS over __aurora_fetch_sync__ so it actually
    // delivers data (deferred to a microtask so listeners attached right after
    // `send()` — the standard pattern — are present when it fires).
    install_xhr(cx, global);

    // WebSocket — define_ctor sets JSFUN_CONSTRUCTOR so `new WebSocket(...)`
    // works (plain define_fn produces an uncallable-with-`new` function).
    define_ctor(cx, global, c"WebSocket", Some(websocket_ctor), 2);

    // MessageChannel / MessagePort scheduler stub. MessageEvent itself comes
    // from install_youtube_polyfills's JS Event/CustomEvent prototype chain.
    define_ctor_with_prototype(cx, global, c"MessageChannel", Some(message_channel_ctor), 0);

    // MediaSource / SourceBuffer / HTMLMediaElement surface — gives YouTube's
    // player a real (if non-functional-decode) object graph to drive instead
    // of crashing on `undefined.addSourceBuffer`. See install_media_polyfills.
    install_media_polyfills(cx, global);
}

// ── Impl ─────────────────────────────────────────────────────────────────────

unsafe fn install_abort_signal(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
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
        define_fn(
            cx,
            ctor_root.handle(),
            c"timeout",
            Some(abort_signal_ctor),
            1,
        );
        define_fn(cx, ctor_root.handle(), c"any", Some(abort_signal_ctor), 1);
    }
}

unsafe fn install_promise_stub(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
    eval_bootstrap(
        cx,
        global,
        c"promise",
        r#"
        (function() {
            function asap(fn) {
                queueMicrotask(fn);
            }

            function SimplePromise(executor) {
                if (!(this instanceof SimplePromise)) {
                    return new SimplePromise(executor);
                }

                var self = this;
                self._state = 'pending';
                self._value = undefined;
                self._handlers = [];

                function settle(state, value) {
                    if (self._state !== 'pending') return;
                    if (state === 'fulfilled' && value === self) {
                        state = 'rejected';
                        value = new TypeError('Promise resolved with itself');
                    }
                    if (state === 'fulfilled' && value && typeof value.then === 'function') {
                        try {
                            value.then(resolve, reject);
                        } catch (e) {
                            reject(e);
                        }
                        return;
                    }
                    self._state = state;
                    self._value = value;
                    asap(function() {
                        var handlers = self._handlers.splice(0);
                        for (var i = 0; i < handlers.length; i++) {
                            runHandler(self, handlers[i]);
                        }
                    });
                }

                function resolve(value) { settle('fulfilled', value); }
                function reject(reason) { settle('rejected', reason); }

                if (typeof executor === 'function') {
                    try {
                        executor(resolve, reject);
                    } catch (e) {
                        reject(e);
                    }
                } else {
                    resolve(executor);
                }
            }

            function runHandler(promise, handler) {
                var cb = promise._state === 'fulfilled' ? handler.onFulfilled : handler.onRejected;
                if (typeof cb !== 'function') {
                    (promise._state === 'fulfilled' ? handler.resolve : handler.reject)(promise._value);
                    return;
                }
                try {
                    handler.resolve(cb(promise._value));
                } catch (e) {
                    handler.reject(e);
                }
            }

            SimplePromise.prototype.then = function(onFulfilled, onRejected) {
                var parent = this;
                return new SimplePromise(function(resolve, reject) {
                    var handler = {
                        onFulfilled: onFulfilled,
                        onRejected: onRejected,
                        resolve: resolve,
                        reject: reject
                    };
                    if (parent._state === 'pending') {
                        parent._handlers.push(handler);
                    } else {
                        asap(function() { runHandler(parent, handler); });
                    }
                });
            };

            SimplePromise.prototype.catch = function(onRejected) {
                return this.then(undefined, onRejected);
            };

            SimplePromise.prototype.finally = function(onFinally) {
                return this.then(function(value) {
                    if (typeof onFinally === 'function') onFinally();
                    return value;
                }, function(reason) {
                    if (typeof onFinally === 'function') onFinally();
                    throw reason;
                });
            };

            SimplePromise.resolve = function(value) {
                return value instanceof SimplePromise ? value : new SimplePromise(function(resolve) {
                    resolve(value);
                });
            };

            SimplePromise.reject = function(reason) {
                return new SimplePromise(function(resolve, reject) {
                    reject(reason);
                });
            };

            SimplePromise.all = function(values) {
                return new SimplePromise(function(resolve, reject) {
                    values = Array.prototype.slice.call(values || []);
                    if (values.length === 0) {
                        resolve([]);
                        return;
                    }
                    var out = new Array(values.length);
                    var remaining = values.length;
                    values.forEach(function(value, index) {
                        SimplePromise.resolve(value).then(function(resolved) {
                            out[index] = resolved;
                            remaining -= 1;
                            if (remaining === 0) resolve(out);
                        }, reject);
                    });
                });
            };

            SimplePromise.race = function(values) {
                return new SimplePromise(function(resolve, reject) {
                    Array.prototype.slice.call(values || []).forEach(function(value) {
                        SimplePromise.resolve(value).then(resolve, reject);
                    });
                });
            };

            SimplePromise.allSettled = function(values) {
                values = Array.prototype.slice.call(values || []);
                return SimplePromise.all(values.map(function(value) {
                    return SimplePromise.resolve(value).then(function(resolved) {
                        return { status: 'fulfilled', value: resolved };
                    }, function(reason) {
                        return { status: 'rejected', reason: reason };
                    });
                }));
            };

            SimplePromise.any = function(values) {
                values = Array.prototype.slice.call(values || []);
                return new SimplePromise(function(resolve, reject) {
                    if (values.length === 0) {
                        reject(new Error('No promises were resolved'));
                        return;
                    }
                    var remaining = values.length;
                    var errors = [];
                    values.forEach(function(value, index) {
                        SimplePromise.resolve(value).then(resolve, function(reason) {
                            errors[index] = reason;
                            remaining -= 1;
                            if (remaining === 0) reject(errors);
                        });
                    });
                });
            };

            // SimplePromise kept as a fallback reference but NOT assigned to
            // globalThis.Promise — SpiderMonkey's native Promise must be used so
            // that resolve/reject reactions go through jq_enqueue_promise_job and
            // are drained by RunJobs after each script. The JS polyfill bypassed
            // the job queue entirely, leaving q.pending always empty.
            globalThis._SimplePromise = SimplePromise;
        })();
    "#,
    );
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
    define_fn(
        cx,
        proto,
        c"getBoundingClientRect",
        Some(get_bounding_client_rect),
        0,
    );
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

unsafe extern "C" fn node_return_first_arg(
    _cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
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
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"addEventListener",
        Some(noop),
        2,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"removeEventListener",
        Some(noop),
        2,
    );

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn get_computed_style(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"getPropertyValue",
        Some(return_empty_string),
        1,
    );
    define_fn(&mut cx, obj_root.handle(), c"setProperty", Some(noop), 2);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"removeProperty",
        Some(return_empty_string),
        1,
    );
    set_prop_str(&mut cx, obj_root.handle(), c"fontSize", "16px");
    set_prop_str(
        &mut cx,
        obj_root.handle(),
        c"fontFamily",
        "Arial, sans-serif",
    );
    set_prop_str(&mut cx, obj_root.handle(), c"display", "block");
    set_prop_str(&mut cx, obj_root.handle(), c"position", "static");
    set_prop_str(&mut cx, obj_root.handle(), c"visibility", "visible");
    set_prop_str(&mut cx, obj_root.handle(), c"opacity", "1");

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn return_empty_string(
    cx: *mut RawJSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);
    let js_str = new_js_string(&mut cx, "");
    if js_str.is_null() {
        args.rval().set(UndefinedValue());
    } else {
        args.rval()
            .set(mozjs::jsval::StringValue(unsafe { &*js_str }));
    }
    true
}

/// `window.getSelection()` — YouTube reads `.anchorNode`/`.focusNode`/etc.
/// straight off the result without a null check, so returning `null` throws.
/// Return a minimal empty-selection object instead.
unsafe extern "C" fn get_selection_null(cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_null(&mut cx, obj_root.handle(), c"anchorNode");
    set_prop_null(&mut cx, obj_root.handle(), c"focusNode");
    set_prop_i32(&mut cx, obj_root.handle(), c"anchorOffset", 0);
    set_prop_i32(&mut cx, obj_root.handle(), c"focusOffset", 0);
    set_prop_i32(&mut cx, obj_root.handle(), c"rangeCount", 0);
    set_prop_bool(&mut cx, obj_root.handle(), c"isCollapsed", true);
    set_prop_str(&mut cx, obj_root.handle(), c"type", "None");
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"removeAllRanges",
        Some(noop),
        0,
    );
    define_fn(&mut cx, obj_root.handle(), c"collapse", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"addRange", Some(noop), 1);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"getRangeAt",
        Some(get_selection_null_returner),
        1,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"toString",
        Some(empty_string_returner),
        0,
    );

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn get_selection_null_returner(
    _cx: *mut RawJSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn empty_string_returner(
    cx: *mut RawJSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);
    let js_str = new_js_string(&mut cx, "");
    args.rval().set(if js_str.is_null() {
        UndefinedValue()
    } else {
        mozjs::jsval::StringValue(&*js_str)
    });
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
    let cleaned: String = encoded
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
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
        out.push(if chunk.len() > 1 {
            TABLE[((b1 & 15) << 2 | b2 >> 6) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(b2 & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> String {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let enc = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0usize;
        while i < enc.len() {
            t[enc[i] as usize] = i as i8;
            i += 1;
        }
        t
    };
    let bytes: Vec<u8> = s.bytes().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = TABLE[bytes[i] as usize];
        let b = TABLE[bytes[i + 1] as usize];
        let c = TABLE[bytes[i + 2] as usize];
        let d = TABLE[bytes[i + 3] as usize];
        if a < 0 || b < 0 {
            break;
        }
        out.push(((a as u8) << 2) | ((b as u8) >> 4));
        if c >= 0 {
            out.push(((b as u8) << 4) | ((c as u8) >> 2));
        }
        if d >= 0 {
            out.push(((c as u8) << 6) | d as u8);
        }
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

unsafe fn new_message_port(cx: &mut JSContext) -> *mut JSObject {
    let port = new_plain_object(cx);
    rooted!(&in(cx) let port_root = port);
    define_fn(cx, port_root.handle(), c"postMessage", Some(noop), 1);
    define_fn(cx, port_root.handle(), c"start", Some(noop), 0);
    define_fn(cx, port_root.handle(), c"close", Some(noop), 0);
    define_fn(cx, port_root.handle(), c"addEventListener", Some(noop), 2);
    define_fn(
        cx,
        port_root.handle(),
        c"removeEventListener",
        Some(noop),
        2,
    );
    set_prop_null(cx, port_root.handle(), c"onmessage");
    port
}

unsafe extern "C" fn message_channel_ctor(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
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

unsafe fn new_storage_object(cx: &mut JSContext, is_session: bool) -> *mut JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_bool(cx, obj_root.handle(), c"__storage_session__", is_session);
    define_fn(cx, obj_root.handle(), c"getItem", Some(storage_get_item), 1);
    define_fn(cx, obj_root.handle(), c"setItem", Some(storage_set_item), 2);
    define_fn(cx, obj_root.handle(), c"removeItem", Some(storage_remove_item), 1);
    define_fn(cx, obj_root.handle(), c"clear", Some(storage_clear), 0);
    define_fn(cx, obj_root.handle(), c"key", Some(storage_key), 1);
    define_getter(cx, obj_root.handle(), c"length", Some(storage_length_getter));
    obj
}

/// Resolve `this` (a localStorage/sessionStorage object) to its backing
/// per-origin map based on the `__storage_session__` flag set at creation.
unsafe fn storage_map<'a>(
    cx: &mut JSContext,
    this: Value,
    state: &'a mut SmState,
) -> &'a mut std::collections::BTreeMap<String, String> {
    let is_session = if this.is_object() {
        let obj = this.to_object_or_null();
        rooted!(&in(cx) let obj_root = obj);
        get_prop_bool(cx, obj_root.handle(), c"__storage_session__")
    } else {
        false
    };
    if is_session {
        &mut state.window.session_storage
    } else {
        &mut state.window.local_storage
    }
}

unsafe extern "C" fn storage_get_item(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let key = arg_to_string(&mut cx, &args, 0);
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    let value = storage_map(&mut cx, this, state).get(&key).cloned();
    match value {
        Some(value) => {
            let js_str = new_js_string(&mut cx, &value);
            if js_str.is_null() {
                args.rval().set(NullValue());
            } else {
                args.rval().set(StringValue(&*js_str));
            }
        }
        None => args.rval().set(NullValue()),
    }
    true
}

unsafe extern "C" fn storage_set_item(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let key = arg_to_string(&mut cx, &args, 0);
    let value = arg_to_string(&mut cx, &args, 1);
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    storage_map(&mut cx, this, state).insert(key, value);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn storage_remove_item(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let key = arg_to_string(&mut cx, &args, 0);
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    storage_map(&mut cx, this, state).remove(&key);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn storage_clear(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    storage_map(&mut cx, this, state).clear();
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn storage_key(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let idx = arg_to_f64(&args, 0) as usize;
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    let key = storage_map(&mut cx, this, state).keys().nth(idx).cloned();
    match key {
        Some(key) => {
            let js_str = new_js_string(&mut cx, &key);
            if js_str.is_null() {
                args.rval().set(NullValue());
            } else {
                args.rval().set(StringValue(&*js_str));
            }
        }
        None => args.rval().set(NullValue()),
    }
    true
}

unsafe extern "C" fn storage_length_getter(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let this = args.thisv().get();
    let state = &mut *get_state_ptr(&cx);
    let len = storage_map(&mut cx, this, state).len();
    args.rval().set(DoubleValue(len as f64));
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
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"addEventListener",
        Some(noop),
        2,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"removeEventListener",
        Some(noop),
        2,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"dispatchEvent",
        Some(return_true),
        1,
    );
    define_fn(&mut cx, obj_root.handle(), c"throwIfAborted", Some(noop), 0);
    args.rval().set(ObjectValue(obj));
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
    set_prop_bool(
        cx,
        obj_root.handle(),
        c"bubbles",
        args.argc_ > 1 && args.get(1).get().to_boolean(),
    );
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
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"initEvent",
        Some(init_event),
        3,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"initCustomEvent",
        Some(init_custom_event),
        4,
    );
    define_fn(&mut cx, obj_root.handle(), c"preventDefault", Some(noop), 0);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"stopPropagation",
        Some(noop),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"stopImmediatePropagation",
        Some(noop),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

/// Polyfills YouTube's bootstrap needs that are far simpler to express as JS
/// than to build through raw JSAPI: real prototype chains for Event/CustomEvent
/// (so `instanceof` and subclassing work), a Trusted Types stub (YouTube wraps
/// all HTML/script sinks through it), and a customElements registry (YouTube
/// defines dozens of custom elements at startup and awaits `whenDefined`).
unsafe fn install_youtube_polyfills(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
    eval_bootstrap(
        cx,
        global,
        c"event-constructors",
        r#"
        (function() {
            globalThis.Image = function Image(width, height) {
                this.src = ''; this.width = width || 0; this.height = height || 0;
                this.naturalWidth = 0; this.naturalHeight = 0; this.complete = false;
                this.onload = null; this.onerror = null; this.crossOrigin = null;
                this.decoding = 'auto'; this.loading = 'eager';
                this.addEventListener = function(){}; this.removeEventListener = function(){};
                this.decode = function(){ return Promise.resolve(); };
            };
            globalThis.Image.prototype = Object.create(
                (typeof HTMLImageElement !== 'undefined') ? HTMLImageElement.prototype : Object.prototype
            );

            globalThis.Event = function Event(type, init) {
                var obj = (this instanceof Event) ? this : {};
                init = init || {};
                obj.type = type || '';
                obj.bubbles = !!(init.bubbles);
                obj.cancelable = !!(init.cancelable);
                obj.defaultPrevented = false;
                obj.isTrusted = false;
                obj.timeStamp = 0;
                obj.target = null; obj.currentTarget = null;
                obj.stopPropagation = function(){};
                obj.stopImmediatePropagation = function(){};
                obj.preventDefault = function(){ obj.defaultPrevented = true; };
                obj.composedPath = function(){ return []; };
                if (!(this instanceof Event)) return obj;
            };

            globalThis.CustomEvent = function CustomEvent(type, init) {
                globalThis.Event.call(this, type, init);
                this.detail = (init && init.detail !== undefined) ? init.detail : null;
            };
            globalThis.CustomEvent.prototype = Object.create(globalThis.Event.prototype);
            globalThis.CustomEvent.prototype.constructor = globalThis.CustomEvent;

            globalThis.ErrorEvent = function ErrorEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.message = init.message || ''; this.error = init.error || null;
            };
            globalThis.ErrorEvent.prototype = Object.create(globalThis.Event.prototype);

            globalThis.MessageEvent = function MessageEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.data = init.data !== undefined ? init.data : null;
                this.origin = init.origin || ''; this.source = init.source || null;
            };
            globalThis.MessageEvent.prototype = Object.create(globalThis.Event.prototype);

            globalThis.PromiseRejectionEvent = function PromiseRejectionEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.promise = init.promise || null; this.reason = init.reason;
            };
            globalThis.PromiseRejectionEvent.prototype = Object.create(globalThis.Event.prototype);
        })();
    "#,
    );

    eval_bootstrap(
        cx,
        global,
        c"trusted-types",
        r#"
        (function() {
            function makeTrusted(val) { return { toString: function(){ return val; } }; }
            globalThis.trustedTypes = {
                createPolicy: function(name, rules) {
                    return {
                        name: name,
                        createHTML: function(s) { return makeTrusted(rules && rules.createHTML ? rules.createHTML(s) : s); },
                        createScript: function(s) { return makeTrusted(rules && rules.createScript ? rules.createScript(s) : s); },
                        createScriptURL: function(s) { return makeTrusted(rules && rules.createScriptURL ? rules.createScriptURL(s) : s); }
                    };
                },
                getAttributeType: function() { return null; },
                getPropertyType: function() { return null; },
                isHTML: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScript: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScriptURL: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                emptyHTML: makeTrusted(''),
                emptyScript: makeTrusted(''),
                defaultPolicy: null
            };
        })();
    "#,
    );

    eval_bootstrap(
        cx,
        global,
        c"custom-elements",
        r#"
        (function() {
            var registry = {};
            var patchedCreateElement = false;
            function trace(msg) {
                console.log('[yt-life] ' + msg);
            }
            function shouldTraceName(name) {
                return true;
            }
            function traceError(where, error) {
                var message = error && (error.name || 'Error') + ': ' + (error.message || '');
                var stack = error && error.stack ? ('\n' + error.stack) : '';
                console.log('[yt-life] ERROR ' + where + ': ' + (message || String(error)) + stack);
            }

            // Upgrade: swap the plain stub element's prototype to the
            // registered class/constructor's prototype and run it bound to
            // the element, then fire connectedCallback. This is exactly what
            // function-style definitions (`function MyEl(){...}`) expect.
            // ES6 `class X extends HTMLElement` constructors throw "class
            // constructor cannot be invoked without 'new'" when called this
            // way. Keep the prototype swap and still fire connectedCallback;
            // most framework element work happens there, and skipping it
            // leaves upgraded nodes inert.
            function tryUpgrade(el, connect) {
                if (!el || el.nodeType !== 1 || el.__ce_upgraded__) return;
                var name = el.localName || (el.tagName ? el.tagName.toLowerCase() : '');
                var ctor = registry[name];
                if (!ctor) return;
                el.__ce_upgraded__ = true;
                if (shouldTraceName(name)) trace('upgrade ' + name + ' connect=' + (connect !== false));
                try {
                    Object.setPrototypeOf(el, ctor.prototype);
                    var hadObjectInitializeProperties =
                        Object.prototype.hasOwnProperty.call(Object.prototype, '_initializeProperties');
                    var oldObjectInitializeProperties = Object.prototype._initializeProperties;
                    if (typeof el._initializeProperties !== 'function') {
                        try {
                            Object.defineProperty(el, '_initializeProperties', {
                                value: function(){},
                                configurable: true,
                                writable: true
                            });
                        } catch (e) {
                            el._initializeProperties = function(){};
                        }
                    }
                    if (typeof Object.prototype._initializeProperties !== 'function') {
                        Object.defineProperty(Object.prototype, '_initializeProperties', {
                            value: function(){},
                            configurable: true,
                            writable: true
                        });
                    }
                    var oldHTMLElement = globalThis.HTMLElement;
                    if (typeof oldHTMLElement === 'function') {
                        var HTMLElementDuringUpgrade = function HTMLElement() {};
                        HTMLElementDuringUpgrade.prototype = oldHTMLElement.prototype;
                        try { Object.setPrototypeOf(HTMLElementDuringUpgrade, oldHTMLElement); } catch (e) {}
                        globalThis.HTMLElement = HTMLElementDuringUpgrade;
                    }
                    try {
                        ctor.call(el);
                    } finally {
                        if (typeof oldHTMLElement === 'function') {
                            globalThis.HTMLElement = oldHTMLElement;
                        }
                        if (hadObjectInitializeProperties) {
                            Object.prototype._initializeProperties = oldObjectInitializeProperties;
                        } else {
                            delete Object.prototype._initializeProperties;
                        }
                    }
                } catch (e) {
                    traceError('constructor ' + name, e);
                }
                if (connect !== false && typeof el.connectedCallback === 'function') {
                    try {
                        if (!el.__ce_connected__) {
                            el.__ce_connected__ = true;
                            if (shouldTraceName(name)) trace('connectedCallback ' + name);
                            el.connectedCallback();
                        }
                    } catch (e) { traceError('connectedCallback ' + name, e); }
                }
                if (globalThis.__aurora_debug_youtube__ && (name === 'ytd-app' || name === 'ytd-masthead')) {
                    try {
                        var tmpl;
                        try { tmpl = ctor.template; } catch (e) { tmpl = 'THREW:' + e.message; }
                        trace('probe ' + name +
                            ' ctor.template=' + (tmpl === undefined ? 'undefined' : tmpl === null ? 'null' : (typeof tmpl)) +
                            ' el._template=' + (typeof el._template) +
                            ' el.root=' + (typeof el.root) +
                            ' el.shadowRoot=' + (el.shadowRoot ? 'set' : String(el.shadowRoot)) +
                            ' kids=' + (el.children ? el.children.length : '?') +
                            ' dataEnabled=' + el.__dataEnabled +
                            ' dataReady=' + el.__dataReady +
                            ' ready=' + (typeof el.ready) +
                            ' stamp=' + (typeof el._stampTemplate) +
                            ' attachDom=' + (typeof el._attachDom));
                    } catch (e) { traceError('probe ' + name, e); }
                }
            }

            function upgradeTree(root) {
                if (!root) return;
                try {
                    tryUpgrade(root, true);
                    if (typeof root.querySelectorAll === 'function') {
                        var all = root.querySelectorAll('*');
                        for (var i = 0; i < all.length; i++) { tryUpgrade(all[i], true); }
                    }
                } catch (e) {}
            }

            // Newly created elements (`document.createElement('ytd-app')`)
            // need upgrading too — patch it in lazily once `document` exists
            // (it doesn't yet at globals-install time).
            function ensureCreateElementPatch() {
                if (patchedCreateElement) return;
                if (typeof document === 'undefined' || typeof document.createElement !== 'function') return;
                patchedCreateElement = true;
                var orig = document.createElement.bind(document);
                document.createElement = function(tagName, options) {
                    var el = orig(tagName, options);
                    if (String(tagName).indexOf('-') >= 0 && shouldTraceName(String(tagName))) trace('createElement ' + tagName);
                    tryUpgrade(el, false);
                    return el;
                };
            }

            globalThis.customElements = {
                define: function(name, ctor, opts) {
                    if (shouldTraceName(name)) trace('define ' + name);
                    registry[name] = ctor;
                    ensureCreateElementPatch();
                    if (typeof document !== 'undefined' && typeof document.querySelectorAll === 'function') {
                        try {
                            var existing = document.querySelectorAll(name);
                            for (var i = 0; i < existing.length; i++) { tryUpgrade(existing[i], true); }
                        } catch (e) { traceError('upgrade existing ' + name, e); }
                    }
                },
                get: function(name) { return registry[name]; },
                whenDefined: function(name) {
                    return registry[name] ? Promise.resolve(registry[name]) : new Promise(function(res) {
                        var orig = customElements.define;
                        customElements.define = function(n, c, o) {
                            orig.call(customElements, n, c, o);
                            if (n === name) res(c);
                        };
                    });
                },
                upgrade: function(root) { trace('customElements.upgrade'); upgradeTree(root); }
            };
        })();
    "#,
    );

    eval_bootstrap(
        cx,
        global,
        c"css-stub",
        r#"
        (function() {
            globalThis.CSS = {
                supports: function() { return false; },
                escape: function(s) { return String(s); }
            };
        })();
    "#,
    );
}

/// `MediaSource` / `SourceBuffer` / `HTMLMediaElement` surface for YouTube's
/// player. Aurora has no streaming demux/decode pipeline yet (see `src/media.rs`
/// — it decodes whole files upfront, nothing like MSE's incremental
/// `appendBuffer` model), so none of this actually feeds bytes to a decoder.
/// What it provides is *shape*: real constructors, prototype chains, event
/// ordering (`loadstart` → `loadedmetadata` → `canplay` → `playing` →
/// `timeupdate`/`seeked`, `sourceopen`, `updateend`, ...) and state machines
/// (`readyState`/`networkState`/`updating`) so player bootstrap code that
/// gates on these — which all of them do — proceeds instead of throwing on
/// `undefined.addSourceBuffer` or hanging forever waiting for an event that
/// never fires. Wiring this to real decoded frames/audio is the next step
/// once a streaming pipeline exists.
unsafe fn install_media_polyfills(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
    eval_bootstrap(
        cx,
        global,
        c"media-source",
        r#"
        (function() {
            if (typeof globalThis.DOMException === 'undefined') {
                globalThis.DOMException = function DOMException(message, name) {
                    var err = new Error(message || '');
                    err.name = name || 'Error';
                    err.code = 0;
                    return err;
                };
            }

            function TimeRanges(ranges) {
                this._ranges = ranges || [];
            }
            Object.defineProperty(TimeRanges.prototype, 'length', {
                get: function() { return this._ranges.length; }
            });
            TimeRanges.prototype.start = function(i) {
                if (!this._ranges[i]) throw new DOMException('Index out of range', 'IndexSizeError');
                return this._ranges[i][0];
            };
            TimeRanges.prototype.end = function(i) {
                if (!this._ranges[i]) throw new DOMException('Index out of range', 'IndexSizeError');
                return this._ranges[i][1];
            };
            globalThis.__aurora_TimeRanges__ = TimeRanges;

            function makeEventTarget(obj) {
                obj._listeners = {};
                obj.addEventListener = function(type, cb) {
                    if (typeof cb !== 'function') return;
                    (obj._listeners[type] = obj._listeners[type] || []).push(cb);
                };
                obj.removeEventListener = function(type, cb) {
                    var l = obj._listeners[type];
                    if (!l) return;
                    var i = l.indexOf(cb);
                    if (i >= 0) l.splice(i, 1);
                };
                obj._dispatch = function(type, init) {
                    var ev = Object.assign({ type: type, target: obj, currentTarget: obj,
                        bubbles: false, cancelable: false, defaultPrevented: false,
                        preventDefault: function(){}, stopPropagation: function(){},
                        stopImmediatePropagation: function(){} }, init || {});
                    var handler = obj['on' + type];
                    if (typeof handler === 'function') { try { handler.call(obj, ev); } catch (e) {} }
                    var l = obj._listeners[type];
                    if (l) { var copy = l.slice(); for (var i = 0; i < copy.length; i++) { try { copy[i].call(obj, ev); } catch (e) {} } }
                };
            }

            function SourceBuffer(mediaSource, mimeType) {
                makeEventTarget(this);
                this.mediaSource = mediaSource;
                this.mode = 'segments';
                this.updating = false;
                this.timestampOffset = 0;
                this.appendWindowStart = 0;
                this.appendWindowEnd = Infinity;
                this.trackDefaults = null;
                this._mimeType = mimeType || '';
                this._buffered = [];
            }
            Object.defineProperty(SourceBuffer.prototype, 'buffered', {
                get: function() { return new TimeRanges(this._buffered); }
            });
            function bufferAppendOp(self, fn) {
                if (self.updating) {
                    self._dispatch('error');
                    throw new DOMException('SourceBuffer is updating', 'InvalidStateError');
                }
                self.updating = true;
                self._dispatch('updatestart');
                queueMicrotask(function() {
                    try { fn(); } catch (e) {}
                    self.updating = false;
                    self._dispatch('update');
                    self._dispatch('updateend');
                    if (self.mediaSource) self.mediaSource._onBufferUpdated();
                });
            }
            SourceBuffer.prototype.appendBuffer = function(data) {
                var self = this;
                bufferAppendOp(self, function() {
                    var dur = self.mediaSource ? self.mediaSource._duration : NaN;
                    var lastEnd = self._buffered.length ? self._buffered[self._buffered.length - 1][1] : self.timestampOffset;
                    var end = (isFinite(dur) && dur > lastEnd) ? dur : (lastEnd + 10);
                    self._buffered.push([self.timestampOffset, end]);
                });
            };
            SourceBuffer.prototype.appendStream = SourceBuffer.prototype.appendBuffer;
            SourceBuffer.prototype.abort = function() {
                this.updating = false;
            };
            SourceBuffer.prototype.remove = function(start, end) {
                var self = this;
                bufferAppendOp(self, function() {
                    self._buffered = self._buffered
                        .map(function(r) {
                            if (end <= r[0] || start >= r[1]) return [r];
                            var parts = [];
                            if (start > r[0]) parts.push([r[0], start]);
                            if (end < r[1]) parts.push([end, r[1]]);
                            return parts;
                        })
                        .reduce(function(acc, parts) { return acc.concat(parts); }, []);
                });
            };
            SourceBuffer.prototype.changeType = function() {};

            function MediaSource() {
                makeEventTarget(this);
                this.readyState = 'closed';
                this.sourceBuffers = [];
                this.activeSourceBuffers = [];
                this._duration = NaN;
                this.__isMediaSource__ = true;
            }
            MediaSource.isTypeSupported = function(type) {
                // Be permissive: claiming support for the containers/codecs
                // YouTube actually serves lets the player pick a stream and
                // proceed, instead of bailing into "your browser can't play
                // this video" because nothing reports as supported.
                return typeof type === 'string' && /^(video|audio)\/(mp4|webm|ogg)/i.test(type);
            };
            Object.defineProperty(MediaSource.prototype, 'duration', {
                get: function() { return this._duration; },
                set: function(v) {
                    this._duration = v;
                    this._dispatch('durationchange');
                    if (this._mediaElement) this._mediaElement.__fireMediaEvent__('durationchange');
                }
            });
            MediaSource.prototype.addSourceBuffer = function(mimeType) {
                if (this.readyState !== 'open') {
                    throw new DOMException('MediaSource is not open', 'InvalidStateError');
                }
                var sb = new SourceBuffer(this, mimeType);
                this.sourceBuffers.push(sb);
                this.activeSourceBuffers.push(sb);
                return sb;
            };
            MediaSource.prototype.removeSourceBuffer = function(sb) {
                var i = this.sourceBuffers.indexOf(sb);
                if (i >= 0) this.sourceBuffers.splice(i, 1);
                i = this.activeSourceBuffers.indexOf(sb);
                if (i >= 0) this.activeSourceBuffers.splice(i, 1);
            };
            MediaSource.prototype.endOfStream = function(error) {
                this.readyState = 'ended';
                this._dispatch('sourceended');
                if (this._mediaElement) {
                    if (!error && isNaN(this._duration)) {
                        var maxEnd = 0;
                        this.sourceBuffers.forEach(function(sb) {
                            sb._buffered.forEach(function(r) { if (r[1] > maxEnd) maxEnd = r[1]; });
                        });
                        this.duration = maxEnd;
                    }
                    this._mediaElement._onSourceEnded();
                }
            };
            MediaSource.prototype.clearLiveSeekableRange = function() {};
            MediaSource.prototype.setLiveSeekableRange = function() {};
            MediaSource.prototype._open = function() {
                if (this.readyState !== 'closed') return;
                this.readyState = 'open';
                this._dispatch('sourceopen');
                if (this._mediaElement) this._mediaElement.__fireMediaEvent__('sourceopen');
            };
            MediaSource.prototype._onBufferUpdated = function() {
                if (this._mediaElement) this._mediaElement._onSourceBufferUpdated();
            };

            globalThis.MediaSource = MediaSource;
            globalThis.ManagedMediaSource = MediaSource;

            // `URL.createObjectURL(mediaSource)` is how player code attaches a
            // MediaSource to a <video>: it sets `video.src = URL.createObjectURL(ms)`.
            // We can't resolve that URL to real bytes, but we *can* recognize
            // that the object behind it is a MediaSource and wire it directly to
            // the element — see __aurora_install_media_element__'s `src` setter.
            if (typeof globalThis.URL === 'function') {
                var objectUrls = {};
                var urlCounter = 0;
                globalThis.URL.createObjectURL = function(obj) {
                    var url = 'blob:aurora://' + (++urlCounter);
                    objectUrls[url] = obj;
                    return url;
                };
                globalThis.URL.revokeObjectURL = function(url) {
                    delete objectUrls[url];
                };
                globalThis.__aurora_resolve_object_url__ = function(url) {
                    return Object.prototype.hasOwnProperty.call(objectUrls, url) ? objectUrls[url] : null;
                };
            }
        })();
    "#,
    );

    eval_bootstrap(
        cx,
        global,
        c"media-element",
        r#"
        (function() {
            // Decorates a freshly-created <video>/<audio> element object (a
            // plain object backed by a real DOM node — see create_js_node) with
            // HTMLMediaElement's state machine, properties and methods. Called
            // natively right after the element object is built.
            //
            // There's no real decoder feeding this — see install_media_polyfills'
            // doc comment — so playback is simulated: `play()` flips `paused`
            // and fires `playing`, a `setInterval` advances `currentTime` against
            // `duration` so progress-watching code (scrubbers, analytics, "next
            // video" triggers) sees plausible motion, and loading a `src` walks
            // through the spec's readiness events so gating code unblocks.
            globalThis.__aurora_install_media_element__ = function(el) {
                if (!el || el.__media_installed__) return;
                el.__media_installed__ = true;

                var listeners = {};
                var nativeAdd = el.addEventListener;
                var nativeRemove = el.removeEventListener;
                el.addEventListener = function(type, cb, opts) {
                    if (typeof cb === 'function') {
                        (listeners[type] = listeners[type] || []).push(cb);
                    }
                    try { nativeAdd.call(el, type, cb, opts); } catch (e) {}
                };
                el.removeEventListener = function(type, cb, opts) {
                    var l = listeners[type];
                    if (l) { var i = l.indexOf(cb); if (i >= 0) l.splice(i, 1); }
                    try { nativeRemove.call(el, type, cb, opts); } catch (e) {}
                };
                el.__fireMediaEvent__ = function(type) {
                    var ev = { type: type, target: el, currentTarget: el, bubbles: false,
                        cancelable: false, defaultPrevented: false, preventDefault: function(){},
                        stopPropagation: function(){}, stopImmediatePropagation: function(){} };
                    var handler = el['on' + type];
                    if (typeof handler === 'function') { try { handler.call(el, ev); } catch (e) {} }
                    var l = listeners[type];
                    if (l) { var copy = l.slice(); for (var i = 0; i < copy.length; i++) { try { copy[i].call(el, ev); } catch (e) {} } }
                };
                var fire = el.__fireMediaEvent__;

                el.HAVE_NOTHING = 0; el.HAVE_METADATA = 1; el.HAVE_CURRENT_DATA = 2;
                el.HAVE_FUTURE_DATA = 3; el.HAVE_ENOUGH_DATA = 4;
                el.NETWORK_EMPTY = 0; el.NETWORK_IDLE = 1; el.NETWORK_LOADING = 2; el.NETWORK_NO_SOURCE = 3;

                var s = {
                    duration: NaN, paused: true, ended: false, seeking: false,
                    readyState: 0, networkState: 0, volume: 1, muted: false,
                    defaultMuted: false, playbackRate: 1, defaultPlaybackRate: 1,
                    autoplay: false, loop: false, controls: false, preload: 'metadata',
                    crossOrigin: null, currentSrc: '', error: null, srcObject: null,
                    videoWidth: el.tagName === 'VIDEO' ? 640 : 0,
                    videoHeight: el.tagName === 'VIDEO' ? 360 : 0,
                    textTracks: [], _mediaSource: null, _timer: null, _currentTime: 0
                };

                Object.keys(s).forEach(function(key) {
                    if (key.charAt(0) === '_') return;
                    Object.defineProperty(el, key, {
                        get: function() { return s[key]; },
                        set: function(v) { s[key] = v; },
                        configurable: true, enumerable: true
                    });
                });

                Object.defineProperty(el, 'buffered', { get: function() { return new globalThis.__aurora_TimeRanges__([]); } });
                Object.defineProperty(el, 'played', { get: function() { return new globalThis.__aurora_TimeRanges__(s._currentTime > 0 ? [[0, s._currentTime]] : []); } });
                Object.defineProperty(el, 'seekable', {
                    get: function() { return new globalThis.__aurora_TimeRanges__(isFinite(s.duration) ? [[0, s.duration]] : []); }
                });

                function stopTicker() {
                    if (s._timer !== null) { clearInterval(s._timer); s._timer = null; }
                }
                function startTicker() {
                    stopTicker();
                    s._timer = setInterval(function() {
                        if (s.paused || s.ended) return;
                        s._currentTime += 0.25 * s.playbackRate;
                        if (isFinite(s.duration) && s._currentTime >= s.duration) {
                            s._currentTime = s.duration;
                            s.paused = true;
                            s.ended = true;
                            stopTicker();
                            fire('timeupdate');
                            fire('ended');
                            return;
                        }
                        fire('timeupdate');
                    }, 250);
                }

                function finishLoading() {
                    if (isNaN(s.duration)) s.duration = 0;
                    s.readyState = 1; fire('durationchange'); fire('loadedmetadata');
                    s.readyState = 2; fire('loadeddata');
                    s.readyState = 4; s.networkState = 1;
                    fire('progress'); fire('canplay'); fire('canplaythrough');
                    if (s.autoplay) { el.play(); }
                }

                function startLoading(srcUrl) {
                    if (!srcUrl) return;
                    stopTicker();
                    s._currentTime = 0; s.ended = false; s.readyState = 0;
                    s.networkState = 2; s.currentSrc = srcUrl;
                    fire('emptied'); fire('loadstart');

                    // `src` pointing at a MediaSource object URL: hand the
                    // element to the MediaSource and let *it* drive readiness
                    // via `sourceopen`/buffered ranges rather than faking a
                    // fixed timeline immediately.
                    var resolved = (typeof globalThis.__aurora_resolve_object_url__ === 'function')
                        ? globalThis.__aurora_resolve_object_url__(srcUrl) : null;
                    if (resolved && resolved.__isMediaSource__) {
                        s._mediaSource = resolved;
                        resolved._mediaElement = el;
                        queueMicrotask(function() { resolved._open(); });
                        return;
                    }

                    queueMicrotask(finishLoading);
                }

                el._onSourceBufferUpdated = function() {
                    // A SourceBuffer gained data: treat that as "enough to play",
                    // matching the spirit of `canplay`/`canplaythrough` gating.
                    if (s.readyState < 4) finishLoading();
                };
                el._onSourceEnded = function() {
                    if (isNaN(s.duration) && s._mediaSource) s.duration = s._mediaSource._duration;
                };

                var srcAttr = el.getAttribute ? el.getAttribute('src') : '';
                Object.defineProperty(el, 'src', {
                    get: function() { return s.currentSrc || srcAttr || ''; },
                    set: function(v) { srcAttr = String(v); startLoading(srcAttr); },
                    configurable: true, enumerable: true
                });
                Object.defineProperty(el, 'currentTime', {
                    get: function() { return s._currentTime; },
                    set: function(v) {
                        s._currentTime = Number(v) || 0;
                        s.seeking = true;
                        fire('seeking');
                        queueMicrotask(function() {
                            s.seeking = false;
                            fire('timeupdate');
                            fire('seeked');
                        });
                    },
                    configurable: true, enumerable: true
                });

                el.load = function() {
                    stopTicker();
                    s.readyState = 0; s.networkState = 0; s.error = null;
                    if (srcAttr) startLoading(srcAttr);
                };
                el.canPlayType = function(type) {
                    return (typeof type === 'string' && /^(video|audio)\/(mp4|webm|ogg)/i.test(type)) ? 'probably' : '';
                };
                el.play = function() {
                    if (!s.paused) return Promise.resolve();
                    s.paused = false; s.ended = false;
                    fire('play');
                    startTicker();
                    fire('playing');
                    return Promise.resolve();
                };
                el.pause = function() {
                    if (s.paused) return;
                    s.paused = true;
                    stopTicker();
                    fire('pause');
                };
                el.fastSeek = function(t) { el.currentTime = t; };
                el.addTextTrack = function(kind, label, lang) {
                    var track = { kind: kind || 'subtitles', label: label || '', language: lang || '',
                        mode: 'disabled', cues: [], activeCues: [],
                        addEventListener: function(){}, removeEventListener: function(){},
                        addCue: function(){}, removeCue: function(){} };
                    s.textTracks.push(track);
                    return track;
                };
                el.captureStream = function() {
                    return { getTracks: function(){ return []; }, getAudioTracks: function(){ return []; },
                        getVideoTracks: function(){ return []; }, addTrack: function(){}, removeTrack: function(){} };
                };

                if (srcAttr) startLoading(srcAttr);
            };
        })();
    "#,
    );
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

unsafe extern "C" fn get_bounding_client_rect(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    for name in &[
        c"x", c"y", c"top", c"left", c"right", c"bottom", c"width", c"height",
    ] {
        set_prop_f64(&mut cx, obj_root.handle(), name, 0.0);
    }
    args.rval().set(ObjectValue(obj));
    true
}

macro_rules! named_observer_ctor {
    ($fn_name:ident, $api:literal) => {
        unsafe extern "C" fn $fn_name(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
            crate::logging::track_missing_api($api);
            observer_ctor(cx, argc, vp)
        }
    };
}
named_observer_ctor!(intersection_observer_ctor, "IntersectionObserver");
named_observer_ctor!(resize_observer_ctor, "ResizeObserver");
named_observer_ctor!(performance_observer_ctor, "PerformanceObserver");

unsafe extern "C" fn observer_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"observe", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"unobserve", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"disconnect", Some(noop), 0);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"takeRecords",
        Some(return_empty_array),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn return_empty_array(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let arr = wrappers2::NewArrayObject(&mut cx, &mozjs::jsapi::HandleValueArray::empty());
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
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
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"toString",
        Some(url_to_string),
        0,
    );

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

unsafe extern "C" fn url_search_params_ctor(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"get",
        Some(prompt_null),
        1,
    );
    define_fn(&mut cx, obj_root.handle(), c"set", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"append", Some(noop), 2);
    define_fn(&mut cx, obj_root.handle(), c"delete", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"has", Some(confirm_false), 1);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"toString",
        Some(return_empty_string),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn abort_controller_ctor(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let signal = new_plain_object(&mut cx);
    rooted!(&in(cx) let sig_root = signal);
    set_prop_bool(&mut cx, sig_root.handle(), c"aborted", false);
    define_fn(
        &mut cx,
        sig_root.handle(),
        c"addEventListener",
        Some(noop),
        2,
    );

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
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"get",
        Some(prompt_null),
        1,
    );
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
    if !args.is_constructing() {
        args.rval().set(UndefinedValue());
        return true;
    }

    let this = args.thisv().get();
    if this.is_object() {
        args.rval().set(this);
    } else {
        let obj = new_plain_object(&mut cx);
        args.rval().set(ObjectValue(obj));
    }
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

/// `fetch()` — built in JS over the native `__aurora_fetch_sync__` helper so it
/// can return a real `Promise<Response>`. The underlying request is blocking
/// (Aurora has no non-blocking I/O wired into the JS runtime), but a Promise
/// that resolves immediately still beats one that never resolves: callers doing
/// `fetch(url).then(r => r.json()).then(...)` or `await fetch(url)` now actually
/// receive data instead of hanging forever.
unsafe fn install_fetch(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
    eval_bootstrap(
        cx,
        global,
        c"fetch",
        r#"
        (function() {
            function trace(msg) {
                if (globalThis.__aurora_debug_youtube__) console.log('[yt-fetch] ' + msg);
            }
            function traceError(where, error) {
                if (!globalThis.__aurora_debug_youtube__) return;
                var detail = error && (error.stack || error.message) || String(error);
                console.log('[yt-fetch] ' + where + ' threw: ' + detail);
            }
            function makeHeaders(raw) {
                return {
                    get: function(name) {
                        var key = String(name).toLowerCase();
                        return (raw && raw.headers && raw.headers[key] !== undefined) ? raw.headers[key] : null;
                    },
                    has: function(name) {
                        var key = String(name).toLowerCase();
                        return !!(raw && raw.headers && raw.headers[key] !== undefined);
                    },
                    forEach: function() {},
                    entries: function() { return [][Symbol.iterator](); }
                };
            }

            function makeResponse(raw, url) {
                var status = raw.ok ? (raw.status || 200) : (raw.status || 0);
                var ok = raw.ok && status >= 200 && status < 300;
                var bodyText = raw.body || '';
                var used = false;
                function consume() {
                    if (used) return Promise.reject(new TypeError('Body has already been read'));
                    used = true;
                    return Promise.resolve(bodyText);
                }
                var response = {
                    ok: ok,
                    status: status,
                    statusText: ok ? 'OK' : (raw.error || ''),
                    url: url,
                    redirected: false,
                    type: 'basic',
                    bodyUsed: false,
                    headers: makeHeaders(raw),
                    json: function() { return consume().then(function(t) {
                        trace('json ' + url + ' bytes=' + t.length);
                        try {
                            return JSON.parse(t);
                        } catch (e) {
                            traceError('json ' + url, e);
                            throw e;
                        }
                    }); },
                    text: function() { return consume(); },
                    arrayBuffer: function() { return consume().then(function(t) {
                        var buf = new ArrayBuffer(t.length);
                        var view = new Uint8Array(buf);
                        for (var i = 0; i < t.length; i++) { view[i] = t.charCodeAt(i) & 0xFF; }
                        return buf;
                    }); },
                    blob: function() { return consume().then(function(t) {
                        return (typeof Blob !== 'undefined') ? new Blob([t]) : { size: t.length, type: '' };
                    }); },
                    clone: function() { return makeResponse(raw, url); }
                };
                return response;
            }

            globalThis.fetch = function fetch(input, init) {
                var url = (typeof input === 'string') ? input
                    : (input && (input.url || input.href)) || String(input);
                trace('fetch ' + url);
                try {
                    var raw = __aurora_fetch_sync__(url);
                    trace('fetch result ' + url + ' ok=' + !!(raw && raw.ok) +
                        ' status=' + (raw && raw.status) + ' bytes=' + ((raw && raw.body && raw.body.length) || 0) +
                        (raw && raw.error ? (' error=' + raw.error) : ''));
                    if (raw && raw.ok) {
                        return Promise.resolve(makeResponse(raw, url));
                    }
                    return Promise.reject(new TypeError('Failed to fetch: ' + url +
                        (raw && raw.error ? (' (' + raw.error + ')') : '')));
                } catch (e) {
                    traceError('fetch ' + url, e);
                    return Promise.reject(e);
                }
            };
        })();
    "#,
    );
}

/// `XMLHttpRequest` — JS-level polyfill over `__aurora_fetch_sync__`. The
/// request itself runs synchronously inside `send()`, but result delivery is
/// deferred via `queueMicrotask` so listeners attached immediately after
/// `send()` (universal in real-world code) are registered before it fires.
unsafe fn install_xhr(cx: &mut JSContext, global: mozjs::gc::Handle<*mut JSObject>) {
    eval_bootstrap(
        cx,
        global,
        c"xhr",
        r#"
        (function() {
            function trace(msg) {
                if (globalThis.__aurora_debug_youtube__) console.log('[yt-xhr] ' + msg);
            }
            function traceError(where, error) {
                if (!globalThis.__aurora_debug_youtube__) return;
                var detail = error && (error.stack || error.message) || String(error);
                console.log('[yt-xhr] ' + where + ' threw: ' + detail);
            }
            function XMLHttpRequest() {
                this.readyState = 0;
                this.status = 0;
                this.statusText = '';
                this.responseText = '';
                this.response = '';
                this.responseURL = '';
                this.responseType = '';
                this.timeout = 0;
                this.withCredentials = false;
                this.onreadystatechange = null;
                this.onload = null;
                this.onloadend = null;
                this.onerror = null;
                this.onabort = null;
                this.ontimeout = null;
                this.upload = { addEventListener: function(){}, removeEventListener: function(){} };
                this._listeners = {};
                this._method = 'GET';
                this._url = '';
            }
            XMLHttpRequest.UNSENT = 0;
            XMLHttpRequest.OPENED = 1;
            XMLHttpRequest.HEADERS_RECEIVED = 2;
            XMLHttpRequest.LOADING = 3;
            XMLHttpRequest.DONE = 4;
            XMLHttpRequest.prototype.open = function(method, url) {
                this._method = method ? String(method) : 'GET';
                this._url = url ? String(url) : '';
                this.readyState = 1;
                this._dispatch('readystatechange');
            };
            XMLHttpRequest.prototype.setRequestHeader = function() {};
            XMLHttpRequest.prototype.getResponseHeader = function() { return null; };
            XMLHttpRequest.prototype.getAllResponseHeaders = function() { return ''; };
            XMLHttpRequest.prototype.overrideMimeType = function() {};
            XMLHttpRequest.prototype.addEventListener = function(type, cb) {
                if (typeof cb !== 'function') return;
                (this._listeners[type] = this._listeners[type] || []).push(cb);
            };
            XMLHttpRequest.prototype.removeEventListener = function(type, cb) {
                var l = this._listeners[type];
                if (!l) return;
                var i = l.indexOf(cb);
                if (i >= 0) l.splice(i, 1);
            };
            XMLHttpRequest.prototype._dispatch = function(type) {
                var ev = { type: type, target: this, currentTarget: this };
                var handler = this['on' + type];
                if (typeof handler === 'function') { try { handler.call(this, ev); } catch (e) { traceError('on' + type + ' ' + this._url, e); } }
                var l = this._listeners[type];
                if (l) { for (var i = 0; i < l.length; i++) { try { l[i].call(this, ev); } catch (e) { traceError(type + ' listener ' + this._url, e); } } }
            };
            XMLHttpRequest.prototype.abort = function() {
                this.readyState = 0;
                this.status = 0;
                this._dispatch('abort');
                this._dispatch('loadend');
            };
            XMLHttpRequest.prototype.send = function(body) {
                var self = this;
                queueMicrotask(function() {
                    if (self.readyState === 0) return; // aborted before send landed
                    var raw;
                    trace('send ' + self._url);
                    try {
                        raw = __aurora_fetch_sync__(self._url);
                    } catch (e) {
                        traceError('send ' + self._url, e);
                        raw = { ok: false, status: 0, body: '', error: String(e) };
                    }
                    self.readyState = 4;
                    trace('result ' + self._url + ' ok=' + !!(raw && raw.ok) +
                        ' status=' + (raw && raw.status) + ' bytes=' + ((raw && raw.body && raw.body.length) || 0) +
                        (raw && raw.error ? (' error=' + raw.error) : ''));
                    if (raw && raw.ok) {
                        self.status = raw.status || 200;
                        self.statusText = 'OK';
                        self.responseText = raw.body || '';
                        self.response = self.responseText;
                        self.responseURL = self._url;
                        self._dispatch('readystatechange');
                        self._dispatch('load');
                        self._dispatch('loadend');
                    } else {
                        self.status = 0;
                        self.statusText = '';
                        self._dispatch('readystatechange');
                        self._dispatch('error');
                        self._dispatch('loadend');
                    }
                });
            };
            globalThis.XMLHttpRequest = XMLHttpRequest;
        })();
    "#,
    );
}

unsafe extern "C" fn websocket_ctor(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_i32(&mut cx, obj_root.handle(), c"readyState", 3); // CLOSED
    define_fn(&mut cx, obj_root.handle(), c"send", Some(noop), 1);
    define_fn(&mut cx, obj_root.handle(), c"close", Some(noop), 0);
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"addEventListener",
        Some(noop),
        2,
    );
    args.rval().set(ObjectValue(obj));
    true
}
