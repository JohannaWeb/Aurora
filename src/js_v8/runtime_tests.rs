use super::V8Runtime;
use crate::blitz_document::BlitzDocument;
use crate::html::Parser;
use crate::identity::{Capability, Identity, IdentityKind};
use crate::js_engine::{EngineKind, JsRuntime, create_runtime};
use crate::window::SnapshotRebuildReason;
use std::cell::RefCell;
use std::rc::Rc;

#[path = "mirror_mutation_tests.rs"]
mod mirror_mutation_tests;

fn blank_dom() -> crate::dom::NodePtr {
    Parser::new("<html><body></body></html>").parse_document()
}

fn test_identity() -> Identity {
    Identity::new(
        "did:aurora:test",
        "Aurora Test",
        IdentityKind::Agent,
        [Capability::ReadWorkspace, Capability::NetworkAccess],
    )
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
    assert_eq!(
        runtime.eval_to_string("window === globalThis"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("self === globalThis"),
        Ok("true".to_string())
    );

    // console.log should be defined (it prints to stdout, so we just check it doesn't throw)
    assert_eq!(
        runtime.eval_to_string("typeof console.log"),
        Ok("function".to_string())
    );
    runtime
        .execute("console.log('Hello from V8!', {a: 1})")
        .unwrap();
}

#[test]
fn v8_supports_timers_and_raf() {
    let mut runtime = V8Runtime::new(blank_dom());
    let now = std::time::Instant::now();

    // setTimeout
    runtime.execute("globalThis.timeoutFired = false; setTimeout(() => { globalThis.timeoutFired = true; }, 10)").unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.timeoutFired"),
        Ok("false".to_string())
    );

    // Tick with immediate 'now' shouldn't fire it (delay is 10ms)
    runtime.tick(now);
    assert_eq!(
        runtime.eval_to_string("globalThis.timeoutFired"),
        Ok("false".to_string())
    );

    // Tick after delay should fire it
    runtime.tick(now + std::time::Duration::from_millis(20));
    assert_eq!(
        runtime.eval_to_string("globalThis.timeoutFired"),
        Ok("true".to_string())
    );

    // requestAnimationFrame
    runtime.execute("globalThis.rafFired = false; requestAnimationFrame((ts) => { globalThis.rafFired = true; globalThis.rafTs = ts; })").unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.rafFired"),
        Ok("false".to_string())
    );

    runtime.drain_animation_frame_callbacks(now);
    assert_eq!(
        runtime.eval_to_string("globalThis.rafFired"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("typeof globalThis.rafTs"),
        Ok("number".to_string())
    );
}

#[test]
fn v8_drains_raf_scheduled_from_timer_then_zero_timer() {
    let mut runtime = V8Runtime::new(blank_dom());

    runtime
        .execute(
            r#"
            globalThis.chain = [];
            setTimeout(() => {
                chain.push('timer1');
                try {
                    requestAnimationFrame(() => {
                        chain.push('raf');
                        setTimeout(() => chain.push('timer2'));
                    });
                    chain.push('scheduled');
                } catch (e) {
                    chain.push('error:' + e.message);
                }
            });
            "#,
        )
        .unwrap();

    let now = std::time::Instant::now();
    runtime.tick(now + std::time::Duration::from_millis(1));
    runtime.tick(now + std::time::Duration::from_millis(2));

    assert_eq!(
        runtime.eval_to_string("chain.join('|')"),
        Ok("timer1|scheduled|raf|timer2".to_string())
    );
}

#[test]
fn v8_supports_event_listeners() {
    let mut runtime = V8Runtime::new(blank_dom());

    runtime.execute("globalThis.eventFired = false; window.addEventListener('test', () => { globalThis.eventFired = true; })").unwrap();
    runtime.fire_dom_content_loaded(); // This fires 'DOMContentLoaded' but not 'test'
    assert_eq!(
        runtime.eval_to_string("globalThis.eventFired"),
        Ok("false".to_string())
    );

    runtime
        .execute(
            "window.addEventListener('DOMContentLoaded', () => { globalThis.eventFired = true; })",
        )
        .unwrap();
    runtime.fire_dom_content_loaded();
    assert_eq!(
        runtime.eval_to_string("globalThis.eventFired"),
        Ok("true".to_string())
    );
}

#[test]
fn v8_lifecycle_events_reach_window_once() {
    let mut runtime = V8Runtime::new(blank_dom());

    runtime
        .execute(
            r#"
            globalThis.dclWindowCount = 0;
            globalThis.dclDocumentCount = 0;
            window.addEventListener('DOMContentLoaded', () => { globalThis.dclWindowCount++; });
            document.addEventListener('DOMContentLoaded', () => { globalThis.dclDocumentCount++; });
            "#,
        )
        .unwrap();

    runtime.fire_dom_content_loaded();

    assert_eq!(
        runtime.eval_to_string("globalThis.dclWindowCount + '|' + globalThis.dclDocumentCount"),
        Ok("1|1".to_string())
    );
}

#[test]
fn v8_mutation_observer_reports_childlist_and_attributes() {
    // Regression: MutationObserver used to be a never-firing JS stub.
    let dom = Parser::new("<html><body><div id='t'></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    runtime
        .execute(
            r#"
            globalThis.records = [];
            const t = document.getElementById('t');
            const mo = new MutationObserver((recs) => {
                for (const r of recs) {
                    records.push(r.type + ':added=' + r.addedNodes.length
                        + ':removed=' + r.removedNodes.length
                        + ':target=' + (r.target && r.target.id)
                        + ':attr=' + r.attributeName);
                }
            });
            mo.observe(t, { childList: true, attributes: true });
            t.appendChild(document.createElement('span'));
            t.setAttribute('data-x', '1');
            "#,
        )
        .unwrap();

    // Records are delivered when the event loop is pumped.
    assert!(runtime.deliver_mutation_records());
    assert_eq!(
        runtime.eval_to_string("globalThis.records.join('|')"),
        Ok("childList:added=1:removed=0:target=t:attr=null|attributes:added=0:removed=0:target=t:attr=data-x"
            .to_string())
    );

    // After disconnect, no further records are delivered.
    runtime
        .execute(
            "mo.disconnect(); globalThis.records = []; t.appendChild(document.createElement('b'));",
        )
        .unwrap();
    assert!(!runtime.deliver_mutation_records());
    assert_eq!(
        runtime.eval_to_string("globalThis.records.length"),
        Ok("0".to_string())
    );
}

#[test]
fn v8_mutation_observer_subtree_observes_descendants() {
    let dom = Parser::new("<html><body><div id='root'><div id='mid'></div></div></body></html>")
        .parse_document();
    let mut runtime = V8Runtime::new(dom);
    runtime
        .execute(
            r#"
            globalThis.hits = 0;
            const root = document.getElementById('root');
            const mid = document.getElementById('mid');
            const mo = new MutationObserver((recs) => { globalThis.hits += recs.length; });
            mo.observe(root, { childList: true, subtree: true });
            mid.appendChild(document.createElement('span')); // mutation on a descendant
            "#,
        )
        .unwrap();
    assert!(runtime.deliver_mutation_records());
    assert_eq!(
        runtime.eval_to_string("globalThis.hits"),
        Ok("1".to_string())
    );
}

#[test]
fn v8_event_target_capture_once_and_remove() {
    // Exercises the JS EventTarget: capture-phase ordering, `once`, and
    // removeEventListener — all on real DOM nodes.
    let dom =
        Parser::new("<html><body><div id='outer'><span id='inner'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    // Capture fires on ancestors before the target; `once` auto-removes.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const outer = document.getElementById('outer');
                const inner = document.getElementById('inner');
                const order = [];
                outer.addEventListener('go', () => order.push('outer-capture'), { capture: true });
                inner.addEventListener('go', () => order.push('inner-target'));
                inner.dispatchEvent(new CustomEvent('go', { bubbles: true }));
                return order.join('>');
            })()"#
        ),
        Ok("outer-capture>inner-target".to_string())
    );

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const inner = document.getElementById('inner');
                let n = 0;
                inner.addEventListener('once-evt', () => n++, { once: true });
                inner.dispatchEvent(new CustomEvent('once-evt'));
                inner.dispatchEvent(new CustomEvent('once-evt'));
                let m = 0;
                const h = () => m++;
                inner.addEventListener('rm-evt', h);
                inner.removeEventListener('rm-evt', h);
                inner.dispatchEvent(new CustomEvent('rm-evt'));
                return n + '|' + m;
            })()"#
        ),
        Ok("1|0".to_string())
    );
}

#[test]
fn v8_dispatch_event_fires_window_and_document_listeners() {
    // Regression: window/document `dispatchEvent` used to be no-op polyfill stubs,
    // so listeners registered via addEventListener never fired.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let win=false, doc=false;
                window.addEventListener('w-evt', () => { win = true; });
                document.addEventListener('d-evt', () => { doc = true; });
                window.dispatchEvent(new CustomEvent('w-evt'));
                document.dispatchEvent(new CustomEvent('d-evt'));
                return win + '|' + doc;
            })()"#
        ),
        Ok("true|true".to_string())
    );
}

#[test]
fn v8_composed_event_reaches_shadow_host() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const root = host.attachShadow({ mode: 'open' });
                const child = document.createElement('span');
                root.appendChild(child);
                let hostCount = 0;
                host.addEventListener('shadow-ping', () => { hostCount++; });
                child.dispatchEvent(new CustomEvent('shadow-ping', { bubbles: true, composed: true }));
                return String(hostCount);
            })()"#
        ),
        Ok("1".to_string())
    );
}

#[test]
fn v8_dispatch_event_sets_composed_path() {
    let dom =
        Parser::new("<html><body><div id='outer'><span id='inner'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const outer = document.getElementById('outer');
                const inner = document.getElementById('inner');
                let result = '';
                outer.addEventListener('path-ping', event => {
                    const path = event.composedPath();
                    result = (path[0] === inner) + '|' + path.some(node => node === outer);
                });
                inner.dispatchEvent(new CustomEvent('path-ping', { bubbles: true, composed: true }));
                return result;
            })()"#
        ),
        Ok("true|true".to_string())
    );
}

#[test]
fn v8_dispatch_event_bubbles_to_ancestors_and_document() {
    // Regression: element dispatch fired only the target's own listeners. A
    // bubbling event must reach ancestor elements and document-level listeners.
    let dom =
        Parser::new("<html><body><div id='outer'><span id='inner'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const outer = document.getElementById('outer');
                const inner = document.getElementById('inner');
                let onOuter=false, onDoc=false, onTarget=false;
                inner.addEventListener('ping', () => { onTarget = true; });
                outer.addEventListener('ping', () => { onOuter = true; });
                document.addEventListener('ping', () => { onDoc = true; });
                inner.dispatchEvent(new CustomEvent('ping', { bubbles: true }));
                return [onTarget, onOuter, onDoc].join('|');
            })()"#
        ),
        Ok("true|true|true".to_string())
    );

    // A non-bubbling event must NOT reach ancestors.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const outer = document.getElementById('outer');
                const inner = document.getElementById('inner');
                let onOuter=false;
                outer.addEventListener('solo', () => { onOuter = true; });
                inner.dispatchEvent(new CustomEvent('solo', { bubbles: false }));
                return String(onOuter);
            })()"#
        ),
        Ok("false".to_string())
    );
}

