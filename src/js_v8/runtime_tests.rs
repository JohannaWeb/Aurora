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
fn v8_supports_basic_globals_and_console() {
    let mut runtime = V8Runtime::new(blank_dom());

    // window and self should be aliases for globalThis
    assert_eq!(runtime.eval_to_string("window === globalThis"), Ok("true".to_string()));
    assert_eq!(runtime.eval_to_string("self === globalThis"), Ok("true".to_string()));

    // console.log should be defined (it prints to stdout, so we just check it doesn't throw)
    assert_eq!(runtime.eval_to_string("typeof console.log"), Ok("function".to_string()));
    runtime.execute("console.log('Hello from V8!', {a: 1})").unwrap();
}

#[test]
fn v8_supports_timers_and_raf() {
    let mut runtime = V8Runtime::new(blank_dom());
    let now = std::time::Instant::now();

    // setTimeout
    runtime.execute("globalThis.timeoutFired = false; setTimeout(() => { globalThis.timeoutFired = true; }, 10)").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.timeoutFired"), Ok("false".to_string()));
    
    // Tick with immediate 'now' shouldn't fire it (delay is 10ms)
    runtime.tick(now);
    assert_eq!(runtime.eval_to_string("globalThis.timeoutFired"), Ok("false".to_string()));

    // Tick after delay should fire it
    runtime.tick(now + std::time::Duration::from_millis(20));
    assert_eq!(runtime.eval_to_string("globalThis.timeoutFired"), Ok("true".to_string()));

    // requestAnimationFrame
    runtime.execute("globalThis.rafFired = false; requestAnimationFrame((ts) => { globalThis.rafFired = true; globalThis.rafTs = ts; })").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.rafFired"), Ok("false".to_string()));

    runtime.drain_animation_frame_callbacks(now);
    assert_eq!(runtime.eval_to_string("globalThis.rafFired"), Ok("true".to_string()));
    assert_eq!(runtime.eval_to_string("typeof globalThis.rafTs"), Ok("number".to_string()));
}

#[test]
fn v8_supports_event_listeners() {
    let mut runtime = V8Runtime::new(blank_dom());

    runtime.execute("globalThis.eventFired = false; window.addEventListener('test', () => { globalThis.eventFired = true; })").unwrap();
    runtime.fire_dom_content_loaded(); // This fires 'DOMContentLoaded' but not 'test'
    assert_eq!(runtime.eval_to_string("globalThis.eventFired"), Ok("false".to_string()));

    runtime.execute("window.addEventListener('DOMContentLoaded', () => { globalThis.eventFired = true; })").unwrap();
    runtime.fire_dom_content_loaded();
    assert_eq!(runtime.eval_to_string("globalThis.eventFired"), Ok("true".to_string()));
}

#[test]
fn v8_supports_dom_queries() {
    let dom = Parser::new("<html><body><div id='mydiv' class='foo'>Hello</div><p>Para</p></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    // getElementById
    runtime.execute("globalThis.el = document.getElementById('mydiv')").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.el.tagName"), Ok("DIV".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.el.id"), Ok("mydiv".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.el.className"), Ok("foo".to_string()));

    // getElementsByTagName
    runtime.execute("globalThis.paras = document.getElementsByTagName('p')").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.paras.length"), Ok("1".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.paras[0].tagName"), Ok("P".to_string()));

    // querySelector
    runtime.execute("globalThis.q = document.querySelector('.foo')").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.q.id"), Ok("mydiv".to_string()));
}

