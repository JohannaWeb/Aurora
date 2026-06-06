#![allow(unsafe_op_in_unsafe_fn)]
use mozjs::context::JSContext;
use mozjs::jsapi::JSObject;

use crate::dom::NodePtr;
use crate::js_sm::capture::WindowCapture;

mod browser_api;
mod core;
mod timers;

use browser_api::*;
use core::*;
use timers::*;

pub(in crate::js_sm) unsafe fn install_globals(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    _document: &NodePtr,
) -> WindowCapture {
    install_core_globals(cx, global);
    install_timers(cx, global);
    install_browser_apis(cx, global);

    WindowCapture::new()
}
