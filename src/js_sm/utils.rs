#![allow(unsafe_op_in_unsafe_fn)]
use std::ffi::{CStr, CString};
use std::ptr::NonNull;

use mozjs::context::JSContext;
use mozjs::conversions::jsstr_to_string;
use mozjs::jsapi::{JSObject, Value};
use mozjs::jsval::{BooleanValue, DoubleValue, NullValue, ObjectValue, StringValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::{evaluate_script, wrappers2, CompileOptionsWrapper};

use super::state::SmState;

// ── State access ────────────────────────────────────────────────────────────

/// Retrieve the SmState pointer set via JS_SetContextPrivate.
/// Only valid inside native callbacks (while JS is executing).
pub(super) unsafe fn get_state_ptr(cx: &JSContext) -> *mut SmState {
    wrappers2::JS_GetContextPrivate(cx) as *mut SmState
}

/// Callback-safe state borrow.
/// # Safety: Must only be called inside a JS native callback.
pub(super) unsafe fn with_state<F, R>(cx: &mut JSContext, f: F) -> R
where
    F: FnOnce(&mut SmState) -> R,
{
    let ptr = get_state_ptr(cx);
    debug_assert!(!ptr.is_null(), "SmState not set on JSContext");
    f(&mut *ptr)
}

// ── Object helpers ───────────────────────────────────────────────────────────

pub(super) unsafe fn new_plain_object(cx: &mut JSContext) -> *mut JSObject {
    wrappers2::JS_NewPlainObject(cx)
}

// ── Property setters ─────────────────────────────────────────────────────────

pub(super) unsafe fn set_prop_f64(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    val: f64,
) {
    rooted!(&in(cx) let v = DoubleValue(val));
    wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
}

pub(super) unsafe fn set_prop_i32(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    val: i32,
) {
    rooted!(&in(cx) let v = DoubleValue(val as f64));
    wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
}

pub(super) unsafe fn set_prop_bool(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    val: bool,
) {
    rooted!(&in(cx) let v = BooleanValue(val));
    wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
}

pub(super) unsafe fn set_prop_str(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    val: &str,
) {
    let js_str = new_js_string(cx, val);
    if !js_str.is_null() {
        rooted!(&in(cx) let v = StringValue(&*js_str));
        wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
    }
}

pub(super) unsafe fn set_prop_null(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
) {
    rooted!(&in(cx) let v = NullValue());
    wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
}

pub(super) unsafe fn set_prop_obj(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    val: *mut JSObject,
) {
    if val.is_null() {
        set_prop_null(cx, obj, name);
    } else {
        rooted!(&in(cx) let v = ObjectValue(val));
        wrappers2::JS_SetProperty(cx, obj, name.as_ptr(), v.handle());
    }
}

// ── Property getters ─────────────────────────────────────────────────────────

pub(super) unsafe fn get_prop_string(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
) -> Option<String> {
    rooted!(&in(cx) let mut val = UndefinedValue());
    if wrappers2::JS_GetProperty(cx, obj, name.as_ptr(), val.handle_mut()) && val.get().is_string() {
        let raw = val.get().to_string();
        if !raw.is_null() {
            return Some(jsstr_to_string(cx.raw_cx(), NonNull::new_unchecked(raw)));
        }
    }
    None
}

// ── Function definition ──────────────────────────────────────────────────────

pub(super) unsafe fn define_fn(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    func: mozjs::jsapi::JSNative,
    nargs: u32,
) {
    wrappers2::JS_DefineFunction(cx, obj, name.as_ptr(), func, nargs, 0);
}

pub(super) unsafe fn define_getter(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    getter: mozjs::jsapi::JSNative,
) {
    use mozjs::jsapi::JSPROP_ENUMERATE;
    wrappers2::JS_DefineProperty1(cx, obj, name.as_ptr(), getter, None, JSPROP_ENUMERATE as u32);
}

pub(super) unsafe fn define_accessor(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    name: &CStr,
    getter: mozjs::jsapi::JSNative,
    setter: mozjs::jsapi::JSNative,
) {
    use mozjs::jsapi::JSPROP_ENUMERATE;
    wrappers2::JS_DefineProperty1(cx, obj, name.as_ptr(), getter, setter, JSPROP_ENUMERATE as u32);
}

/// Extract a __node_id__ integer from a JS value that represents a DOM node object.
pub(super) unsafe fn val_to_node_id(cx: &mut JSContext, val: Value) -> Option<u32> {
    if !val.is_object() { return None; }
    let obj = val.to_object_or_null();
    if obj.is_null() { return None; }
    rooted!(&in(cx) let obj_root = obj);
    rooted!(&in(cx) let mut id_val = UndefinedValue());
    if wrappers2::JS_GetProperty(cx, obj_root.handle(), c"__node_id__".as_ptr(), id_val.handle_mut())
        && id_val.get().is_number()
    {
        Some(id_val.get().to_number() as u32)
    } else {
        None
    }
}

// ── String helpers ───────────────────────────────────────────────────────────

pub(super) unsafe fn new_js_string(cx: &mut JSContext, s: &str) -> *mut mozjs::jsapi::JSString {
    let cs = CString::new(s.as_bytes()).unwrap_or_default();
    wrappers2::JS_NewStringCopyZ(cx, cs.as_ptr())
}

pub(super) unsafe fn value_to_string(cx: &mut JSContext, val: mozjs::gc::Handle<Value>) -> String {
    let raw = wrappers2::ToStringSlow(cx, val);
    if raw.is_null() {
        return String::new();
    }
    jsstr_to_string(cx.raw_cx(), NonNull::new_unchecked(raw))
}

pub(super) unsafe fn arg_to_string(cx: &mut JSContext, args: &mozjs::jsapi::CallArgs, idx: u32) -> String {
    if args.argc_ > idx {
        rooted!(&in(cx) let v = args.get(idx).get());
        value_to_string(cx, v.handle())
    } else {
        String::new()
    }
}

pub(super) unsafe fn arg_to_f64(args: &mozjs::jsapi::CallArgs, idx: u32) -> f64 {
    if args.argc_ > idx {
        let v = args.get(idx).get();
        if v.is_number() {
            v.to_number()
        } else {
            0.0
        }
    } else {
        0.0
    }
}

pub(super) unsafe fn arg_to_object(args: &mozjs::jsapi::CallArgs, idx: u32) -> *mut JSObject {
    if args.argc_ > idx {
        let v = args.get(idx).get();
        if v.is_object() {
            v.to_object_or_null()
        } else {
            std::ptr::null_mut()
        }
    } else {
        std::ptr::null_mut()
    }
}

// ── Callback storage on global ───────────────────────────────────────────────
// Callbacks are stored as properties `__cb{N}__` on the global object.
// This keeps them alive (prevents GC) since they're reachable from the global.

pub(super) fn cb_prop_name(id: u32) -> CString {
    CString::new(format!("__cb{}__", id)).unwrap()
}

pub(super) unsafe fn store_callback(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    id: u32,
    val: mozjs::gc::Handle<Value>,
) {
    let name = cb_prop_name(id);
    wrappers2::JS_SetProperty(cx, global, name.as_ptr(), val);
}

pub(super) unsafe fn call_stored_callback(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    id: u32,
    args_slice: &[Value],
) -> bool {
    let name = cb_prop_name(id);
    rooted!(&in(cx) let mut cb_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(cx, global, name.as_ptr(), cb_val.handle_mut()) {
        return false;
    }
    if !cb_val.get().is_object() {
        return false;
    }
    rooted!(&in(cx) let mut rval = UndefinedValue());

    if args_slice.is_empty() {
        let empty = mozjs::jsapi::HandleValueArray::empty();
        wrappers2::JS_CallFunctionValue(cx, global, cb_val.handle(), &empty, rval.handle_mut())
    } else {
        // Single argument — build raw jsapi HandleValueArray pointing to the value on the stack.
        // The value is already rooted (it came from a rooted slot), so this is safe for the call duration.
        let arr = mozjs::jsapi::HandleValueArray::from(unsafe {
            mozjs::jsapi::Handle::from_marked_location(&args_slice[0] as *const Value)
        });
        wrappers2::JS_CallFunctionValue(cx, global, cb_val.handle(), &arr, rval.handle_mut())
    }
}

pub(super) unsafe fn delete_callback(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    id: u32,
) {
    let name = cb_prop_name(id);
    rooted!(&in(cx) let v = UndefinedValue());
    wrappers2::JS_SetProperty(cx, global, name.as_ptr(), v.handle());
}

// ── Bootstrap script evaluation ──────────────────────────────────────────────

/// Evaluate a JS source string against `global`. Used during global setup to
/// install polyfills (constructors, prototype chains) that are far simpler to
/// express as JS than to build through raw JSAPI calls. Errors are reported to
/// stderr but not propagated — a broken polyfill must not abort engine setup.
pub(super) unsafe fn eval_bootstrap(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    label: &'static std::ffi::CStr,
    src: &str,
) {
    rooted!(&in(cx) let mut rval = UndefinedValue());
    let options = CompileOptionsWrapper::new(cx, label.to_str().unwrap_or("bootstrap"), 1);
    if evaluate_script(cx, global, src, rval.handle_mut(), options).is_err() {
        let msg = pending_exception_string(cx);
        eprintln!("JS bootstrap error ({}): {}", label.to_string_lossy(), msg);
    }
}

// ── Exception reporting ───────────────────────────────────────────────────────

pub(super) unsafe fn pending_exception_string(cx: &mut JSContext) -> String {
    rooted!(&in(cx) let mut exn = UndefinedValue());
    if wrappers2::JS_GetPendingException(cx, exn.handle_mut()) {
        let s = value_to_string(cx, exn.handle());
        wrappers2::JS_ClearPendingException(cx);
        s
    } else {
        "JS error (no exception)".to_string()
    }
}