#[test]
fn v8_supports_dom_queries() {
    let dom =
        Parser::new("<html><body><div id='mydiv' class='foo'>Hello</div><p>Para</p></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    // getElementById
    runtime
        .execute("globalThis.el = document.getElementById('mydiv')")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.el.tagName"),
        Ok("DIV".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.el.id"),
        Ok("mydiv".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.el.className"),
        Ok("foo".to_string())
    );

    // getElementsByTagName
    runtime
        .execute("globalThis.paras = document.getElementsByTagName('p')")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.paras.length"),
        Ok("1".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.paras[0].tagName"),
        Ok("P".to_string())
    );

    // querySelector
    runtime
        .execute("globalThis.q = document.querySelector('.foo')")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.q.id"),
        Ok("mydiv".to_string())
    );
}

#[test]
fn v8_supports_extended_dom_methods() {
    let dom =
        Parser::new("<html><body><div id='parent'><div id='child'></div></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    // querySelectorAll
    assert_eq!(
        runtime.eval_to_string("document.querySelectorAll('div').length"),
        Ok("2".to_string())
    );

    // parentNode, firstChild, lastChild
    runtime.execute("globalThis.child = document.getElementById('child'); globalThis.parent = document.getElementById('parent');").unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.child.parentNode === globalThis.parent"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.parent.firstChild === globalThis.child"),
        Ok("true".to_string())
    );

    // matches and closest
    assert_eq!(
        runtime.eval_to_string("globalThis.child.matches('#child')"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.child.closest('#parent') === globalThis.parent"),
        Ok("true".to_string())
    );

    // appendChild
    runtime.execute("globalThis.newDiv = document.createElement('div'); globalThis.newDiv.id = 'new-div'; globalThis.parent.appendChild(globalThis.newDiv);").unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.parent.querySelectorAll('div').length"),
        Ok("2".to_string())
    ); // parent has #child and #new-div
    assert_eq!(
        runtime.eval_to_string("document.querySelectorAll('div').length"),
        Ok("3".to_string())
    );

    // getAttribute / setAttribute
    runtime
        .execute("globalThis.newDiv.setAttribute('data-test', 'v8')")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.getAttribute('data-test')"),
        Ok("v8".to_string())
    );

    // removeChild
    runtime
        .execute("globalThis.parent.removeChild(globalThis.child)")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.parent.querySelectorAll('div').length"),
        Ok("1".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.parent.firstChild.id"),
        Ok("new-div".to_string())
    );

    // textContent
    runtime
        .execute("globalThis.newDiv.appendChild(document.createTextNode('Hello Text'))")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.textContent"),
        Ok("Hello Text".to_string())
    );

    // innerHTML
    runtime
        .execute("globalThis.newDiv.innerHTML = '<span>Nested</span>'")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.querySelectorAll('span').length"),
        Ok("1".to_string())
    );

    // classList
    runtime
        .execute("globalThis.newDiv.classList.add('my-class');")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.classList.contains('my-class')"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.className"),
        Ok("my-class".to_string())
    );
    runtime
        .execute("globalThis.newDiv.classList.remove('my-class');")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.classList.contains('my-class')"),
        Ok("false".to_string())
    );

    // style
    runtime
        .execute("globalThis.newDiv.style.color = 'red';")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.style.color"),
        Ok("red".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.getAttribute('style')"),
        Ok("color: red".to_string())
    );
    runtime
        .execute("globalThis.newDiv.style.backgroundColor = 'blue';")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.style.backgroundColor"),
        Ok("blue".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("globalThis.newDiv.style.getPropertyValue('background-color')"),
        Ok("blue".to_string())
    );
}

#[test]
fn v8_supports_youtube_relevant_element_navigation_surface() {
    let dom = Parser::new(
        "<html><body><section id='root'>text<span id='a'></span><em id='b'></em></section></body></html>",
    )
    .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const root = document.getElementById('root');
            const a = document.getElementById('a');
            const b = document.getElementById('b');
            [
                root.childElementCount,
                root.firstElementChild.id,
                root.lastElementChild.id,
                a.nextElementSibling.id,
                b.previousElementSibling.id,
                a.parentElement.id,
                root.hasChildNodes(),
                a.isConnected
            ].join('|')
            "#
        ),
        Ok("2|a|b|b|a|root|true|true".to_string())
    );
}

#[test]
fn v8_attributed_string_setup_props_tolerates_inherited_callable_style() {
    // Regression: YouTube's setUpProps reads every declared prop off
    // `rawProps` and throws "Function props must be configured as STATIC, not
    // SIGNAL." when a SIGNAL prop holds a Function. Our bootstrap installs a
    // callable fallback `style` on Object.prototype, so a `yt-attributed-string`
    // whose `rawProps` lacks an own `style` resolves `rawProps.style` to that
    // inherited callable and trips the check. The custom-element hook wraps
    // rawProps in a Proxy that neutralizes such inherited callables.
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            class YtAttributedString extends HTMLElement {
                // A non-function own style so normalizeAttributedStringProps
                // leaves rawProps.style resolving to the inherited callable.
                get style() { return { color: 'red' }; }
                connectedCallback() {
                    this.rawProps = this.rawProps || {};
                    this.setUpProps();
                    this.__setup_ok__ = true;
                }
                setUpProps() {
                    var config = { style: 1, data: 1 };
                    for (var name in config) {
                        if (this.rawProps[name] instanceof Function) {
                            throw new Error(
                                'Function props must be configured as STATIC, not SIGNAL.');
                        }
                    }
                }
            }
            customElements.define('yt-attributed-string', YtAttributedString);
            var el = document.createElement('yt-attributed-string');
            document.body.appendChild(el);
            return String(el.__setup_ok__ === true);
            })()
            "#
        ),
        Ok("true".to_string())
    );
}

#[test]
fn v8_define_mirrors_into_native_registry() {
    let mut runtime = V8Runtime::new(blank_dom());

    // `customElements.define` should populate the native CE registry (Phase 1
    // of the native custom-element-reaction plan). Observed via the
    // `__aurora_ce_is_defined_native` binding.
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            class NativeMirror extends HTMLElement {
                static get observedAttributes() { return ['data-x']; }
                connectedCallback() {}
            }
            const before = String(__aurora_ce_is_defined_native('native-mirror'));
            customElements.define('native-mirror', NativeMirror);
            const after = String(__aurora_ce_is_defined_native('native-mirror'));
            return before + '|' + after;
            })()
            "#
        ),
        Ok("false|true".to_string())
    );
}

#[test]
fn v8_native_upgrade_candidates_collect_subtree_elements() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const host = document.createElement('div');
            const child = document.createElement('x-upgrade-candidate');
            host.appendChild(child);
            const tags = __aurora_ce_upgrade_candidates_native(host)
                .map(node => node.localName)
                .join(',');
            return tags;
            })()
            "#
        ),
        Ok("div,x-upgrade-candidate".to_string())
    );
}

#[test]
fn v8_native_reactions_fire_connected_callback_once() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    // With the CEReactions stack wired, appending a custom element fires
    // connectedCallback before appendChild returns. The backup queue drain is a
    // no-op afterward.
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            globalThis.__cc_calls__ = 0;
            class NativeConnect extends HTMLElement {
                connectedCallback() { globalThis.__cc_calls__++; }
            }
            customElements.define('native-connect', NativeConnect);
            const el = document.createElement('native-connect');
            document.body.appendChild(el);
            return String(globalThis.__cc_calls__);
            })()
            "#
        ),
        Ok("1".to_string()),
        "connectedCallback must fire synchronously at the end of appendChild"
    );

    assert!(!runtime.drain_custom_element_reactions());

    assert_eq!(
        runtime.eval_to_string("String(globalThis.__cc_calls__)"),
        Ok("1".to_string()),
        "connectedCallback must fire exactly once"
    );

    // Draining again with nothing queued is still a no-op.
    assert!(!runtime.drain_custom_element_reactions());
    assert_eq!(
        runtime.eval_to_string("String(globalThis.__cc_calls__)"),
        Ok("1".to_string())
    );
}

#[test]
fn v8_native_reactions_drain_in_mutation_observer_phase() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    // The production event-loop pump still reaches the MutationObserver
    // delivery phase, but the queue should already be empty because the
    // boundary drained synchronously.
    let _ = runtime.eval_to_string(
        r#"
        (() => {
        globalThis.__cc2__ = 0;
        class NcTwo extends HTMLElement { connectedCallback() { globalThis.__cc2__++; } }
        customElements.define('nc-two', NcTwo);
        document.body.appendChild(document.createElement('nc-two'));
        return 'ok';
        })()
        "#,
    );

    assert_eq!(
        runtime.eval_to_string("String(globalThis.__cc2__)"),
        Ok("1".to_string())
    );
    assert!(!runtime.deliver_mutation_records());
    assert_eq!(
        runtime.eval_to_string("String(globalThis.__cc2__)"),
        Ok("1".to_string()),
        "connectedCallback must not fire again during production mutation delivery"
    );
}

#[test]
fn v8_native_reactions_drain_synchronously() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    let _ = runtime.eval_to_string(
        r#"
        (() => {
        globalThis.__sync_calls__ = 0;
        class NativeSync extends HTMLElement {
            connectedCallback() { globalThis.__sync_calls__++; }
        }
        customElements.define('native-sync', NativeSync);
        const el = document.createElement('native-sync');
        document.body.appendChild(el);
        return 'ok';
        })()
        "#,
    );

    // After appendChild (which is a [CEReactions] boundary), connectedCallback should have run synchronously!
    assert_eq!(
        runtime.eval_to_string("String(globalThis.__sync_calls__)"),
        Ok("1".to_string()),
        "connectedCallback must fire synchronously at the end of the appendChild call"
    );
}

#[test]
fn v8_native_reactions_fire_disconnected_callback() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    // Removing a connected custom element fires disconnectedCallback at the
    // end of removeChild. The backup queue drain is a no-op afterward.
    let _ = runtime.eval_to_string(
        r#"
        (() => {
        globalThis.__dc_calls__ = 0;
        class NativeDisconnect extends HTMLElement {
            disconnectedCallback() { globalThis.__dc_calls__++; }
        }
        customElements.define('native-disconnect', NativeDisconnect);
        const el = document.createElement('native-disconnect');
        document.body.appendChild(el);
        document.body.removeChild(el);
        return 'ok';
        })()
        "#,
    );

    assert_eq!(
        runtime.eval_to_string("String(globalThis.__dc_calls__)"),
        Ok("1".to_string()),
        "disconnectedCallback must fire synchronously at the end of removeChild"
    );
    assert!(!runtime.drain_custom_element_reactions());
    assert_eq!(
        runtime.eval_to_string("String(globalThis.__dc_calls__)"),
        Ok("1".to_string()),
        "disconnectedCallback must fire exactly once"
    );
}

#[test]
fn v8_native_reactions_fire_attribute_changed_callback() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    // attributeChangedCallback fires synchronously for observed attributes, with
    // the old (null when absent) and new values, in mutation order.
    let _ = runtime.eval_to_string(
        r#"
        (() => {
        globalThis.__ac_log__ = [];
        class NativeAttr extends HTMLElement {
            static get observedAttributes() { return ['data-x']; }
            attributeChangedCallback(name, oldV, newV) {
                globalThis.__ac_log__.push(name + ':' + oldV + '->' + newV);
            }
        }
        customElements.define('native-attr', NativeAttr);
        const el = document.createElement('native-attr');
        document.body.appendChild(el);
        el.setAttribute('data-x', '1');   // observed: null -> 1
        el.setAttribute('data-y', '2');   // NOT observed -> no callback
        el.setAttribute('data-x', '3');   // observed: 1 -> 3
        el.removeAttribute('data-x');     // observed: 3 -> null
        return 'ok';
        })()
        "#,
    );

    assert_eq!(
        runtime.eval_to_string("globalThis.__ac_log__.join('|')"),
        Ok("data-x:null->1|data-x:1->3|data-x:3->null".to_string())
    );
    assert!(!runtime.drain_custom_element_reactions());
}

/// Genuine native-first upgrade for a `class ... extends HTMLElement`
/// definition: registered directly through `__aurora_ce_define_native`,
/// bypassing `customElements.define`, so the JS shim's own eager
/// `tryUpgrade` (which normally wins the construction race — see
/// `run_upgrade_constructor`'s `wrapper_is_already_upgraded` shortcut) never
/// touches the element. This is the code path every other native-reaction
/// test in this file avoids exercising, since they all go through
/// `customElements.define`/`document.createElement`, both of which the shim
/// intercepts.
///
/// Verifies the full spec order in one pass: the constructor runs, the
/// `HTMLElement` constructor sets the instance's prototype from
/// `new.target` (so `super()` inside a `class` upgrade adopts the existing
/// element rather than minting a detached one — the fix in
/// `PatchedHTMLElement`), and only *after* construction succeeds are
/// `attributeChangedCallback` (for the attribute already present) and
/// `connectedCallback` invoked, in that order, exactly once each.
#[test]
fn v8_native_first_upgrade_constructs_class_style_element_in_spec_order() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                globalThis.__order__ = [];
                const el = document.createElement('x-native-first');
                el.setAttribute('data-x', '1');

                class C extends HTMLElement {
                    constructor() {
                        super();
                        this.__marked__ = true;
                        globalThis.__order__.push('ctor');
                    }
                    static get observedAttributes() { return ['data-x']; }
                    attributeChangedCallback(name, oldV, newV) {
                        globalThis.__order__.push('attr:' + name + ':' + oldV + '->' + newV);
                    }
                    connectedCallback() { globalThis.__order__.push('connected'); }
                }
                // Native-only definition: the JS shim's registry never
                // learns about 'x-native-first', so its own eager upgrade
                // paths (document.createElement's patch, flushPending) find
                // no definition and leave the element alone.
                __aurora_ce_define_native('x-native-first', C);

                document.body.appendChild(el);

                return 'proto=' + (Object.getPrototypeOf(el) === C.prototype) +
                    ' marked=' + !!el.__marked__ +
                    ' state=' + __aurora_ce_state_native(el) +
                    ' order=' + globalThis.__order__.join(',');
            })()"#,
        ),
        Ok("proto=true marked=true state=custom order=ctor,attr:data-x:null->1,connected".to_string())
    );
}

