use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};
use crate::window::SnapshotRebuildReason;

/// Which JS engine backend to construct.
///
/// Aurora now ships a single JavaScript backend: V8. This enum stays as a small
/// seam so call sites do not need to name the concrete runtime directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineKind {
    V8,
}

impl EngineKind {
    /// Unset, unsupported, or legacy engine names all fall back to V8.
    pub(crate) fn from_env() -> Self {
        match std::env::var("AURORA_JS_ENGINE").as_deref() {
            Ok("v8") => Self::V8,
            Ok(other) => {
                log::warn!("[JS] unsupported AURORA_JS_ENGINE={other:?}; using v8");
                Self::V8
            }
            _ => Self::default_compiled(),
        }
    }

    /// The engine to use when none is explicitly requested.
    pub(crate) fn default_compiled() -> Self {
        Self::V8
    }
}

/// Dependency-injection seam: every place that needs a JS runtime asks this
/// factory instead of naming the concrete runtime type.
pub(crate) fn create_runtime(
    kind: EngineKind,
    dom: &NodePtr,
    render_document: Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>,
) -> Result<Box<dyn JsRuntime>, String> {
    match kind {
        EngineKind::V8 => {
            #[cfg(feature = "v8")]
            {
                Ok(Box::new(crate::js_v8::V8Runtime::with_render_document(
                    dom.clone(),
                    render_document,
                )))
            }
            #[cfg(not(feature = "v8"))]
            {
                Err("V8 backend not compiled in (build with --features v8)".to_string())
            }
        }
    }
}

/// Common interface implemented by the JavaScript backend.
///
/// All methods take `&mut self` so the trait is object-safe and can be stored as
/// `Box<dyn JsRuntime>` without any generic parameters leaking into callers.
pub(crate) trait JsRuntime {
    fn execute(&mut self, script: &str) -> Result<(), String>;
    fn set_current_script(&mut self, script: Option<&NodePtr>);

    fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<LayoutTree>>,
        stylesheet: Rc<RefCell<Stylesheet>>,
        viewport: Rc<RefCell<ViewportSize>>,
    );

    fn set_render_document(
        &mut self,
        _render_document: Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>,
    ) {
    }

    fn clear_dirty_bits(&mut self);
    fn has_dirty_bits(&self) -> bool;
    fn take_needs_reflow(&mut self) -> bool;
    fn take_snapshot_rebuild_reason(&mut self) -> Option<SnapshotRebuildReason> {
        None
    }

    fn tick(&mut self, now: Instant) -> bool;
    fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool;

    /// Recompute style and layout for the current document.
    /// Returns true if any work was performed.
    fn perform_style_and_layout(&mut self) -> bool {
        false
    }

    /// Paint the current frame.
    /// Returns true if any work was performed.
    fn perform_paint(&mut self) -> bool {
        false
    }

    /// Deliver any pending `MutationObserver` records to their callbacks.
    /// Returns true if any were delivered (so the event-loop pump keeps going).
    /// Backends that drain observers internally (or lack them) keep the default.
    fn deliver_mutation_records(&mut self) -> bool {
        false
    }

    fn dispatch_event(&mut self, node: &NodePtr, event_type: &str) -> bool;
    fn fire_dom_content_loaded(&mut self);
    fn fire_load(&mut self);

    fn next_deadline(&self) -> Option<Instant>;
    fn has_animation_frame_callbacks(&self) -> bool;
    fn has_ready_work(&self, now: Instant) -> bool;
}