#[test]
fn v8_supports_extended_dom_methods() {
    let dom = Parser::new("<html><body><div id='parent'><div id='child'></div></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    // querySelectorAll
    assert_eq!(runtime.eval_to_string("document.querySelectorAll('div').length"), Ok("2".to_string()));

    // parentNode, firstChild, lastChild
    runtime.execute("globalThis.child = document.getElementById('child'); globalThis.parent = document.getElementById('parent');").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.child.parentNode === globalThis.parent"), Ok("true".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.parent.firstChild === globalThis.child"), Ok("true".to_string()));
    
    // matches and closest
    assert_eq!(runtime.eval_to_string("globalThis.child.matches('#child')"), Ok("true".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.child.closest('#parent') === globalThis.parent"), Ok("true".to_string()));

    // appendChild
    runtime.execute("globalThis.newDiv = document.createElement('div'); globalThis.newDiv.id = 'new-div'; globalThis.parent.appendChild(globalThis.newDiv);").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.parent.querySelectorAll('div').length"), Ok("2".to_string())); // parent has #child and #new-div
    assert_eq!(runtime.eval_to_string("document.querySelectorAll('div').length"), Ok("3".to_string()));
    
    // getAttribute / setAttribute
    runtime.execute("globalThis.newDiv.setAttribute('data-test', 'v8')").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.getAttribute('data-test')"), Ok("v8".to_string()));
    
    // removeChild
    runtime.execute("globalThis.parent.removeChild(globalThis.child)").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.parent.querySelectorAll('div').length"), Ok("1".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.parent.firstChild.id"), Ok("new-div".to_string()));

    // textContent
    runtime.execute("globalThis.newDiv.appendChild(document.createTextNode('Hello Text'))").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.textContent"), Ok("Hello Text".to_string()));

    // innerHTML
    runtime.execute("globalThis.newDiv.innerHTML = '<span>Nested</span>'").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.querySelectorAll('span').length"), Ok("1".to_string()));

    // classList
    runtime.execute("globalThis.newDiv.classList.add('my-class');").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.classList.contains('my-class')"), Ok("true".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.className"), Ok("my-class".to_string()));
    runtime.execute("globalThis.newDiv.classList.remove('my-class');").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.classList.contains('my-class')"), Ok("false".to_string()));

    // style
    runtime.execute("globalThis.newDiv.style.color = 'red';").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.style.color"), Ok("red".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.getAttribute('style')"), Ok("color: red".to_string()));
    runtime.execute("globalThis.newDiv.style.backgroundColor = 'blue';").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.style.backgroundColor"), Ok("blue".to_string()));
    assert_eq!(runtime.eval_to_string("globalThis.newDiv.style.getPropertyValue('background-color')"), Ok("blue".to_string()));
}

#[test]
fn v8_supports_storage_and_fetch() {
    let mut runtime = V8Runtime::new(blank_dom());

    // localStorage
    runtime.execute("localStorage.setItem('test', 'value');").unwrap();
    assert_eq!(runtime.eval_to_string("localStorage.getItem('test')"), Ok("value".to_string()));
    runtime.execute("localStorage.removeItem('test');").unwrap();
    assert_eq!(runtime.eval_to_string("localStorage.getItem('test')"), Ok("null".to_string()));

    // fetch polyfill (basic check)
    assert_eq!(runtime.eval_to_string("typeof fetch"), Ok("function".to_string()));
    // We can't easily test actual network without mocking fetch_string, 
    // but we can check if it returns a Promise.
    runtime.execute("globalThis.p = fetch('https://google.com')").unwrap();
    assert_eq!(runtime.eval_to_string("globalThis.p instanceof Promise"), Ok("true".to_string()));
    
    // atob / btoa
    assert_eq!(runtime.eval_to_string("btoa('hello')"), Ok("aGVsbG8=".to_string()));
    assert_eq!(runtime.eval_to_string("atob('aGVsbG8=')"), Ok("hello".to_string()));
}

#[test]
fn v8_supports_document_structure_and_screen() {
    let dom = Parser::new("<html><head><title>Test Title</title></head><body><div id='content'></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(runtime.eval_to_string("document.title"), Ok("Test Title".to_string()));
    assert_eq!(runtime.eval_to_string("document.body.tagName"), Ok("BODY".to_string()));
    assert_eq!(runtime.eval_to_string("document.head.tagName"), Ok("HEAD".to_string()));
    assert_eq!(runtime.eval_to_string("document.documentElement.tagName"), Ok("HTML".to_string()));
    assert_eq!(runtime.eval_to_string("document.defaultView === window"), Ok("true".to_string()));

    // screen stub
    assert_eq!(runtime.eval_to_string("screen.width"), Ok("1200".to_string()));
    assert_eq!(runtime.eval_to_string("screen.height"), Ok("800".to_string()));
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