/// The ES5-adapter shape (`function C() { HTMLElement.call(this); }`,
/// Polymer's `custom-elements-es5-adapter` pattern) must upgrade through the
/// same native construction-stack path as a `class` constructor, with no
/// special-casing. Companion to the `class`-style test above.
#[test]
fn v8_native_first_upgrade_constructs_es5_style_element_via_construction_stack() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                globalThis.__es5_order__ = [];
                const el = document.createElement('x-native-first-es5');

                function C() {
                    const self = HTMLElement.call(this) || this;
                    self.__marked__ = true;
                    globalThis.__es5_order__.push('ctor');
                    return self;
                }
                C.prototype = Object.create(HTMLElement.prototype);
                C.prototype.constructor = C;
                C.prototype.connectedCallback = function() {
                    globalThis.__es5_order__.push('connected');
                };
                __aurora_ce_define_native('x-native-first-es5', C);

                document.body.appendChild(el);

                return 'proto=' + (Object.getPrototypeOf(el) === C.prototype) +
                    ' marked=' + !!el.__marked__ +
                    ' state=' + __aurora_ce_state_native(el) +
                    ' order=' + globalThis.__es5_order__.join(',');
            })()"#,
        ),
        Ok("proto=true marked=true state=custom order=ctor,connected".to_string())
    );
}

/// A constructor that throws leaves the element `Failed`, per spec, and the
/// element keeps its original prototype rather than a half-adopted one —
/// without poisoning reactions queued for other elements in the same drain.
#[test]
fn v8_native_first_upgrade_marks_failed_on_throwing_constructor() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const bad = document.createElement('x-native-throws');
                const good = document.createElement('x-native-ok');

                class Bad extends HTMLElement {
                    constructor() { super(); throw new Error('nope'); }
                }
                globalThis.__good_connected__ = 0;
                class Good extends HTMLElement {
                    connectedCallback() { globalThis.__good_connected__++; }
                }
                __aurora_ce_define_native('x-native-throws', Bad);
                __aurora_ce_define_native('x-native-ok', Good);

                const container = document.createElement('div');
                document.body.appendChild(container);
                container.appendChild(bad);
                container.appendChild(good);

                return 'bad=' + __aurora_ce_state_native(bad) +
                    ' good=' + __aurora_ce_state_native(good) +
                    ' goodConnected=' + globalThis.__good_connected__;
            })()"#,
        ),
        Ok("bad=failed good=custom goodConnected=1".to_string())
    );
}

/// Stage 2: a genuinely native-constructed element must get the same Polymer
/// compensation (`created()`, `ready()`) a shim-constructed one gets from
/// `tryUpgrade` — previously `run_upgrade_constructor`'s real-construction
/// branch ran the constructor and moved straight to attribute/connect
/// reactions, calling neither. Both are legacy Polymer lifecycle hooks (not
/// part of the custom-element spec), invoked only when present on the
/// element, so this exercises `afterConstruct`/
/// `__aurora_ce_after_construct_native__` specifically.
#[test]
fn v8_native_first_upgrade_runs_polymer_created_and_ready_hooks() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                globalThis.__hooks_order__ = [];
                const el = document.createElement('x-native-polymer');

                class C extends HTMLElement {
                    constructor() { super(); globalThis.__hooks_order__.push('ctor'); }
                    created() { globalThis.__hooks_order__.push('created'); }
                    ready() { globalThis.__hooks_order__.push('ready'); }
                    connectedCallback() { globalThis.__hooks_order__.push('connected'); }
                }
                __aurora_ce_define_native('x-native-polymer', C);

                document.body.appendChild(el);

                return globalThis.__hooks_order__.join(',');
            })()"#,
        ),
        Ok("ctor,created,ready,connected".to_string())
    );
}

/// `__aurora_ce_has_pending_connected_reaction_native` must report only
/// pending `connectedCallback` reactions, not just "some reaction queued".
/// The JS shim (`connectUpgraded`) defers to the native trampoline when this
/// returns true; a false positive from a queued disconnected/attributeChanged
/// reaction would make it wait for a trampoline call that never comes, losing
/// the connectedCallback entirely. Probe the distinction mid-drain: while the
/// first old child's disconnectedCallback runs, its sibling still has a
/// disconnected reaction queued (must NOT count) and the incoming child has a
/// connected reaction queued (must count).
#[test]
fn v8_native_pending_connected_check_ignores_non_connect_reactions() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const probe = [];
                class XPendOld extends HTMLElement {
                    disconnectedCallback() {
                        if (this.id !== 'first') return;
                        probe.push('sibling=' + __aurora_ce_has_pending_connected_reaction_native(
                            globalThis.__pend_sibling__));
                        probe.push('incoming=' + __aurora_ce_has_pending_connected_reaction_native(
                            globalThis.__pend_incoming__));
                        probe.push('self=' + __aurora_ce_has_pending_connected_reaction_native(this));
                    }
                }
                let incomingConnected = 0;
                class XPendNew extends HTMLElement {
                    connectedCallback() { incomingConnected++; }
                }
                customElements.define('x-pend-old', XPendOld);
                customElements.define('x-pend-new', XPendNew);

                const container = document.createElement('div');
                document.body.appendChild(container);
                const first = document.createElement('x-pend-old');
                first.id = 'first';
                const sibling = document.createElement('x-pend-old');
                container.appendChild(first);
                container.appendChild(sibling);
                globalThis.__pend_sibling__ = sibling;
                const incoming = document.createElement('x-pend-new');
                globalThis.__pend_incoming__ = incoming;

                container.replaceChildren(incoming);
                return probe.join('|') + '|connected=' + incomingConnected;
            })()"#,
        ),
        Ok("sibling=false|incoming=true|self=false|connected=1".to_string()),
        "a queued disconnected reaction must not read as a pending connected reaction"
    );
}

/// YouTube's controller-extraction pattern: the registered class is a thin
/// shell whose prototype has no lifecycle methods, so define-time capture
/// finds nothing; the Polymer instance actually implementing the lifecycle is
/// created lazily on first access of `el.polymerController`. The native
/// insertion path must still fire connectedCallback for such elements —
/// enqueued as `Reaction::DynamicConnected` and resolved through the
/// controller at drain time, with the controller (not the shell) as `this`.
#[test]
fn v8_native_reactions_drive_polymer_controller_connected_callback() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const log = [];
                class XCtrlShell extends HTMLElement {
                    constructor() {
                        super();
                        const shell = this;
                        let ctrl = null;
                        Object.defineProperty(this, 'polymerController', {
                            configurable: true,
                            get() {
                                if (!ctrl) {
                                    ctrl = {
                                        hostElement: shell,
                                        connectedCallback() {
                                            log.push('connected host=' + this.hostElement.localName +
                                                ' this-is-ctrl=' + (this !== shell));
                                        }
                                    };
                                }
                                return ctrl;
                            }
                        });
                    }
                }
                customElements.define('x-ctrl-shell', XCtrlShell);
                const el = document.createElement('x-ctrl-shell');
                document.body.appendChild(el);
                return log.join('|');
            })()"#,
        ),
        Ok("connected host=x-ctrl-shell this-is-ctrl=true".to_string()),
        "native drain must resolve connectedCallback through polymerController"
    );
}

#[test]
fn v8_custom_element_connects_only_after_append() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const calls = [];
            class LateConnect extends HTMLElement {
                ready() {
                    calls.push('ready:' + this.isConnected);
                }
                connectedCallback() {
                    calls.push('connected:' + this.isConnected);
                }
            }
            customElements.define('late-connect', LateConnect);
            const el = document.createElement('late-connect');
            const before = calls.join(',');
            document.body.appendChild(el);
            return before + '|' + calls.join(',');
            })()
            "#
        ),
        Ok("ready:false|ready:false,connected:true".to_string())
    );
}

#[test]
fn v8_defers_stamped_child_upgrade_until_polymer_finishes_indexing() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const events = [];
            function StampChild() {
                events.push('child-ctor');
                HTMLElement.call(this);
            }
            StampChild.prototype = Object.create(HTMLElement.prototype);
            StampChild.prototype.constructor = StampChild;
            StampChild.prototype.ready = function() {
                events.push('child-ready');
            };
            StampChild.prototype.connectedCallback = function() {
                globalThis.stampChildConnected = this.isConnected;
            };

            function StampHost() { HTMLElement.call(this); }
            StampHost.prototype = Object.create(HTMLElement.prototype);
            StampHost.prototype.constructor = StampHost;
            StampHost.prototype._stampTemplate = function(template) {
                const fragment = document.importNode(template.content, true);
                events.length = 0;
                events.push('stamp-start');
                const child = fragment.childNodes[0];
                events.push('during:' + !!child.__ce_upgraded__);
                events.push('stamp-end');
                return fragment;
            };
            StampHost.prototype.ready = function() {
                const template = document.createElement('template');
                template.innerHTML = '<x-stamp-child></x-stamp-child>';
                const fragment = this._stampTemplate(template);
                events.push('after:' + !!fragment.firstChild.__ce_upgraded__);
                const child = fragment.firstChild;
                this.appendChild(fragment);
                events.push('parent:' + (this.firstChild === child));
            };

            customElements.define('x-stamp-child', StampChild);
            customElements.define('x-stamp-host', StampHost);
            document.body.appendChild(document.createElement('x-stamp-host'));
            return events.join('|');
            })()
            "#
        ),
        Ok(
            "stamp-start|during:false|stamp-end|child-ctor|child-ready|after:true|parent:true"
                .to_string()
        )
    );
    assert_eq!(
        runtime.eval_to_string("String(globalThis.stampChildConnected)"),
        Ok("true".to_string())
    );
}

#[test]
fn v8_restores_consumed_template_content_when_cached_metadata_is_reused() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
                function ReusedTemplateHost() { HTMLElement.call(this); }
                ReusedTemplateHost.prototype = Object.create(HTMLElement.prototype);
                ReusedTemplateHost.prototype.constructor = ReusedTemplateHost;
                ReusedTemplateHost.prototype._stampTemplate = function(template) {
                    return document.importNode(template.content, true);
                };
                ReusedTemplateHost.prototype.ready = function() {
                    const template = document.createElement('template');
                    template.innerHTML = '<yt-button-shape></yt-button-shape><tp-yt-paper-tooltip><slot></slot></tp-yt-paper-tooltip>';
                    template._templateInfo = { nodeInfoList: [{}, {}, {}] };
                    this._stampTemplate(template);
                    while (template.content.firstChild) {
                        template.content.removeChild(template.content.firstChild);
                    }
                    const second = this._stampTemplate(template);
                    this.result = second.childNodes.length + '|' + second.childNodes[0].localName + '|' +
                        second.childNodes[1].localName + '|' + second.childNodes[1].firstChild.localName;
                };
                customElements.define('x-reused-template-host', ReusedTemplateHost);
                const host = document.createElement('x-reused-template-host');
                document.body.appendChild(host);
                return host.result;
            })()
            "#
        ),
        Ok("2|yt-button-shape|tp-yt-paper-tooltip|slot".to_string())
    );
}

#[test]
fn v8_defers_controller_stamped_child_upgrade_until_indexing_finishes() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const events = [];
            function ControllerStampChild() {
                events.push('child-ctor');
                HTMLElement.call(this);
            }
            ControllerStampChild.prototype = Object.create(HTMLElement.prototype);
            ControllerStampChild.prototype.constructor = ControllerStampChild;
            ControllerStampChild.prototype.ready = function() {
                events.push('child-ready');
            };

            function ControllerStampHost() { HTMLElement.call(this); }
            ControllerStampHost.prototype = Object.create(HTMLElement.prototype);
            ControllerStampHost.prototype.constructor = ControllerStampHost;
            // Aurora sees this shell method, but YouTube delegates ready() to
            // a raw controller on `inst`. Its public polymerController proxy
            // intentionally does not expose private methods like _stampTemplate.
            ControllerStampHost.prototype._stampTemplate = function() {
                throw new Error('shell stamp should not run');
            };
            ControllerStampHost.prototype.created = function() {
                const host = this;
                this.inst = {
                    hostElement: host,
                    _stampTemplate(template) {
                        const fragment = document.importNode(template.content, true);
                        events.length = 0;
                        events.push('stamp-start');
                        const child = fragment.childNodes[0];
                        events.push('during:' + !!child.__ce_upgraded__);
                        events.push('stamp-end');
                        return fragment;
                    },
                    ready() {
                        const template = document.createElement('template');
                        template.innerHTML = '<x-controller-stamp-child></x-controller-stamp-child>';
                        const fragment = this._stampTemplate(template);
                        events.push('after:' + !!fragment.firstChild.__ce_upgraded__);
                        this.hostElement.appendChild(fragment);
                    },
                };
                this.polymerController = {
                    ready() { host.inst.ready(); },
                };
            };
            ControllerStampHost.prototype.ready = function() {
                this.polymerController.ready();
            };

            customElements.define('x-controller-stamp-child', ControllerStampChild);
            customElements.define('x-controller-stamp-host', ControllerStampHost);
            document.body.appendChild(document.createElement('x-controller-stamp-host'));
            return events.join('|');
            })()
            "#
        ),
        Ok(
            "stamp-start|during:false|stamp-end|child-ctor|child-ready|after:true"
                .to_string()
        )
    );
}

