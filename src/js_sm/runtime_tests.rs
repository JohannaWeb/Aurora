use super::*;
use crate::dom::{Node, NodePtr};
use crate::html::Parser;
use std::time::Instant;

#[test]
fn promise_callbacks_run_at_end_of_execute() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    // Promise reactions are microtasks: they drain at the end of execute()
    // (the script-execution checkpoint), not on a later tick. mozjs 0.14's
    // RunJobs ignored custom job queues so reactions used to slip to tick;
    // 0.16 dispatches to our JobQueue trap as the drain loop always intended.
    runtime
        .execute(
            r#"
            Promise.resolve("ready").then((value) => {
                document.body.textContent = value;
            });
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "ready");

    runtime
        .execute(
            r#"
            new Promise((resolve) => resolve(20))
                .then((value) => value + 22)
                .then((value) => {
                    document.body.textContent = String(value);
                });
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "42");
    runtime
        .execute(
            r#"
            document.body.textContent = "";
            class TestCard extends HTMLElement {
                connectedCallback() {
                    this.textContent = "connected";
                }
            }
            customElements.define("test-card", TestCard);

            const el = document.createElement("test-card");
            document.body.appendChild(el);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "connected");

    runtime
        .execute(
            r#"
            document.body.textContent = "";
            window.addEventListener("DOMContentLoaded", () => {
                document.body.textContent += "dom";
            });
            document.addEventListener("load", () => {
                document.body.textContent += ":doc-load";
            });
            window.addEventListener("load", () => {
                document.body.textContent += ":win-load";
            });
            "#,
        )
        .unwrap();

    runtime.fire_dom_content_loaded();
    runtime.fire_load();
    assert_eq!(text_content(&dom), "dom:doc-load:win-load");
}

#[test]
fn custom_elements_upgrade_existing_dom_nodes_on_define() {
    let dom = Parser::new("<html><body><test-card></test-card></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class TestCard extends HTMLElement {
                connectedCallback() {
                    this.textContent = "hydrated";
                }
            }
            customElements.define("test-card", TestCard);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "hydrated");
}

#[test]
fn dom_module_templates_resolve_for_custom_elements() {
    let dom = Parser::new(
        "<html><body>\
         <dom-module id=\"test-card\"><template><span>hydrated</span></template></dom-module>\
         </body></html>",
    )
    .parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class DomModule extends HTMLElement {}
            customElements.define("dom-module", DomModule);

            class TestCard extends HTMLElement {
            }

            customElements.define("test-card", TestCard);
            document.body.textContent = String(!!customElements.get("test-card").template);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "true");

    runtime
        .execute(
            r#"
            (() => {
                document.body.textContent = "";
                const template = customElements.get("test-card").template;
                document.body.textContent = String(template.content.cloneNode(true).nodeType);
            })();
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "11");

    runtime
        .execute(
            r#"
            (() => {
                document.body.textContent = "";
                const template = customElements.get("test-card").template;
                document.body.appendChild(template.content.cloneNode(true));
            })();
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "hydrated");
}

#[test]
fn template_content_preserves_identity_and_document_fragment_prototype() {
    let dom = Parser::new(
        "<html><body><template id=\"tpl\"><span>hydrated</span></template></body></html>",
    )
    .parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r##"
            (() => {
                const tpl1 = document.querySelector("#tpl");
                const tpl2 = document.querySelector("#tpl");
                document.body.textContent = [
                    String(tpl1 === tpl2),
                    String(tpl1.content === tpl2.content),
                    String(tpl1.content instanceof DocumentFragment),
                ].join(":");
            })();
            "##,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "true:true:true");
}

#[test]
fn class_custom_element_upgrade_preserves_constructor_template_lookup() {
    let dom = Parser::new(
        "<html><body>\
         <dom-module id=\"test-card\"><template><span>hydrated</span></template></dom-module>\
         <test-card></test-card>\
         </body></html>",
    )
    .parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class DomModule extends HTMLElement {}
            customElements.define("dom-module", DomModule);

            class TestCard extends HTMLElement {
                connectedCallback() {
                    const template = this.constructor.template;
                    this.setAttribute("data-template", String(!!template));
                    this.setAttribute("data-constructor-match", String(this.constructor === customElements.get("test-card")));
                    this.textContent = template && template.content ? template.content.textContent.trim() : "missing";
                }
            }

            customElements.define("test-card", TestCard);
            "#,
        )
        .unwrap();

    let card = find_first_tag(&dom, "test-card").expect("test-card should exist");
    let card_ref = card.borrow();
    let Node::Element(card) = &*card_ref else {
        panic!("test-card should be an element");
    };
    assert_eq!(
        card.attributes.get("data-template").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        card.attributes
            .get("data-constructor-match")
            .map(String::as_str),
        Some("true")
    );
    drop(card_ref);
    assert_eq!(text_content(&dom), "hydrated");
}

