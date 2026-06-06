use crate::dom::NodePtr;
use mozjs::jsapi::JSObject;
use super::capture::WindowCapture;
use super::registry::NodeRegistry;

/// Pinned state passed to all JS native callbacks via JS_SetContextPrivate.
/// Lives inside SmRuntime as Box<SmState> (stable address).
pub(super) struct SmState {
    pub(super) document: NodePtr,
    pub(super) registry: NodeRegistry,
    pub(super) window: WindowCapture,
    /// Raw pointer to the JS global object.  Always valid while SmRuntime is alive.
    pub(super) global: *mut JSObject,
}

// SmState is not Send because NodePtr (Rc) is not Send.
// That's correct — the JS runtime is single-threaded.