#[test]
fn v8_define_rejects_invalid_and_duplicate_names() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const results = [];
            function make() {
                function C() { HTMLElement.call(this); }
                C.prototype = Object.create(HTMLElement.prototype);
                C.prototype.constructor = C;
                return C;
            }
            try { customElements.define('nohyphen', make()); results.push('no-throw'); }
            catch (e) { results.push('invalid:' + e.name); }

            const first = make();
            customElements.define('x-dup', first);
            try { customElements.define('x-dup', make()); results.push('no-throw'); }
            catch (e) { results.push('dupname:' + e.name); }

            try { customElements.define('x-other', first); results.push('no-throw'); }
            catch (e) { results.push('dupctor:' + e.name); }
            return results.join('|');
            })()
            "#
        ),
        Ok("invalid:SyntaxError|dupname:NotSupportedError|dupctor:NotSupportedError".to_string())
    );
}

#[test]
fn v8_custom_elements_get_reads_the_native_registry() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            function C() { HTMLElement.call(this); }
            C.prototype = Object.create(HTMLElement.prototype);
            C.prototype.constructor = C;
            const before = customElements.get('x-lookup') === undefined;
            customElements.define('x-lookup', C);
            const after = customElements.get('x-lookup') === C;
            return 'before=' + before + ' after=' + after;
            })()
            "#
        ),
        Ok("before=true after=true".to_string())
    );
}

#[test]
fn v8_element_custom_state_tracks_upgrade() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const plain = document.createElement('div');
            const before = __aurora_ce_state_native(plain);
            const el = document.createElement('x-state-probe');
            const undef = __aurora_ce_state_native(el);
            function C() { HTMLElement.call(this); }
            C.prototype = Object.create(HTMLElement.prototype);
            C.prototype.constructor = C;
            customElements.define('x-state-probe', C);
            document.body.appendChild(el);
            return 'div=' + before + ' pre=' + undef + ' post=' + __aurora_ce_state_native(el);
            })()
            "#
        ),
        Ok("div=uncustomized pre=undefined post=custom".to_string())
    );
}

#[test]
fn v8_attach_internals_requires_form_associated_and_round_trips_validity() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const out = [];
            function Plain() { HTMLElement.call(this); }
            Plain.prototype = Object.create(HTMLElement.prototype);
            Plain.prototype.constructor = Plain;
            customElements.define('x-plain-internals', Plain);
            const plain = document.createElement('x-plain-internals');
            try { plain.attachInternals(); out.push('no-throw'); }
            catch (e) { out.push('plain:' + e.name); }

            function Formy() { HTMLElement.call(this); }
            Formy.formAssociated = true;
            Formy.prototype = Object.create(HTMLElement.prototype);
            Formy.prototype.constructor = Formy;
            customElements.define('x-form-internals', Formy);
            const el = document.createElement('x-form-internals');
            const internals = el.attachInternals();
            out.push('willValidate=' + internals.willValidate);
            out.push('validBefore=' + internals.checkValidity());
            internals.setFormValue('hello');
            internals.setValidity({ valueMissing: true }, 'required');
            out.push('validAfter=' + internals.checkValidity());
            internals.setValidity({});
            out.push('validReset=' + internals.checkValidity());
            try { el.attachInternals(); out.push('no-throw'); }
            catch (e) { out.push('second:' + e.name); }
            return out.join('|');
            })()
            "#
        ),
        Ok(concat!(
            "plain:NotSupportedError|willValidate=true|validBefore=true|",
            "validAfter=false|validReset=true|second:NotSupportedError"
        )
        .to_string())
    );
}

/// The shared preamble for the form lifecycle tests: a form-associated
/// definition that records every callback it receives into `out`.
const FORM_ASSOCIATED_DEFINITION: &str = r#"
    const out = [];
    function Formy() { HTMLElement.call(this); }
    Formy.formAssociated = true;
    Formy.prototype = Object.create(HTMLElement.prototype);
    Formy.prototype.constructor = Formy;
    Formy.prototype.formAssociatedCallback = function(form) {
        out.push('assoc=' + (form ? form.getAttribute('id') : 'null'));
    };
    Formy.prototype.formDisabledCallback = function(disabled) {
        out.push('disabled=' + disabled);
    };
    Formy.prototype.formResetCallback = function() { out.push('reset'); };
"#;

#[test]
fn v8_form_associated_callback_follows_the_form_owner() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(&format!(
            r#"
            (() => {{
            {FORM_ASSOCIATED_DEFINITION}
            customElements.define('x-owner-track', Formy);

            const form = document.createElement('form');
            form.setAttribute('id', 'f1');
            document.body.appendChild(form);

            const el = document.createElement('x-owner-track');
            form.appendChild(el);
            form.removeChild(el);
            return out.join('|');
            }})()
            "#
        )),
        Ok("assoc=f1|assoc=null".to_string())
    );
}

#[test]
fn v8_form_attribute_associates_across_the_document() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(&format!(
            r#"
            (() => {{
            {FORM_ASSOCIATED_DEFINITION}
            customElements.define('x-form-attr', Formy);

            const target = document.createElement('form');
            target.setAttribute('id', 'target');
            document.body.appendChild(target);
            const other = document.createElement('form');
            other.setAttribute('id', 'other');
            document.body.appendChild(other);

            // Inserted inside `other`, but the form attribute points elsewhere.
            const el = document.createElement('x-form-attr');
            el.setAttribute('form', 'target');
            other.appendChild(el);

            // Repointing the attribute moves the association.
            el.setAttribute('form', 'other');
            return out.join('|');
            }})()
            "#
        )),
        Ok("assoc=target|assoc=other".to_string())
    );
}

#[test]
fn v8_form_disabled_callback_follows_an_ancestor_fieldset() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(&format!(
            r#"
            (() => {{
            {FORM_ASSOCIATED_DEFINITION}
            customElements.define('x-disabled-track', Formy);

            const fieldset = document.createElement('fieldset');
            document.body.appendChild(fieldset);
            const el = document.createElement('x-disabled-track');
            fieldset.appendChild(el);

            fieldset.setAttribute('disabled', '');
            const willValidate = el.attachInternals().willValidate;
            fieldset.removeAttribute('disabled');
            return out.join('|') + '|willValidate=' + willValidate;
            }})()
            "#
        )),
        Ok("disabled=true|disabled=false|willValidate=false".to_string())
    );
}

#[test]
fn v8_form_reset_fires_reset_callback_on_owned_custom_elements() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(&format!(
            r#"
            (() => {{
            {FORM_ASSOCIATED_DEFINITION}
            customElements.define('x-reset-track', Formy);

            const form = document.createElement('form');
            form.setAttribute('id', 'f1');
            document.body.appendChild(form);
            const owned = document.createElement('x-reset-track');
            form.appendChild(owned);

            // Outside the form, so it must not be reset by it.
            const unowned = document.createElement('x-reset-track');
            document.body.appendChild(unowned);

            out.length = 0;
            form.reset();
            return out.join('|');
            }})()
            "#
        )),
        Ok("reset".to_string())
    );
}

#[test]
fn v8_element_internals_form_resolves_live() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(&format!(
            r#"
            (() => {{
            {FORM_ASSOCIATED_DEFINITION}
            customElements.define('x-internals-form', Formy);

            const el = document.createElement('x-internals-form');
            const internals = el.attachInternals();
            // attachInternals runs while the element is still disconnected,
            // which is exactly when a snapshot would go stale.
            const before = internals.form === null;
            const form = document.createElement('form');
            document.body.appendChild(form);
            form.appendChild(el);
            return 'before=' + before + ' after=' + (internals.form === form);
            }})()
            "#
        )),
        Ok("before=true after=true".to_string())
    );
}

#[test]
fn v8_custom_element_calls_attached_when_connected_callback_missing() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            function LegacyAttached() {
                HTMLElement.call(this);
            }
            LegacyAttached.prototype = Object.create(HTMLElement.prototype);
            LegacyAttached.prototype.constructor = LegacyAttached;
            LegacyAttached.prototype.attached = function() {
                this.__attached_count__ = (this.__attached_count__ || 0) + 1;
            };
            customElements.define('legacy-attached', LegacyAttached);
            const el = document.createElement('legacy-attached');
            const before = el.__attached_count__ || 0;
            document.body.appendChild(el);
            document.body.appendChild(el);
            return before + '|' + el.__attached_count__;
            })()
            "#
        ),
        Ok("0|1".to_string())
    );
}

#[test]
fn v8_custom_element_runs_before_register_and_created() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            function PolymerLike() {
                HTMLElement.call(this);
            }
            PolymerLike.prototype = Object.create(HTMLElement.prototype);
            PolymerLike.prototype.constructor = PolymerLike;
            PolymerLike.prototype.beforeRegister = function() {
                this.__before_register_count__ = (this.__before_register_count__ || 0) + 1;
            };
            PolymerLike.prototype.created = function() {
                this.__created_count__ = (this.__created_count__ || 0) + 1;
            };
            customElements.define('polymer-like', PolymerLike);
            const el = document.createElement('polymer-like');
            return [el.__before_register_count__, el.__created_count__].join('|');
            })()
            "#
        ),
        Ok("1|1".to_string())
    );
}

#[test]
fn v8_custom_element_before_register_can_be_invoked_on_upgrade() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            function LateBeforeRegister() {
                HTMLElement.call(this);
            }
            LateBeforeRegister.prototype = Object.create(HTMLElement.prototype);
            LateBeforeRegister.prototype.constructor = LateBeforeRegister;
            customElements.define('late-before-register', LateBeforeRegister);
            LateBeforeRegister.prototype.beforeRegister = function() {
                this.__before_register_count__ = (this.__before_register_count__ || 0) + 1;
            };
            const el = document.createElement('late-before-register');
            return String(el.__before_register_count__);
            })()
            "#
        ),
        Ok("1".to_string())
    );
}

#[test]
fn v8_append_child_moves_node_out_of_previous_parent() {
    // Regression: inserting a node that already has a parent must *move* it
    // (detach from the old parent first). Without this, the node stays parented
    // in two places, so `oldParent.firstChild` never changes — which spun
    // YouTube's `while (el.firstChild) frag.appendChild(el.firstChild)` icon
    // clear-loop forever.
    let dom = Parser::new(
        "<html><body><div id='a'><span id='c'></span></div><div id='b'></div></body></html>",
    )
    .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const a = document.getElementById('a');
            const b = document.getElementById('b');
            const c = document.getElementById('c');
            b.appendChild(c);
            return [
                a.firstChild === null,        // c left a
                a.childNodes.length,          // a is now empty
                b.firstChild === c,           // c is now in b
                c.parentNode === b
            ].join('|');
            })()
            "#
        ),
        Ok("true|0|true|true".to_string())
    );

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const host = document.getElementById('a');
            const frag = document.createDocumentFragment();
            const one = document.createElement('span');
            const two = document.createElement('em');
            frag.appendChild(one);
            frag.appendChild(two);
            host.appendChild(frag);
            return [
                frag.childNodes.length,
                host.childNodes.length,
                host.firstChild === one,
                host.lastChild === two,
                one.parentNode === host,
                two.parentNode === host
            ].join('|');
            })()
            "#
        ),
        Ok("0|2|true|true|true|true".to_string())
    );

    // A clear-and-rebuild loop must terminate.
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const b = document.getElementById('b');
            const frag = document.createDocumentFragment();
            let guard = 0;
            while (b.firstChild) {
                frag.appendChild(b.firstChild);
                if (++guard > 1000) return 'runaway';
            }
            return frag.childNodes.length + '|' + (b.firstChild === null);
            })()
            "#
        ),
        Ok("1|true".to_string())
    );
}