#[test]
fn class_custom_element_upgrade_replays_constructor() {
    let dom = Parser::new("<html><body><test-card></test-card></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            let constructorCalls = 0;
            class TestCard extends HTMLElement {
                constructor() {
                    super();
                    constructorCalls += 1;
                }
                connectedCallback() {
                    this.textContent = String(constructorCalls);
                }
            }

            customElements.define("test-card", TestCard);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "1");
}

#[test]
fn document_current_script_tracks_running_script_node() {
    let dom =
        Parser::new("<html><body><script src=\"/app.js\"></script></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());
    let script = find_first_tag(&dom, "script").expect("script element should exist");

    runtime.set_current_script(Some(&script));
    runtime
        .execute(
            r#"
            document.body.textContent =
                document.currentScript.tagName + ":" + document.currentScript.getAttribute("src");
            "#,
        )
        .unwrap();
    runtime.set_current_script(None);

    assert_eq!(text_content(&dom), "SCRIPT:/app.js");
}

#[test]
fn element_attributes_exposes_named_node_map() {
    let dom = Parser::new("<html><body><div id=\"app\" class=\"one\"></div></body></html>")
        .parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r##"
            const el = document.querySelector("#app");
            const attrs = el.attributes;
            const before = [
                attrs.length,
                attrs.getNamedItem("id").value,
                attrs.item(0).name ? "item" : "missing"
            ].join(":");
            attrs.getNamedItem("class").value = "two";
            attrs.removeNamedItem("id");
            attrs.setNamedItem({ name: "data-ready", value: before });
            document.body.setAttribute(
                "data-result",
                [
                    before,
                    el.getAttribute("class"),
                    el.hasAttribute("id"),
                    el.getAttribute("data-ready"),
                    el.attributes.length
                ].join("|")
            );
            "##,
        )
        .unwrap();

    let body = find_first_tag(&dom, "body").unwrap();
    let result = match &*body.borrow() {
        Node::Element(element) => element.attributes.get("data-result").cloned(),
        _ => None,
    };
    assert_eq!(result.as_deref(), Some("2:app:item|two|false|2:app:item|2"));
}

#[test]
fn request_idle_callback_receives_deadline_object() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            requestIdleCallback((deadline) => {
                document.body.textContent = String(deadline.didTimeout) + ":" + String(typeof deadline.timeRemaining);
            });
            "#,
        )
        .unwrap();

    assert!(runtime.tick(Instant::now()));
    assert_eq!(text_content(&dom), "false:function");
}

#[test]
fn message_channel_delivers_messages() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const channel = new MessageChannel();
            channel.port2.onmessage = (event) => {
                document.body.textContent = event.data;
            };
            channel.port1.postMessage("ping");
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "ping");
}

#[test]
fn url_polyfill_resolves_relative_urls_and_query_params() {
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            const url = new URL('/watch?v=abc123&feature=share', 'https://www.youtube.com/feed/subscriptions?persist=1');
            url.searchParams.set('feature', 'related');
            url.searchParams.append('t', '42');
            const relative = new URL('../shorts/xyz?si=token', 'https://www.youtube.com/watch/');
            const params = new URLSearchParams('a=1&a=2&empty=');
            document.body.textContent = [
                url.href,
                url.origin,
                url.hostname,
                url.searchParams.get('v'),
                url.searchParams.get('feature'),
                url.searchParams.getAll('a').length,
                relative.href,
                params.getAll('a').join(','),
                String(params.has('empty'))
            ].join('|');
            "#,
        )
        .unwrap();

    assert_eq!(
        text_content(&dom),
        "https://www.youtube.com/watch?v=abc123&feature=related&t=42|https://www.youtube.com|www.youtube.com|abc123|related|0|https://www.youtube.com/shorts/xyz?si=token|1,2|true"
    );
}

#[test]
fn es5_closure_custom_element_upgrade_resolves_prototype_methods() {
    // Mirrors YouTube's kevlar ES5 "PolySi" wrapper (`ui3` in the bundle):
    // a function constructor inheriting from HTMLElement via Closure's
    // goog.inherits, whose body calls a method defined on its own prototype.
    let dom =
        Parser::new("<html><body><ytd-masthead></ytd-masthead></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            var E = function() {
                var g = HTMLElement.call(this) || this;
                g.is = 'ytd-masthead';
                g.createElement();
                return g;
            };
            function tmp() {}
            tmp.prototype = HTMLElement.prototype;
            E.prototype = new tmp();
            E.prototype.constructor = E;
            E.prototype.createElement = function() {
                document.body.setAttribute('data-created', 'yes');
            };
            customElements.define('ytd-masthead', E);
            "#,
        )
        .unwrap();

    let body = find_first_tag(&dom, "body").unwrap();
    let created = match &*body.borrow() {
        Node::Element(element) => element.attributes.get("data-created").cloned(),
        _ => None,
    };
    assert_eq!(created.as_deref(), Some("yes"));
}

