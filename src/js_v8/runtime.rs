use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;
use std::time::Instant;

use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};

// V8 allows exactly one platform per process, initialized before the first
// isolate and never torn down (same constraint family as SpiderMonkey's
// JSEngine, but V8 tolerates living forever).
static V8_INIT: Once = Once::new();

fn ensure_platform() {
    V8_INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

pub(crate) struct V8Runtime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
    // Kept for the upcoming DOM bridge; unused until then.
    #[allow(dead_code)]
    document: NodePtr,
}

impl V8Runtime {
    pub(crate) fn new(document: NodePtr) -> Self {
        ensure_platform();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        let context = {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, v8::ContextOptions::default());
            v8::Global::new(scope, context)
        };
        Self {
            isolate,
            context,
            document,
        }
    }

    /// Evaluate a script and return its completion value as a string.
    /// Test/diagnostic helper; the `JsRuntime` trait only reports errors.
    #[cfg(test)]
    pub(crate) fn eval_to_string(&mut self, source: &str) -> Result<String, String> {
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        v8::tc_scope!(let scope, scope);
        compile_and_run(scope, source)
    }
}

/// Compile and run a script, returning its completion value stringified.
fn compile_and_run(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    source: &str,
) -> Result<String, String> {
    let code = v8::String::new(scope, source)
        .ok_or_else(|| "script source exceeds V8 string limits".to_string())?;
    let Some(script) = v8::Script::compile(scope, code, None) else {
        return Err(exception_message(scope, "compile error"));
    };
    match script.run(scope) {
        Some(value) => Ok(value.to_rust_string_lossy(scope)),
        None => Err(exception_message(scope, "uncaught exception")),
    }
}

fn exception_message(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    fallback: &str,
) -> String {
    scope
        .exception()
        .map(|exc| exc.to_rust_string_lossy(scope))
        .unwrap_or_else(|| fallback.to_string())
}

impl crate::js_engine::JsRuntime for V8Runtime {
    fn execute(&mut self, script: &str) -> Result<(), String> {
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        v8::tc_scope!(let scope, scope);
        compile_and_run(scope, script).map(|_| ())
    }

    // The methods below are honest no-ops: the DOM bridge, timers, and event
    // loop are not wired to V8 yet. They exist so the runtime is a drop-in
    // `Box<dyn JsRuntime>` for the engine-swap path.
    fn set_current_script(&mut self, _script: Option<&NodePtr>) {}

    fn set_shared_state(
        &mut self,
        _layout_tree: Rc<RefCell<LayoutTree>>,
        _stylesheet: Rc<RefCell<Stylesheet>>,
        _viewport: Rc<RefCell<ViewportSize>>,
    ) {
    }

    fn clear_dirty_bits(&mut self) {}
    fn has_dirty_bits(&self) -> bool {
        false
    }
    fn take_needs_reflow(&mut self) -> bool {
        false
    }

    fn tick(&mut self, _now: Instant) -> bool {
        false
    }
    fn drain_animation_frame_callbacks(&mut self, _now: Instant) -> bool {
        false
    }

    fn dispatch_event(&mut self, _node: &NodePtr, _event_type: &str) -> bool {
        false
    }
    fn fire_dom_content_loaded(&mut self) {}
    fn fire_load(&mut self) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }
    fn has_animation_frame_callbacks(&self) -> bool {
        false
    }
    fn has_ready_work(&self, _now: Instant) -> bool {
        false
    }
}