#[test]
fn v8_polymer_id_map_exposes_direct_and_camel_aliases_before_ready() {
    // YouTube/Polymer templates expose stamped ids both through `this.$` and as
    // direct host properties before `ready()` runs. `ytd-watch-flexy.ready()`
    // relies on `this.primary`/`this.secondary`; dashed ids are commonly read as
    // camelCase properties such as `this.playerContainer`.
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            function TestPolymerIdMap() {
                HTMLElement.call(this);
                const root = this.attachShadow({ mode: 'open' });
                const primary = document.createElement('div');
                primary.id = 'primary';
                const player = document.createElement('div');
                player.id = 'player-container';
                root.appendChild(primary);
                root.appendChild(player);
            }
            TestPolymerIdMap.prototype = Object.create(HTMLElement.prototype);
            TestPolymerIdMap.prototype.constructor = TestPolymerIdMap;
            TestPolymerIdMap.prototype.ready = function() {
                globalThis.idMapProbe = [
                    this.$ && this.$.primary === this.primary,
                    this.primary && this.primary.id,
                    this.$ && this.$['player-container'] === this.playerContainer,
                    this.playerContainer && this.playerContainer.id
                ].join('|');
            };
            customElements.define('test-polymer-id-map', TestPolymerIdMap);
            document.createElement('test-polymer-id-map');
            const tpl = document.createElement('template');
            tpl.innerHTML = '<div id="secondary"></div><div id="player-container"></div>';
            function TestLazyIdMap() { HTMLElement.call(this); }
            TestLazyIdMap.template = tpl;
            TestLazyIdMap.prototype = Object.create(HTMLElement.prototype);
            TestLazyIdMap.prototype.constructor = TestLazyIdMap;
            TestLazyIdMap.prototype.ready = function() {
                const t = this.constructor.template;
                const root = this.attachShadow({ mode: 'open' });
                root.appendChild(t.content.cloneNode(true));
                globalThis.lazyIdMapProbe = [
                    this.secondary && this.secondary.id,
                    this.playerContainer && this.playerContainer.id,
                    typeof this.secondary.addEventListener
                ].join('|');
            };
            customElements.define('test-lazy-id-map', TestLazyIdMap);
            document.createElement('test-lazy-id-map');
            return globalThis.idMapProbe + '||' + globalThis.lazyIdMapProbe;
            })()
            "#
        ),
        Ok("true|primary|true|player-container||secondary|player-container|function".to_string())
    );
}

#[test]
fn v8_supports_prepend_before_after_mutation_helpers() {
    let dom =
        Parser::new("<html><body><div id='root'><span id='middle'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const root = document.getElementById('root');
            const middle = document.getElementById('middle');
            root.prepend('lead');
            middle.before(document.createElement('before'));
            middle.after(document.createElement('after'), 'tail');
            Array.prototype.map.call(root.childNodes, n => n.nodeType === 1 ? n.localName : n.textContent).join('|')
            "#
        ),
        Ok("lead|before|span|after|tail".to_string())
    );
}

#[test]
fn v8_supports_insert_adjacent_and_replace_helpers() {
    let dom =
        Parser::new("<html><body><div id='root'><span id='middle'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const root = document.getElementById('root');
            const middle = document.getElementById('middle');
            middle.insertAdjacentHTML('beforebegin', '<b id="before">B</b>');
            middle.insertAdjacentText('afterend', 'tail');
            const after = document.createElement('i');
            after.id = 'after';
            middle.insertAdjacentElement('afterend', after);
            return Array.prototype.map.call(root.childNodes, n => n.nodeType === 1 ? n.localName + ':' + (n.id || '') : n.textContent).join('|');
            })()
            "#
        ),
        Ok("b:before|span:middle|i:after|tail".to_string())
    );

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const root = document.getElementById('root');
            root.replaceChildren('start', document.createElement('main'));
            return root.firstChild.textContent + '|' + root.lastElementChild.localName + '|' + root.childElementCount;
            })()
            "#
        ),
        Ok("start|main|1".to_string())
    );

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const main = document.querySelector('main');
            main.replaceWith(document.createElement('aside'), 'end');
            return Array.prototype.map.call(document.getElementById('root').childNodes, n => n.nodeType === 1 ? n.localName : n.textContent).join('|');
            })()
            "#
        ),
        Ok("start|aside|end".to_string())
    );
}

#[test]
fn v8_template_inner_html_parses_as_fragment() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const template = document.createElement('template');
            template.innerHTML = '<!--scope--><yt-guide-manager id="guide-service"></yt-guide-manager><div id="x">y</div>';
            const first = template.content.firstChild;
            return [
                template.content.childNodes.length,
                first.nodeType,
                first.localName || first.nodeName,
                first.nextSibling ? first.nextSibling.localName : 'none',
                template.innerHTML.indexOf('<html') === -1
            ].join('|');
            })()
            "#
        ),
        Ok("2|1|yt-guide-manager|div|true".to_string())
    );
}

#[test]
fn v8_template_content_children_keep_parent_links() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const template = document.createElement('template');
            template.innerHTML = '<div id="a"><span id="b"></span></div><p id="c"></p>';
            const original = template.content;
            const cloned = original.cloneNode(true);
            const firstOriginal = original.firstChild;
            const firstClone = cloned.firstChild;
            return [
                firstOriginal.parentNode === original,
                firstOriginal.firstChild.parentNode === firstOriginal,
                firstClone.parentNode === cloned,
                firstClone.firstChild.parentNode === firstClone
            ].join('|');
            })()
            "#
        ),
        Ok("true|true|true|true".to_string())
    );
}

#[test]
fn v8_template_content_parent_and_siblings_are_visible() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const template = document.createElement('template');
            template.innerHTML = '<div id="a"></div><p id="b"></p>';
            const content = template.content;
            const first = content.firstChild;
            const second = first.nextSibling;
            return [
                first.parentNode === content,
                second.parentNode === content,
                first.nextSibling === second,
                second.previousSibling === first
            ].join('|');
            })()
            "#
        ),
        Ok("true|true|true|true".to_string())
    );
}

#[test]
fn v8_dom_constructor_prototypes_forward_to_native_wrappers() {
    let dom =
        Parser::new("<html><body><div id='root'><span id='child'></span></div></body></html>")
            .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const root = document.getElementById('root');
            const child = HTMLElement.prototype.querySelector.call(root, '#child');
            HTMLElement.prototype.setAttribute.call(child, 'data-proto', 'ok');
            child.__shady_setAttribute('data-shady', 'yes');
            const clone = Node.prototype.cloneNode.call(child, false);
            const rootNode = child.__shady_getRootNode();
            const all = DocumentFragment.prototype.querySelectorAll.call(document.createRange().createContextualFragment('<a></a><b></b>'), 'a,b');
            return [child.id, child.getAttribute('data-proto'), child.getAttribute('data-shady'), clone.localName, rootNode.nodeType, root.__shady_native_contains(child), all.length].join('|');
            })()
            "#
        ),
        Ok("child|ok|yes|span|1|true|2".to_string())
    );
}

#[test]
fn v8_exposes_youtube_bootstrap_constructor_and_config_shape() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            [
                Object.getOwnPropertyNames(Element.prototype).indexOf('style') >= 0,
                typeof document.createElement('div').style.cssText,
                // A plain object's `style` is an ordinary property: the old
                // callable `Object.prototype.style` shim was removed (it leaked
                // into Polymer's rawProps.style), so this stays `undefined:undefined`.
                (() => { const o = {}; o.style = 'display: block'; return o.style.cssText + ':' + typeof o.style.call; })(),
                (ytcfg.set({WEB_PLAYER_CONTEXT_CONFIGS: {OTHER: {contextId: 'OTHER'}}}), typeof ytcfg.get('WEB_PLAYER_CONTEXT_CONFIGS').WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH.serializedExperimentIds),
                ytcfg.get('WEB_PLAYER_CONTEXT_CONFIGS').WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH.serializedExperimentFlags,
                ytcfg.get('WEB_PLAYER_CONTEXT_CONFIGS').WEB_PLAYER_CONTEXT_CONFIG_ID_UNKNOWN.serializedExperimentFlags,
                ({a: {compactVideoRenderer: true}}).some(v => v.compactVideoRenderer),
                (() => { const wm = new WeakMap(); wm.set('dom-repeat', 7); return wm.get('dom-repeat'); })(),
                typeof ytcfg.set
            ].join('|')
            "#
        ),
        Ok("true|string|undefined:undefined|string|0|0|true|7|function".to_string())
    );
}

#[test]
fn v8_supports_svg_namespace_attribute_methods() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttributeNS(null, 'd', 'M0 0L1 1');
            path.setAttributeNS('http://www.w3.org/1999/xlink', 'href', '#icon');
            const before = [
                path.getAttribute('d'),
                path.getAttributeNS(null, 'd'),
                path.getAttributeNS('http://www.w3.org/1999/xlink', 'href'),
                path.hasAttributeNS(null, 'd'),
                typeof path.style.cssText
            ].join('|');
            path.removeAttributeNS(null, 'd');
            return before + '|' + path.hasAttribute('d');
            })()
            "#
        ),
        Ok("M0 0L1 1|M0 0L1 1|#icon|true|string|false".to_string())
    );
}

#[test]
fn v8_create_element_ns_exposes_svg_shape() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
                const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
                const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                return [
                    svg.namespaceURI,
                    svg instanceof SVGElement,
                    path.namespaceURI,
                    path instanceof SVGElement
                ].join('|');
            })()
            "#
        ),
        Ok("http://www.w3.org/2000/svg|true|http://www.w3.org/2000/svg|true".to_string())
    );
}

#[test]
fn v8_element_style_survives_custom_element_prototype_swap() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            // The upgrade swaps the wrapper's prototype to the class; `style`
            // must still resolve (via Element.prototype) to the native
            // declaration. A class that *overrides* `style` wins, as in
            // browsers — see v8_element_style_is_a_prototype_accessor_not_an_own_property.
            class StyleProbe extends HTMLElement {
                connectedCallback() {
                    this.style.cssText = 'color: red';
                    this.__probe = this.getAttribute('style') + '|' + typeof this.style.cssText;
                }
            }
            customElements.define('style-probe', StyleProbe);
            const el = document.createElement('style-probe');
            document.body.appendChild(el);
            return el.__probe;
            })()
            "#
        ),
        Ok("color: red|string".to_string())
    );
}

#[test]
fn v8_exposes_common_element_metric_and_attribute_probes() {
    let dom = Parser::new("<html><body><div id='box' hidden data-token='abc'></div></body></html>")
        .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const box = document.getElementById('box');
            [
                box.hasAttributes(),
                box.hidden,
                box.tabIndex,
                box.offsetWidth,
                box.clientHeight,
                box.scrollTop,
                typeof box.normalize
            ].join('|')
            "#
        ),
        Ok("true|true|0|0|0|0|function".to_string())
    );
}

#[test]
fn v8_connected_custom_elements_get_metric_fallbacks() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const regular = document.createElement('div');
            const custom = document.createElement('metric-probe');
            const before = custom.clientWidth;
            document.body.appendChild(custom);
            return [
                regular.clientWidth,
                before,
                custom.clientWidth > 0,
                custom.offsetWidth === custom.clientWidth
            ].join('|');
            })()
            "#
        ),
        Ok("0|0|true|true".to_string())
    );
}

#[test]
fn v8_hydrates_dataset_from_data_attributes() {
    let dom = Parser::new(
        "<html><body><div id='item' data-video-id='abc' data-session='xyz'></div></body></html>",
    )
    .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const item = document.getElementById('item');
            item.dataset.videoId + '|' + item.dataset.session
            "#
        ),
        Ok("abc|xyz".to_string())
    );
}

#[test]
fn v8_decorates_video_elements_with_media_surface() {
    let dom = Parser::new("<html><body><video id='player' src='clip.mp4'></video></body></html>")
        .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const player = document.getElementById('player');
            [
                typeof player.play,
                typeof player.pause,
                player.canPlayType('video/mp4'),
                player.videoWidth,
                player.readyState,
                player.buffered.length
            ].join('|')
            "#
        ),
        Ok("function|function|probably|640|4|0".to_string())
    );

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const player = document.getElementById('player');
            player.play();
            return String(player.paused);
            })()
            "#
        ),
        Ok("false".to_string())
    );
}