#[test]
fn es5_adapter_custom_element_upgrade_returns_upgraded_element() {
    // YouTube's kevlar ES5 bundle wraps HTMLElement with Polymer's
    // custom-elements-es5-adapter: `function() { return
    // Reflect.construct(HTMLElement, [], this.constructor); }`. During an
    // upgrade replay, that construct call must hand back the element being
    // upgraded (native "construction stack" semantics) — otherwise the
    // constructor body runs against a detached plain object.
    let dom =
        Parser::new("<html><body><ytd-masthead></ytd-masthead></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            var NativeHTMLElement = HTMLElement;
            var AdapterHTMLElement = function() {
                return Reflect.construct(NativeHTMLElement, [], this.constructor);
            };
            AdapterHTMLElement.prototype = NativeHTMLElement.prototype;
            AdapterHTMLElement.prototype.constructor = AdapterHTMLElement;
            Object.setPrototypeOf(AdapterHTMLElement, NativeHTMLElement);
            HTMLElement = AdapterHTMLElement;

            var E = function() {
                var g = HTMLElement.call(this) || this;
                g.is = 'ytd-masthead';
                g.createElement();
                return g;
            };
            E.prototype = Object.create(HTMLElement.prototype);
            E.prototype.constructor = E;
            E.prototype.createElement = function() {
                var target = document.querySelector('ytd-masthead');
                document.body.setAttribute('data-created', 'yes');
                document.body.setAttribute('data-upgraded-in-place', String(this === target));
            };
            customElements.define('ytd-masthead', E);
            "#,
        )
        .unwrap();

    let body = find_first_tag(&dom, "body").unwrap();
    let (created, in_place) = match &*body.borrow() {
        Node::Element(element) => (
            element.attributes.get("data-created").cloned(),
            element.attributes.get("data-upgraded-in-place").cloned(),
        ),
        _ => (None, None),
    };
    assert_eq!(created.as_deref(), Some("yes"));
    assert_eq!(in_place.as_deref(), Some("true"));
}

#[test]
fn template_accessor_defers_to_inherited_static_template() {
    // Polymer 3 / YouTube kevlar resolve templates through a static
    // `template` getter on a base class, often assigned lazily after
    // customElements.define. Aurora's dom-module fallback accessor must not
    // shadow that inherited getter.
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            class Base extends HTMLElement {
                static get template() {
                    return Base.__lazyTemplate || null;
                }
            }
            class Card extends Base {}
            customElements.define('test-card', Card);

            // Assigned after define, like kevlar's lazy `l()` thunk.
            const tpl = document.createElement('template');
            tpl.content.appendChild(document.createTextNode('stamped'));
            Base.__lazyTemplate = tpl;

            const resolved = customElements.get('test-card').template;
            document.body.textContent = String(resolved === tpl);
            "#,
        )
        .unwrap();

    assert_eq!(text_content(&dom), "true");
}

#[test]
fn script_created_template_supports_kevlar_build_sequence() {
    // YouTube's kevlar `AZ.template` getter builds the app template with:
    //   var d = document.createElement("template");
    //   d.innerHTML = <html string>;            (via TrustedTypes helpers)
    //   d.content.insertBefore(shared.content.cloneNode(true), d.content.firstChild);
    // Each step must work on a script-created (never parsed) template.
    let dom = Parser::new("<html><body></body></html>").parse_document();
    let mut runtime = SmRuntime::new(dom.clone());

    runtime
        .execute(
            r#"
            var steps = [];
            try {
                var d = document.createElement('template');
                steps.push('create:ok');
                d.innerHTML = '<div id="content"><span>app</span></div>';
                steps.push('innerHTML:ok');
                steps.push('content:' + (d.content ? 'ok' : 'MISSING'));
                steps.push('kids:' + (d.content && d.content.childNodes ? d.content.childNodes.length : '?'));
                var shared = document.createElement('template');
                shared.innerHTML = '<style>x{}</style>';
                var clone = shared.content.cloneNode(true);
                steps.push('clone:' + (clone ? 'ok' : 'MISSING'));
                d.content.insertBefore(clone, d.content.firstChild);
                steps.push('insertBefore:ok');
                steps.push('finalKids:' + d.content.childNodes.length);
            } catch (e) {
                steps.push('THREW:' + e.message);
            }
            document.body.textContent = steps.join('|');
            "#,
        )
        .unwrap();

    assert_eq!(
        text_content(&dom),
        "create:ok|innerHTML:ok|content:ok|kids:1|clone:ok|insertBefore:ok|finalKids:2"
    );
}

fn text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Document { children, .. } => children.iter().map(text_content).collect(),
        Node::Element(element) => element.children.iter().map(text_content).collect(),
        Node::Text(text) => text.clone(),
    }
}

fn find_first_tag(node: &NodePtr, tag: &str) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Document { children, .. } => {
            children.iter().find_map(|child| find_first_tag(child, tag))
        }
        Node::Element(element) => {
            if element.tag_name == tag {
                return Some(node.clone());
            }
            element
                .children
                .iter()
                .find_map(|child| find_first_tag(child, tag))
        }
        Node::Text(_) => None,
    }
}
