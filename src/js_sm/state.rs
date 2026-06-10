use super::capture::WindowCapture;
use super::mutation_observer::MutationObserverEntry;
use super::registry::NodeRegistry;
use crate::dom::NodePtr;
use mozjs::jsapi::JSObject;

/// Pinned state passed to all JS native callbacks via JS_SetContextPrivate.
/// Lives inside SmRuntime as Box<SmState> (stable address).
pub(super) struct SmState {
    pub(super) document: NodePtr,
    pub(super) registry: NodeRegistry,
    pub(super) window: WindowCapture,
    /// Raw pointer to the JS global object.  Always valid while SmRuntime is alive.
    pub(super) global: *mut JSObject,
    /// Active `MutationObserver.observe()` registrations and their pending records.
    pub(super) mutation_observers: Vec<MutationObserverEntry>,
}

// SmState is not Send because NodePtr (Rc) is not Send.
// That's correct — the JS runtime is single-threaded.