#[test]
fn v8_supports_element_attributes_named_node_map() {
    let dom =
        Parser::new("<html><body><div id='app' class='one'></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const el = document.getElementById('app');
            const attrs = el.attributes;
            const before = [
                attrs.length,
                attrs.getNamedItem('id').value,
                attrs.item(0).name ? 'item' : 'missing'
            ].join(':');
            attrs.removeNamedItem('id');
            attrs.setNamedItem({ name: 'data-ready', value: before });
            [
                before,
                el.hasAttribute('id'),
                el.getAttribute('data-ready'),
                el.attributes.length
            ].join('|')
            "#
        ),
        Ok("2:app:item|false|2:app:item|2".to_string())
    );
}

#[test]
fn v8_supports_storage_and_fetch() {
    let mut runtime = V8Runtime::new(blank_dom());

    // localStorage
    runtime
        .execute("localStorage.setItem('test', 'value');")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("localStorage.getItem('test')"),
        Ok("value".to_string())
    );
    runtime.execute("localStorage.removeItem('test');").unwrap();
    assert_eq!(
        runtime.eval_to_string("localStorage.getItem('test')"),
        Ok("null".to_string())
    );

    // fetch polyfill (basic check)
    assert_eq!(
        runtime.eval_to_string("typeof fetch"),
        Ok("function".to_string())
    );
    // Keep this deterministic: the bridge itself has a transport-level test,
    // while this checks the browser-facing Promise surface.
    runtime
        .execute(
            r#"
            globalThis.__aurora_fetch_start__ = function() { return 1; };
            globalThis.__aurora_fetch_poll__ = function() {
                return { ok: true, status: 200, statusText: 'OK', body: '', headers: '' };
            };
            "#,
        )
        .unwrap();
    runtime
        .execute("globalThis.p = fetch('https://google.com')")
        .unwrap();
    assert_eq!(
        runtime.eval_to_string("globalThis.p instanceof Promise"),
        Ok("true".to_string())
    );

    // atob / btoa
    assert_eq!(
        runtime.eval_to_string("btoa('hello')"),
        Ok("aGVsbG8=".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("atob('aGVsbG8=')"),
        Ok("hello".to_string())
    );
}

#[test]
fn v8_fetch_and_xhr_forward_headers_and_preserve_http_status() {
    let mut runtime = V8Runtime::new(blank_dom());

    runtime
        .execute(
            r#"
            globalThis.__networkCalls = [];
            globalThis.__networkResult = {
                ok: true,
                status: 418,
                statusText: "I'm a teapot",
                body: '{"error":"teapot"}',
                headers: 'content-type=application%2Fjson&x-test=present'
            };
            globalThis.__recordNetworkCall = function(url, method, body, headers) {
                __networkCalls.push([url, method, body, headers].join('|'));
                return __networkResult;
            };
            globalThis.__aurora_fetch_sync__ = __recordNetworkCall;
            globalThis.__aurora_fetch_start__ = function(url, method, body, headers) {
                __recordNetworkCall(url, method, body, headers);
                return 7;
            };
            globalThis.__aurora_fetch_poll__ = function() { return __networkResult; };

            var request = new Request('/youtubei/v1/browse', {
                method: 'POST',
                body: '{"browseId":"FEwhat_to_watch"}',
                headers: { 'X-Youtube-Client-Name': '1' }
            });
            globalThis.__fetchPromise = fetch(request, {
                headers: new Headers([['X-Youtube-Client-Version', '2.20260619']])
            });

            var xhr = new XMLHttpRequest();
            xhr.open('POST', '/youtubei/v1/next', false);
            xhr.setRequestHeader('Content-Type', 'application/json+protobuf');
            xhr.setRequestHeader('X-Goog-Visitor-Id', 'visitor token');
            xhr.send('{}');
            globalThis.__xhrSummary = [
                xhr.status,
                xhr.responseText,
                xhr.getResponseHeader('content-type'),
                xhr.getResponseHeader('x-test'),
                xhr.getAllResponseHeaders().indexOf('x-test: present') >= 0
            ].join('|');
            __fetchPromise.then(function(response) {
                globalThis.__fetchSummary = [
                    response.status,
                    response.statusText,
                    response.ok,
                    response.headers.get('content-type')
                ].join('|');
            });
            "#,
        )
        .unwrap();
    runtime.tick(std::time::Instant::now() + std::time::Duration::from_millis(1));

    assert_eq!(
        runtime.eval_to_string("__networkCalls[0]"),
        Ok("about:/youtubei/v1/browse|POST|{\"browseId\":\"FEwhat_to_watch\"}|x-youtube-client-name=1&x-youtube-client-version=2.20260619".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("__networkCalls[1]"),
        Ok("about:/youtubei/v1/next|POST|{}|content-type=application%2Fjson%2Bprotobuf&x-goog-visitor-id=visitor%20token".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("__xhrSummary"),
        Ok("418|{\"error\":\"teapot\"}|application/json|present|true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("__fetchSummary"),
        Ok("418|I'm a teapot|false|application/json".to_string())
    );
}

#[test]
fn v8_url_polyfill_resolves_relative_urls_and_query_params() {
    let mut runtime = V8Runtime::new(blank_dom());

    assert_eq!(
        runtime.eval_to_string(
            r#"
            const url = new URL('/watch?v=abc123&feature=share', 'https://www.youtube.com/feed/subscriptions?persist=1');
            url.searchParams.set('feature', 'related');
            url.searchParams.append('t', '42');
            const relative = new URL('../shorts/xyz?si=token', 'https://www.youtube.com/watch/');
            const params = new URLSearchParams('a=1&a=2&empty=');
            [
                url.href,
                url.origin,
                url.hostname,
                url.searchParams.get('v'),
                url.searchParams.get('feature'),
                relative.href,
                params.getAll('a').join(','),
                String(params.has('empty'))
            ].join('|')
            "#
        ),
        Ok("https://www.youtube.com/watch?v=abc123&feature=related&t=42|https://www.youtube.com|www.youtube.com|abc123|related|https://www.youtube.com/shorts/xyz?si=token|1,2|true".to_string())
    );
}

#[test]
fn v8_supports_document_structure_and_screen() {
    let dom = Parser::new(
        "<html><head><title>Test Title</title></head><body><div id='content'></div></body></html>",
    )
    .parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string("document.title"),
        Ok("Test Title".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.body.tagName"),
        Ok("BODY".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.head.tagName"),
        Ok("HEAD".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.documentElement.tagName"),
        Ok("HTML".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.defaultView === window"),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.nodeType"),
        Ok("9".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.nodeName"),
        Ok("#document".to_string())
    );

    // screen stub — desktop dimensions (matches the 1440x1024 viewport the V8
    // bootstrap reports so sites like YouTube take their desktop layout path).
    assert_eq!(
        runtime.eval_to_string("screen.width"),
        Ok("1440".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("screen.height"),
        Ok("1024".to_string())
    );
}

#[test]
fn v8_exposes_iframe_document_surface() {
    let dom = Parser::new("<html><head></head><body></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);

    assert_eq!(
        runtime.eval_to_string(
            "var iframe = document.createElement('iframe'); !!(iframe.contentDocument && iframe.contentDocument.documentElement && iframe.contentWindow && iframe.contentWindow.document)"
        ),
        Ok("true".to_string())
    );
    assert_eq!(
        runtime.eval_to_string(
            "var iframe = document.createElement('iframe'); iframe.contentDocument.documentElement.tagName"
        ),
        Ok("HTML".to_string())
    );
}

#[test]
fn engines_hot_swap_behind_the_js_runtime_trait() {
    let dom = blank_dom();
    let mut runtime: Box<dyn JsRuntime> = create_runtime(EngineKind::V8, &dom, None).unwrap();

    runtime
        .execute("globalThis.answer = 6 * 7;")
        .unwrap_or_else(|e| panic!("V8 failed to execute: {e}"));
    // Observable through the trait alone: a wrong value would throw and surface
    // as Err from execute.
    runtime
        .execute("if (globalThis.answer !== 42) throw new Error('engine state lost');")
        .unwrap_or_else(|e| panic!("V8 lost state across execute calls: {e}"));
    assert!(
        runtime.execute("syntax error here").is_err(),
        "V8 should surface compile errors"
    );
}

#[test]
fn v8_text_node_move_detaches_from_previous_parent() {
    let html = "<html><body><div id='p1'>hello</div><div id='p2'></div></body></html>";
    let mut runtime = V8Runtime::new(Parser::new(html).parse_document());

    runtime
        .execute(
            r#"
        const p1 = document.getElementById('p1');
        const p2 = document.getElementById('p2');
        const text = p1.firstChild;
        p2.appendChild(text);
    "#,
        )
        .unwrap();

    // The text node should no longer be a child of p1
    assert_eq!(
        runtime.eval_to_string("document.getElementById('p1').childNodes.length"),
        Ok("0".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.getElementById('p2').childNodes.length"),
        Ok("1".to_string())
    );
    assert_eq!(
        runtime.eval_to_string("document.getElementById('p2').firstChild.textContent"),
        Ok("hello".to_string())
    );
}

#[test]
fn v8_detached_attribute_mutation_delivers_observer_records_without_rebuild() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
        .expect("render document should build");
    let mut runtime = V8Runtime::with_render_document(dom, Some(Rc::new(RefCell::new(render_doc))));

    runtime
        .execute(
            r#"
            globalThis.hits = 0;
            const t = document.createElement('div');
            const mo = new MutationObserver((recs) => { globalThis.hits += recs.length; });
            mo.observe(t, { attributes: true });
            t.setAttribute('data-x', '1');
            "#,
        )
        .unwrap();

    assert!(runtime.deliver_mutation_records());
    assert_eq!(
        runtime.eval_to_string("globalThis.hits"),
        Ok("1".to_string())
    );
    assert_eq!(
        runtime.take_snapshot_rebuild_reason(),
        None
    );
}

// ─── Task 4.2: core Shadow DOM semantics ────────────────────────────────────

fn runtime_with_render_doc(dom: crate::dom::NodePtr) -> V8Runtime {
    let identity = test_identity();
    let render_doc = BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
        .expect("render document should build");
    V8Runtime::with_render_document(dom, Some(Rc::new(RefCell::new(render_doc))))
}

#[test]
fn v8_set_text_content_on_text_node_syncs_without_reborrow_panic() {
    // Regression: the SetTextContent dispatcher held `node.borrow_mut()` across
    // the render-sync call. `sync_text_node` re-borrows the node via `parent_ptr`
    // / `is_shadow_root_node`, which aborted the process with "RefCell already
    // mutably borrowed". YouTube hit this immediately because Polymer rewrites
    // `textContent` on text nodes during hydration. Requires a render document so
    // the sync path actually runs.
    let dom = Parser::new("<html><body><div id='t'>before</div></body></html>").parse_document();
    let mut runtime = runtime_with_render_doc(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const t = document.getElementById('t');
                const textNode = t.firstChild;
                textNode.textContent = 'after';
                return t.textContent;
            })()"#
        ),
        Ok("after".to_string())
    );
}

#[test]
fn v8_layout_accessors_read_blitz_layout() {
    // Phase 8.2: getBoundingClientRect and the native metric bridge report the
    // real Blitz/Stylo border-box size, not the old 0 stub.
    let dom = Parser::new(
        "<html><body><div id='box' style='width:200px;height:50px'></div></body></html>",
    )
    .parse_document();
    let mut runtime = runtime_with_render_doc(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const el = document.getElementById('box');
                const r = el.getBoundingClientRect();
                return [
                    r.width, r.height,
                    el.__aurora_metric__('offsetWidth'),
                    el.__aurora_metric__('clientHeight'),
                ].join('|');
            })()"#
        ),
        Ok("200|50|200|50".to_string())
    );
}

#[test]
fn v8_offset_position_accessors_read_blitz_layout() {
    // The `offsetTop`/`offsetLeft` getters (installed in v8_post.js) read the
    // real Blitz/Stylo box origin via __aurora_metric__ instead of the old
    // static 0 data property. Position is document-relative.
    let dom = Parser::new(
        "<html><body style='margin:0'><div id='box' style='position:absolute;left:40px;top:60px;width:100px;height:20px'></div></body></html>",
    )
    .parse_document();
    let mut runtime = runtime_with_render_doc(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const el = document.getElementById('box');
                return [el.offsetLeft, el.offsetTop].join('|');
            })()"#
        ),
        Ok("40|60".to_string())
    );
}

