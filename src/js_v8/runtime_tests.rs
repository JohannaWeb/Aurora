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
            class YtAttributedString {
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
    // We can't easily test actual network without mocking fetch_string,
    // but we can check if it returns a Promise.
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
fn engines_hot_swap_behind_the_js_runtime_trait() {
    let dom = blank_dom();
    let mut runtime: Box<dyn JsRuntime> = create_runtime(EngineKind::V8, &dom).unwrap();

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
