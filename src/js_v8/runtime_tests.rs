use super::V8Runtime;
use crate::html::Parser;
use crate::js_engine::{EngineKind, JsRuntime, create_runtime};

fn blank_dom() -> crate::dom::NodePtr {
    Parser::new("<html><body></body></html>").parse_document()
}

#[test]
fn v8_executes_javascript_and_reports_exceptions() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string("[1, 2, 3].map(x => x * 2).join('-')"),
        Ok("2-4-6".to_string())
    );

    let err = runtime
        .eval_to_string("throw new TypeError('boom')")
        .unwrap_err();
    assert!(err.contains("boom"), "{err}");

    // State persists across execute calls within the same context.
    runtime.eval_to_string("globalThis.counter = 41").unwrap();
    assert_eq!(
        runtime.eval_to_string("++globalThis.counter"),
        Ok("42".to_string())
    );
}

#[test]
fn engines_hot_swap_behind_the_js_runtime_trait() {
    // The same driver code must work against any backend picked at runtime —
    // this is the dependency-injection seam the runner uses (EngineKind comes
    // from AURORA_JS_ENGINE there; here we iterate explicitly).
    let kinds = [EngineKind::SpiderMonkey, EngineKind::V8];

    for kind in kinds {
        let dom = blank_dom();
        let mut runtime: Box<dyn JsRuntime> = create_runtime(kind, &dom)
            .unwrap_or_else(|e| panic!("{kind:?} backend unavailable: {e}"));

        runtime
            .execute("globalThis.answer = 6 * 7;")
            .unwrap_or_else(|e| panic!("{kind:?} failed to execute: {e}"));
        // Observable through the trait alone: a wrong value would throw and
        // surface as Err from execute.
        runtime
            .execute("if (globalThis.answer !== 42) throw new Error('engine state lost');")
            .unwrap_or_else(|e| panic!("{kind:?} lost state across execute calls: {e}"));
        assert!(
            runtime.execute("syntax error here").is_err(),
            "{kind:?} should surface compile errors"
        );
    }
}

#[test]
fn compiled_out_engines_return_err_not_panic() {
    #[cfg(not(feature = "engine-boa"))]
    {
        let dom = blank_dom();
        let err = create_runtime(EngineKind::Boa, &dom).err();
        assert!(err.is_some_and(|e| e.contains("engine-boa")));
    }
}