#[test]
fn v8_element_from_point_hits_blitz_layout() {
    // Phase 8.2 follow-up: document.elementFromPoint hit-tests the real Blitz
    // layout instead of the old `return null` stub.
    let dom = Parser::new(
        "<html><body><div id='box' style='position:absolute;left:0;top:0;width:200px;height:100px'></div></body></html>",
    )
    .parse_document();
    let mut runtime = runtime_with_render_doc(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const hit = document.elementFromPoint(20, 20);
                return hit ? (hit.id || hit.tagName) : 'null';
            })()"#
        ),
        Ok("box".to_string())
    );
}

#[test]
fn v8_element_from_point_returns_null_without_render_document() {
    let dom = Parser::new("<html><body><div id='box'></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);
    assert_eq!(
        runtime.eval_to_string("String(document.elementFromPoint(10, 10))"),
        Ok("null".to_string())
    );
}

#[test]
fn v8_layout_accessors_zero_without_render_document() {
    // No render document attached → no Blitz layout → metrics report 0 (the JS
    // heuristic fallback then takes over in v8_post.js, not exercised here).
    let dom = Parser::new("<html><body><div id='box'></div></body></html>").parse_document();
    let mut runtime = V8Runtime::new(dom);
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const el = document.getElementById('box');
                const r = el.getBoundingClientRect();
                return [r.width, r.height, el.__aurora_metric__('offsetWidth')].join('|');
            })()"#
        ),
        Ok("0|0|0".to_string())
    );
}

#[test]
fn v8_attach_shadow_creates_distinct_shadow_root() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const root = host.attachShadow({ mode: 'open' });
                return [
                    root !== host,                 // distinct object
                    host.shadowRoot === root,      // host points back at root
                    root.host === host,            // root points back at host
                    root.parentNode === null,      // shadow root is not a light child
                    root.host === host,            // parentNode read preserved host link
                    root.nodeType === 11,          // DocumentFragment node type
                    root.mode === 'open',
                ].join('|');
            })()"#
        ),
        Ok("true|true|true|true|true|true|true".to_string())
    );
}

#[test]
fn v8_shadow_children_are_not_light_dom_children() {
    // A node appended to the shadow root must not appear among the host's light
    // DOM children, and must appear among the shadow root's children.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const light = document.createElement('p');
                host.appendChild(light);
                const root = host.attachShadow({ mode: 'open' });
                const shadow = document.createElement('span');
                root.appendChild(shadow);
                return [
                    host.childNodes.length,                                // light only
                    Array.prototype.indexOf.call(host.childNodes, shadow), // -1: not light
                    Array.prototype.indexOf.call(host.childNodes, light),  // 0: is light
                    root.childNodes.length,                                // shadow only
                    root.childNodes[0] === shadow,
                ].join('|');
            })()"#
        ),
        Ok("1|-1|0|1|true".to_string())
    );
}

#[test]
fn v8_query_selector_respects_shadow_boundary() {
    // DOM selector queries use the logical DOM even when a composed Blitz tree
    // is attached: a document/host query sees light-DOM matches, and a
    // shadow-root query sees only its own shadow matches.
    let mut runtime = runtime_with_render_doc(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const light = document.createElement('p');
                light.className = 'target';
                host.appendChild(light);
                const root = host.attachShadow({ mode: 'open' });
                const shadow = document.createElement('p');
                shadow.className = 'target';
                root.appendChild(shadow);
                return [
                    document.querySelectorAll('.target').length, // light only
                    host.querySelectorAll('.target').length,     // light only
                    root.querySelectorAll('.target').length,     // shadow only
                ].join('|');
            })()"#
        ),
        Ok("1|1|1".to_string())
    );
}

#[test]
fn v8_adopts_shadydom_logical_root_and_connects_lite_children() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    // Pin the JS-shim-driven path (the AURORA_NATIVE_CE_REACTIONS=0 opt-out);
    // the `_with_native_reactions` variant below covers the default.
    runtime.set_native_ce_reactions(false);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let connected = 0;
                class XLite extends HTMLElement {
                    connectedCallback() {
                        connected++;
                        this.textContent = 'connected';
                    }
                }
                customElements.define('x-lite', XLite);

                // This is the shape produced by ShadyDOM useNativeShadow=false:
                // an ordinary detached fragment exposed as the component root.
                const host = document.createElement('x-host');
                const logicalRoot = document.createDocumentFragment();
                const child = document.createElement('x-lite');
                logicalRoot.appendChild(child);
                host.root = logicalRoot;
                document.body.appendChild(host);

                // Stamping discovers the already-upgraded child after the host
                // exposes its logical root. The connect gate must adopt the root
                // and cross to the connected host.
                customElements.__aurora_track_custom_element__(child);
                return [
                    connected,
                    child.isConnected,
                    logicalRoot.host === host,
                    host.shadowRoot === logicalRoot,
                    child.textContent,
                ].join('|');
            })()"#,
        ),
        Ok("1|true|true|true|connected".to_string())
    );

    render_doc.borrow().validate_mirror_integrity().unwrap();
}

/// Same scenario as above, with native CE reactions on: `child` becomes
/// connected purely via `__aurora_track_custom_element__` (detached-fragment
/// adoption), which never touches a native mutation entry point, so nothing is
/// ever queued for it natively. The JS fallback in `connectUpgraded` must
/// still fire `connectedCallback` directly in this case.
#[test]
fn v8_adopts_shadydom_logical_root_and_connects_lite_children_with_native_reactions() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let connected = 0;
                class XLite2 extends HTMLElement {
                    connectedCallback() {
                        connected++;
                        this.textContent = 'connected';
                    }
                }
                customElements.define('x-lite2', XLite2);

                const host = document.createElement('x-host2');
                const logicalRoot = document.createDocumentFragment();
                const child = document.createElement('x-lite2');
                logicalRoot.appendChild(child);
                host.root = logicalRoot;
                document.body.appendChild(host);

                customElements.__aurora_track_custom_element__(child);
                return [
                    connected,
                    child.isConnected,
                    logicalRoot.host === host,
                    host.shadowRoot === logicalRoot,
                    child.textContent,
                ].join('|');
            })()"#,
        ),
        Ok("1|true|true|true|connected".to_string())
    );

    render_doc.borrow().validate_mirror_integrity().unwrap();
}

#[test]
fn v8_composes_polymer_owned_detached_stamp_into_host_root() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let connected = 0;
                class XOwnedLite extends HTMLElement {
                    connectedCallback() {
                        connected++;
                        this.textContent = 'composed';
                    }
                }
                customElements.define('x-owned-lite', XOwnedLite);

                const host = document.createElement('x-owner');
                document.body.appendChild(host);
                const root = host.attachShadow({ mode: 'open' });
                const stamp = document.createDocumentFragment();
                const child = document.createElement('x-owned-lite');
                child.__dataHost = host;
                stamp.appendChild(child);

                customElements.__aurora_track_custom_element__(child);
                return [
                    connected,
                    child.isConnected,
                    child.parentNode === root,
                    root.childNodes.length,
                    stamp.childNodes.length,
                    child.textContent,
                ].join('|');
            })()"#,
        ),
        Ok("1|true|true|1|0|composed".to_string())
    );
    render_doc.borrow().validate_mirror_integrity().unwrap();
}

#[test]
fn v8_tracks_fragment_owner_during_custom_element_lifecycle() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    // Pin the JS-shim-driven path (the AURORA_NATIVE_CE_REACTIONS=0 opt-out);
    // the `_with_native_reactions` variant below covers the default.
    runtime.set_native_ce_reactions(false);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let childConnected = 0;
                const source = document.createDocumentFragment();
                source.appendChild(document.createElement('x-tracked-child'));
                class XTrackedChild extends HTMLElement {
                    connectedCallback() { childConnected++; }
                }
                class XTrackedOwner extends HTMLElement {
                    connectedCallback() {
                        const root = this.attachShadow({ mode: 'open' });
                        const stamp = source.cloneNode(true);
                        const child = stamp.firstChild;
                        customElements.__aurora_track_custom_element__(child);
                        this.result = [
                            stamp.__aurora_fragment_owner__ === this,
                            child.parentNode === root,
                            stamp.childNodes.length,
                        ].join('|');
                    }
                }
                customElements.define('x-tracked-child', XTrackedChild);
                customElements.define('x-tracked-owner', XTrackedOwner);
                const owner = document.createElement('x-tracked-owner');
                document.body.appendChild(owner);
                customElements.__aurora_track_custom_element__(owner);
                return owner.result + '|' + childConnected;
            })()"#,
        ),
        Ok("true|true|0|1".to_string())
    );
    render_doc.borrow().validate_mirror_integrity().unwrap();
}

/// Regression test for the native-reactions gap this fix closes: the owner is
/// inserted via a real `appendChild` (native queues its connectedCallback and
/// fires it during the drop of `append_child`'s `CeReactionsGuard`), but the
/// child only becomes connected via Aurora's detached-ShadyDOM-fragment
/// composition inside that callback — a path that never calls a native
/// mutation entry point, so nothing is ever queued for it. Before the fix,
/// `connectUpgraded` unconditionally trusted `__aurora_native_ce_reactions__`
/// and skipped calling `child.connectedCallback()` itself, so it silently
/// never fired. `ce_has_pending_connected_reaction_native` lets JS tell the
/// two cases apart per element instead of gating on the global flag.
#[test]
fn v8_tracks_fragment_owner_during_custom_element_lifecycle_with_native_reactions() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    runtime.set_native_ce_reactions(true);

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let childConnected = 0;
                let ownerConnected = 0;
                const source = document.createDocumentFragment();
                source.appendChild(document.createElement('x-tracked-child2'));
                class XTrackedChild2 extends HTMLElement {
                    connectedCallback() { childConnected++; }
                }
                class XTrackedOwner2 extends HTMLElement {
                    connectedCallback() {
                        ownerConnected++;
                        const root = this.attachShadow({ mode: 'open' });
                        const stamp = source.cloneNode(true);
                        const child = stamp.firstChild;
                        customElements.__aurora_track_custom_element__(child);
                        this.result = [
                            stamp.__aurora_fragment_owner__ === this,
                            child.parentNode === root,
                            stamp.childNodes.length,
                        ].join('|');
                    }
                }
                customElements.define('x-tracked-child2', XTrackedChild2);
                customElements.define('x-tracked-owner2', XTrackedOwner2);
                const owner = document.createElement('x-tracked-owner2');
                document.body.appendChild(owner);
                return owner.result + '|' + childConnected + '|' + ownerConnected;
            })()"#,
        ),
        Ok("true|true|0|1|1".to_string()),
        "owner's connectedCallback (native-queued) and child's (JS fallback) must each fire exactly once"
    );
    render_doc.borrow().validate_mirror_integrity().unwrap();
}

#[test]
fn v8_preserves_registered_lifecycle_after_constructor_replaces_prototype() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                let connected = 0;
                class XProtoSwap extends HTMLElement {
                    constructor() {
                        super();
                        Object.setPrototypeOf(this, HTMLElement.prototype);
                    }
                    connectedCallback() { connected++; }
                }
                customElements.define('x-proto-swap', XProtoSwap);
                const el = document.createElement('x-proto-swap');
                document.body.appendChild(el);
                customElements.__aurora_track_custom_element__(el);
                return [connected, typeof el.connectedCallback, el.isConnected].join('|');
            })()"#,
        ),
        Ok("1|function|true".to_string())
    );
}

/// Real Comment nodes: nodeType 8, `#comment` nodeName, CharacterData
/// surface, exclusion from parent textContent, deep-clone fidelity, and use
/// as an insertBefore anchor. Polymer's dom-if/dom-repeat rely on comment
/// markers and on childNodes indices staying exactly as recorded at
/// template-parse time, so comments must be first-class nodes rather than
/// approximated with text nodes (nodeType 3), as they were historically.
#[test]
fn v8_supports_comment_nodes() {
    let dom = blank_dom();
    let identity = test_identity();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .expect("render document should build"),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));

    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const c = document.createComment('marker');
                const parts = [];
                parts.push(c.nodeType);
                parts.push(c.nodeName);
                parts.push(c.data);
                parts.push(c.textContent);

                const div = document.createElement('div');
                div.appendChild(document.createTextNode('a'));
                div.appendChild(c);
                div.appendChild(document.createTextNode('b'));
                document.body.appendChild(div);
                parts.push(div.childNodes.length);
                parts.push(div.textContent);

                c.textContent = 'renamed';
                parts.push(c.data);

                const clone = div.cloneNode(true);
                parts.push(clone.childNodes.length);
                parts.push(clone.childNodes[1].nodeType);
                parts.push(clone.childNodes[1].data);

                const span = document.createElement('span');
                div.insertBefore(span, c);
                parts.push(div.childNodes.length);
                parts.push(div.childNodes[1].nodeName.toLowerCase());
                parts.push(div.childNodes[2].nodeType);
                return parts.join('|');
            })()"#,
        ),
        Ok("8|#comment|marker|marker|3|ab|renamed|3|8|renamed|4|span|8".to_string())
    );

    render_doc.borrow().validate_mirror_integrity().unwrap();
}

