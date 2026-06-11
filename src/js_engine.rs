use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};

/// Which JS engine backend to construct. SpiderMonkey is the main engine;
/// Boa and V8 are optional backends behind the `engine-boa` / `v8` features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineKind {
    SpiderMonkey,
    Boa,
    V8,
}

impl EngineKind {
    /// Engine selection via `AURORA_JS_ENGINE` (`spidermonkey`/`sm`, `boa`,
    /// `v8`). Unset or unrecognized values fall back to whichever of
    /// SpiderMonkey/V8 is compiled in (they're mutually exclusive features).
    pub(crate) fn from_env() -> Self {
        match std::env::var("AURORA_JS_ENGINE").as_deref() {
            Ok("boa") => Self::Boa,
            Ok("v8") => Self::V8,
            #[cfg(feature = "engine-spidermonkey")]
            Ok("spidermonkey" | "sm") => Self::SpiderMonkey,
            _ => Self::default_engine(),
        }
    }

    #[cfg(feature = "engine-spidermonkey")]
    fn default_engine() -> Self {
        Self::SpiderMonkey
    }

    #[cfg(not(feature = "engine-spidermonkey"))]
    fn default_engine() -> Self {
        Self::V8
    }
}

/// Dependency-injection seam: every place that needs a JS runtime asks this
/// factory instead of naming a concrete engine type. Engines compiled out via
/// features return `Err` so callers can fall back rather than fail to build.
pub(crate) fn create_runtime(
    kind: EngineKind,
    dom: &NodePtr,
) -> Result<Box<dyn JsRuntime>, String> {
    match kind {
        EngineKind::SpiderMonkey => {
            #[cfg(feature = "engine-spidermonkey")]
            {
                Ok(Box::new(crate::js_sm::SmRuntime::new(dom.clone())))
            }
            #[cfg(not(feature = "engine-spidermonkey"))]
            {
                Err("SpiderMonkey backend not compiled in (build with --features engine-spidermonkey)".to_string())
            }
        }
        EngineKind::Boa => {
            #[cfg(feature = "engine-boa")]
            {
                Ok(Box::new(crate::js_boa::BoaRuntime::new(dom.clone())))
            }
            #[cfg(not(feature = "engine-boa"))]
            {
                Err("Boa backend not compiled in (build with --features engine-boa)".to_string())
            }
        }
        EngineKind::V8 => {
            #[cfg(feature = "v8")]
            {
                Ok(Box::new(crate::js_v8::V8Runtime::new(dom.clone())))
            }
            #[cfg(not(feature = "v8"))]
            {
                Err("V8 backend not compiled in (build with --features v8)".to_string())
            }
        }
    }
}

/// Common interface implemented by every JS engine backend (SpiderMonkey, Boa, …).
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

    fn clear_dirty_bits(&mut self);
    fn has_dirty_bits(&self) -> bool;
    fn take_needs_reflow(&mut self) -> bool;

    fn tick(&mut self, now: Instant) -> bool;
    fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool;

    fn dispatch_event(&mut self, node: &NodePtr, event_type: &str) -> bool;
    fn fire_dom_content_loaded(&mut self);
    fn fire_load(&mut self);

    fn next_deadline(&self) -> Option<Instant>;
    fn has_animation_frame_callbacks(&self) -> bool;
    fn has_ready_work(&self, now: Instant) -> bool;
}