/// Comments written via innerHTML survive the HTML parser as real comment
/// nodes and serialize back out as `<!-- -->` markup.
#[test]
fn v8_parses_and_serializes_comments_in_inner_html() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const div = document.createElement('div');
                div.innerHTML = '<span>x</span><!--mark--><b>y</b>';
                const kinds = [];
                for (let i = 0; i < div.childNodes.length; i++) {
                    kinds.push(div.childNodes[i].nodeType);
                }
                return kinds.join(',') + '|' + div.innerHTML;
            })()"#,
        ),
        Ok("1,8,1|<span>x</span><!--mark--><b>y</b>".to_string())
    );
}

#[test]
fn v8_deep_clone_preserves_template_content() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const template = document.createElement('template');
                template.innerHTML = '<section><span>inside</span></section>';
                const clone = template.cloneNode(true);
                return [
                    template.content.childNodes.length,
                    clone.content.childNodes.length,
                    clone.content.firstChild.localName,
                    clone.content.firstChild.textContent,
                ].join('|');
            })()"#,
        ),
        Ok("1|1|section|inside".to_string())
    );
}

#[test]
fn v8_shadycss_lite_rewrites_host_and_slotted_selectors() {
    // ShadyCSS-lite rewrites shadow-scoped selectors to target the flattened
    // (synthetic, no-native-shadow) render tree, scoped by the component tag.
    let mut runtime = V8Runtime::new(blank_dom());
    // The CSS fixtures below contain no single quotes, so they embed directly.
    let mut scope_css = |css: &str| {
        runtime.eval_to_string(&format!(
            "globalThis.__aurora_shadycss__.scopeCss('{css}', 'x-foo')"
        ))
    };

    // :host -> tag ; :host(sel) -> tagsel
    assert_eq!(
        scope_css(":host { color: red; }"),
        Ok("x-foo{ color: red; }".to_string())
    );
    assert_eq!(
        scope_css(":host(.dark) { background: black; }"),
        Ok("x-foo.dark{ background: black; }".to_string())
    );
    // ::slotted(sel) -> tag sel (flattened: slotted children are descendants)
    assert_eq!(
        scope_css("::slotted(.a) { color: blue; }"),
        Ok("x-foo .a{ color: blue; }".to_string())
    );
    // Component-internal selectors are scoped as descendants of the host tag.
    assert_eq!(
        scope_css(".inner { color: green; }"),
        Ok("x-foo .inner{ color: green; }".to_string())
    );
    // Global selectors that define resets/variables must not be scoped (the
    // selector is preserved verbatim; only surrounding whitespace is trimmed).
    assert_eq!(
        scope_css(":root { --x: 1; }"),
        Ok(":root{ --x: 1; }".to_string())
    );
}

// ─── Task 5.1 / 5.2: ShadyCSS instrumentation + warning ──────────────────────

#[test]
fn v8_shadycss_diagnostics_are_gated_behind_debug_flag() {
    // Without the debug flag, no diagnostics are recorded; with it on, selector
    // rewrites and at-rule passthroughs are captured on the diagnostics buffer.
    let mut runtime = V8Runtime::new(blank_dom());

    // Gated off by default.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const sc = globalThis.__aurora_shadycss__;
                sc.diagnostics.length = 0;
                sc.scopeCss(':host { color: red; } @keyframes spin { from {} }', 'x-comp');
                return String(sc.diagnostics.length);
            })()"#
        ),
        Ok("0".to_string())
    );

    // Enabled: selector rewrite + at-rule passthrough are recorded.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const sc = globalThis.__aurora_shadycss__;
                globalThis.__aurora_debug_shadycss__ = true;
                sc.diagnostics.length = 0;
                sc.scopeCss(':host { color: red; } @keyframes spin { from {} }', 'x-comp');
                const kinds = sc.diagnostics.map(d => d.kind).sort().join(',');
                const sel = sc.diagnostics.find(d => d.kind === 'selector');
                return kinds + '|' + (sel ? sel.from + '=>' + sel.to + '@' + sel.component : 'no-sel');
            })()"#
        ),
        Ok("at-rule-passthrough,selector|:host=>x-comp@x-comp".to_string())
    );
}

#[test]
fn v8_shadycss_emits_once_per_page_warning() {
    // The synthetic-rewriting warning fires once per page regardless of how many
    // times the rewriter runs.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const sc = globalThis.__aurora_shadycss__;
                const before = sc.warningCount;
                sc.scopeCss(':host { color: red; }', 'x-a');
                sc.scopeCss('.inner { color: blue; }', 'x-b');
                sc.scopeCss(':host(.x) { color: green; }', 'x-c');
                return before + '|' + sc.warningCount;
            })()"#
        ),
        Ok("0|1".to_string())
    );
}

#[test]
fn v8_shadow_slot_distribution_assigns_light_children_to_slots() {
    let mut runtime = V8Runtime::new(blank_dom());
    // Intended native behavior: light children are distributed to matching
    // <slot>s and exposed via slot.assignedNodes(). Synthetic shadow has no slot
    // outlets, so this documents the gap rather than current behavior.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const light = document.createElement('span');
                host.appendChild(light);
                const root = host.attachShadow({ mode: 'open' });
                const slot = document.createElement('slot');
                root.appendChild(slot);
                const assigned = typeof slot.assignedNodes === 'function'
                    ? slot.assignedNodes()
                    : [];
                return String(assigned.length === 1 && assigned[0] === light);
            })()"#
        ),
        Ok("true".to_string())
    );
}

#[test]
#[ignore = "composed-path shadow semantics are unsupported: composedPath() does \
            not yet model the shadow-root boundary between a shadow child and \
            its host. Tracked for native shadow semantics (Phase 4 follow-up)."]
fn v8_composed_path_includes_shadow_root_between_child_and_host() {
    let mut runtime = V8Runtime::new(blank_dom());
    // Intended native behavior: a composed event dispatched inside a shadow tree
    // exposes the shadow root in its composedPath between the target and the host.
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
                const host = document.createElement('div');
                document.body.appendChild(host);
                const root = host.attachShadow({ mode: 'open' });
                const child = document.createElement('span');
                root.appendChild(child);
                let sawRoot = false, sawHost = false;
                host.addEventListener('ping', (event) => {
                    const path = event.composedPath();
                    sawRoot = path.indexOf(root) !== -1;
                    sawHost = path.indexOf(host) !== -1;
                });
                child.dispatchEvent(new CustomEvent('ping', { bubbles: true, composed: true }));
                return sawRoot + '|' + sawHost;
            })()"#
        ),
        Ok("true|true".to_string())
    );
}

#[test]
fn multiple_runtimes_interleave_and_drop_in_any_order() {
    // Multi-tab support: each tab owns a runtime (and thus a V8 isolate) on the
    // same thread. Operations must be able to interleave across runtimes, and
    // runtimes must be droppable in arbitrary order — not just reverse order of
    // creation (tabs close in whatever order the user closes them).
    let mut first = V8Runtime::new(blank_dom());
    let mut second = V8Runtime::new(blank_dom());
    let mut third = V8Runtime::new(blank_dom());

    first.eval_to_string("globalThis.who = 'first'").unwrap();
    second.eval_to_string("globalThis.who = 'second'").unwrap();
    third.eval_to_string("globalThis.who = 'third'").unwrap();

    // Interleaved access: contexts stay isolated per runtime.
    assert_eq!(first.eval_to_string("who"), Ok("first".to_string()));
    assert_eq!(second.eval_to_string("who"), Ok("second".to_string()));
    assert_eq!(first.eval_to_string("who"), Ok("first".to_string()));

    // Drop the *middle* runtime first, then the oldest; the survivors must
    // still execute. With enter-forever isolates this ordering panics.
    drop(second);
    assert_eq!(first.eval_to_string("who"), Ok("first".to_string()));
    assert_eq!(third.eval_to_string("who"), Ok("third".to_string()));
    drop(first);
    assert_eq!(third.eval_to_string("who"), Ok("third".to_string()));
}

#[test]
fn v8_element_style_is_a_prototype_accessor_not_an_own_property() {
    // An own `style` data property on every wrapper shadowed custom-element
    // class accessors; YouTube's wrapped components then captured the
    // declaration object as a user-supplied `style` prop and crashed their
    // host render.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const div = document.createElement('div');
            const own = Object.prototype.hasOwnProperty.call(div, 'style');
            div.style.width = '5px';
            div.style = 'height: 7px';
            function Wrapped() { HTMLElement.call(this); }
            Wrapped.prototype = Object.create(HTMLElement.prototype);
            Wrapped.prototype.constructor = Wrapped;
            Object.defineProperty(Wrapped.prototype, 'style', { set(v) {}, configurable: true });
            customElements.define('x-wrapped-style', Wrapped);
            const wrapped = document.createElement('x-wrapped-style');
            return [own, div.style.height, typeof wrapped.style].join(',');
            })()
            "#
        ),
        Ok("false,7px,undefined".to_string())
    );
}

#[test]
fn v8_inserting_an_emptied_composed_fragment_reclaims_its_nodes() {
    // composeDetachedStamp may move a fragment's content into an owner's
    // shadow root before the fragment's creator inserts it (YouTube's renderer
    // stamper). The creator's own insertion must still place the content.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const host = document.createElement('div');
            document.body.appendChild(host);
            const root = host.attachShadow({ mode: 'open' });
            const container = document.createElement('div');
            const moved = document.createElement('span');
            const stolen = document.createElement('b');
            const elsewhere = document.createElement('i');
            root.appendChild(container);
            const fragment = document.createDocumentFragment();
            root.appendChild(moved);
            root.appendChild(stolen);
            elsewhere.appendChild(stolen); // page moved it after composition
            Object.defineProperty(fragment, '__aurora_composed_nodes__', {
                value: { nodes: [moved, stolen], target: root },
                configurable: true, writable: true
            });
            container.appendChild(fragment);
            return [moved.parentNode === container, stolen.parentNode === elsewhere,
                    fragment.childNodes.length].join(',');
            })()
            "#
        ),
        Ok("true,true,0".to_string())
    );
}

#[test]
fn v8_shady_physical_insert_into_adopted_host_lands_in_shadow_root() {
    // ShadyDOM renders a ShadyRoot's content as the host's physical children
    // via captured natives (`Node.prototype.appendChild.call(host, …)`).
    // Aurora renders the adopted root instead, so that content must go there;
    // the host's own light children stay put for slot distribution.
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const host = document.createElement('div');
            const light = document.createElement('p');
            host.appendChild(light);
            document.body.appendChild(host);
            const root = document.createDocumentFragment();
            host.__aurora_adoptShadowRoot(root, 'open');
            Object.defineProperty(host, 'shadowRoot', { value: root, configurable: true });
            Object.defineProperty(root, 'host', { value: host, configurable: true });
            Object.defineProperty(root, '__aurora_registered_shadow_root__', { value: true, configurable: true });
            const text = document.createTextNode('hello');
            Node.prototype.appendChild.call(host, text);
            Node.prototype.insertBefore.call(host, light, null);
            const plainHost = document.createElement('div');
            const plainText = document.createTextNode('x');
            Node.prototype.appendChild.call(plainHost, plainText);
            return [text.parentNode === root, light.parentNode === host,
                    plainText.parentNode === plainHost].join(',');
            })()
            "#
        ),
        Ok("true,true,true".to_string())
    );
}

#[test]
fn replay_date_pinning_shifts_the_clock_and_keeps_native_dates() {
    let mut runtime = V8Runtime::new(blank_dom());
    runtime
        .eval_to_string(&super::determinism::pin_date_script(1_794_595_964_000.0))
        .unwrap();

    assert_eq!(
        runtime.eval_to_string(
            r#"
            (() => {
            const start = 1794595964000;
            const near = (ms) => ms >= start && ms < start + 60000;
            const d = new Date();
            return [
                near(Date.now()),
                near(d.getTime()),
                d instanceof Date,
                d.constructor === Date,
                typeof Date(),
                new Date(0).toISOString(),
                new Date(2020, 1).getMonth(),
                new Date(2020, 1).getDate(),
                Date.UTC(2020, 0, 2),
                Date.parse('2020-01-02T00:00:00Z')
            ].join('|');
            })()
            "#
        ),
        Ok("true|true|true|true|string|1970-01-01T00:00:00.000Z|1|1|1577923200000|1577923200000"
            .to_string())
    );
}
