// Boa JavaScript runtime with an expanded DOM/BOM bridge.
//
// Supported surface (enough to let many modern scripts initialize without throwing):
//   document:       body, head, documentElement, title, readyState, cookie,
//                   createElement, createTextNode, createDocumentFragment,
//                   getElementById, getElementsByTagName, getElementsByClassName,
//                   querySelector, querySelectorAll, addEventListener/removeEventListener
//   Element/Node:   tagName, nodeName, nodeType, id, className, textContent,
//                   innerHTML, innerText, outerHTML, children, childNodes,
//                   firstChild, lastChild, parentNode, parentElement, style,
//                   classList, dataset, attributes list,
//                   appendChild, insertBefore, removeChild, replaceChild,
//                   cloneNode, contains, setAttribute, getAttribute,
//                   removeAttribute, hasAttribute, hasAttributes,
//                   querySelector, querySelectorAll,
//                   getElementsByTagName, getElementsByClassName,
//                   getBoundingClientRect, focus, blur, click,
//                   addEventListener/removeEventListener/dispatchEvent
//   window:         document, window, self, top, parent, globalThis,
//                   innerWidth/innerHeight/outerWidth/outerHeight,
//                   devicePixelRatio, scrollX/Y/scrollTo/scrollBy,
//                   setTimeout/setInterval/clearTimeout/clearInterval,
//                   requestAnimationFrame/cancelAnimationFrame,
//                   requestIdleCallback/cancelIdleCallback,
//                   matchMedia, getComputedStyle, alert/confirm/prompt,
//                   addEventListener/removeEventListener/dispatchEvent,
//                   localStorage/sessionStorage, location, history, navigator,
//                   performance, screen, MutationObserver, IntersectionObserver,
//                   ResizeObserver, fetch, XMLHttpRequest (stub)
//
// Design notes:
//   * Every JS node object carries a __node_id referencing a NodeRegistry
//     entry. Methods pull the NodePtr back out of the registry on each call,
//     so mutations from Rust (html parser, etc.) stay visible through the
//     same JS handle.
//   * Timers, observers, fetch, and storage are stubs: they keep pages alive
//     without panicking, but don't schedule real work.
//   * parentNode is resolved by scanning from the document root — the DOM
//     doesn't store parent pointers, so this is O(N) per call but rare enough
//     in practice.

use crate::dom::{ElementNode, Node, NodePtr};

use boa_engine::object::builtins::JsArray;
use boa_engine::object::{FunctionObjectBuilder, ObjectInitializer};
use boa_engine::property::Attribute;
use boa_engine::{
    js_string, Context, JsError, JsObject, JsResult, JsString, JsValue, NativeFunction, Source,
};
use boa_gc::{empty_trace, Finalize, Trace};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Registry: maps a small integer id per JS node handle to a NodePtr.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct NodeRegistry {
    nodes: Rc<RefCell<BTreeMap<u32, NodePtr>>>,
    next_id: Rc<RefCell<u32>>,
}

unsafe impl Trace for NodeRegistry {
    empty_trace!();
}
impl Finalize for NodeRegistry {}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            nodes: Rc::new(RefCell::new(BTreeMap::new())),
            next_id: Rc::new(RefCell::new(1)),
        }
    }

    fn register(&self, node: NodePtr) -> u32 {
        let mut next_id = self.next_id.borrow_mut();
        let id = *next_id;
        *next_id += 1;
        self.nodes.borrow_mut().insert(id, node);
        id
    }

    fn lookup(&self, id: u32) -> Option<NodePtr> {
        self.nodes.borrow().get(&id).cloned()
    }
}

// ---------------------------------------------------------------------------
// Captures: grouping Rc handles passed into Boa closures.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct NodeCapture {
    node: NodePtr,
    registry: NodeRegistry,
    document: NodePtr,
}
unsafe impl Trace for NodeCapture {
    empty_trace!();
}
impl Finalize for NodeCapture {}

#[derive(Clone)]
struct DocCapture {
    document: NodePtr,
    registry: NodeRegistry,
}
unsafe impl Trace for DocCapture {
    empty_trace!();
}
impl Finalize for DocCapture {}

#[derive(Clone)]
struct WindowCapture {
    storage: Rc<RefCell<BTreeMap<String, String>>>,
    session: Rc<RefCell<BTreeMap<String, String>>>,
    next_timer: Rc<RefCell<u32>>,
}
unsafe impl Trace for WindowCapture {
    empty_trace!();
}
impl Finalize for WindowCapture {}

// ---------------------------------------------------------------------------
// Runtime.
// ---------------------------------------------------------------------------

pub struct BoaRuntime {
    context: Context,
    #[allow(dead_code)]
    document: NodePtr,
    #[allow(dead_code)]
    registry: NodeRegistry,
}

impl BoaRuntime {
    pub fn new(document: NodePtr) -> Self {
        let mut context = Context::default();
        let registry = NodeRegistry::new();

        install_globals(&mut context, &document, &registry);
        install_dom_constructors(&mut context);
        install_document(&mut context, &document, &registry);
        install_observers(&mut context);
        install_xhr_and_fetch(&mut context);

        Self {
            context,
            document,
            registry,
        }
    }

    pub fn execute(&mut self, script: &str) -> JsResult<JsValue> {
        self.context.eval(Source::from_bytes(script))
    }
}

// ---------------------------------------------------------------------------
// Helpers: build NativeFunction → JsFunction, accessors, value conversion.
// ---------------------------------------------------------------------------

fn native_to_jsfn(context: &mut Context, native: NativeFunction) -> JsValue {
    FunctionObjectBuilder::new(context.realm(), native)
        .name(js_string!(""))
        .length(0)
        .constructor(false)
        .build()
        .into()
}

fn js_string_of(value: &JsValue) -> String {
    value
        .as_string()
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| {
            if value.is_undefined() || value.is_null() {
                String::new()
            } else {
                value.display().to_string()
            }
        })
}

fn node_from_js(
    value: &JsValue,
    registry: &NodeRegistry,
    context: &mut Context,
) -> Option<NodePtr> {
    let obj = value.as_object()?;
    let id_val = obj.get(js_string!("__node_id"), context).ok()?;
    let id = id_val.as_number()? as u32;
    registry.lookup(id)
}

// ---------------------------------------------------------------------------
// Global setup: window, navigator, location, timers, storage, observers.
// ---------------------------------------------------------------------------

fn install_globals(context: &mut Context, document: &NodePtr, _registry: &NodeRegistry) {
    let global_obj = context.global_object().clone();

    let _ = context.register_global_property(
        js_string!("globalThis"),
        global_obj.clone(),
        Attribute::all(),
    );
    let _ = context.register_global_property(
        js_string!("window"),
        global_obj.clone(),
        Attribute::all(),
    );
    let _ =
        context.register_global_property(js_string!("self"), global_obj.clone(), Attribute::all());
    let _ =
        context.register_global_property(js_string!("top"), global_obj.clone(), Attribute::all());
    let _ = context.register_global_property(
        js_string!("parent"),
        global_obj.clone(),
        Attribute::all(),
    );

    // Console with a handful of methods.
    let console = ObjectInitializer::new(context)
        .function(log_native(), js_string!("log"), 1)
        .function(log_native(), js_string!("info"), 1)
        .function(log_native(), js_string!("warn"), 1)
        .function(log_native(), js_string!("error"), 1)
        .function(log_native(), js_string!("debug"), 1)
        .function(log_native(), js_string!("trace"), 1)
        .function(noop_native(), js_string!("group"), 0)
        .function(noop_native(), js_string!("groupEnd"), 0)
        .function(noop_native(), js_string!("time"), 0)
        .function(noop_native(), js_string!("timeEnd"), 0)
        .build();
    let _ = context.register_global_property(js_string!("console"), console, Attribute::all());

    // Window event listeners (no-op).
    let _ = global_obj.set(
        js_string!("addEventListener"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("removeEventListener"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("dispatchEvent"),
        native_to_jsfn(context, return_bool(true)),
        false,
        context,
    );

    // Viewport & screen stubs.
    for (name, val) in [
        ("innerWidth", 1200.0),
        ("innerHeight", 800.0),
        ("outerWidth", 1200.0),
        ("outerHeight", 800.0),
        ("devicePixelRatio", 1.0),
        ("scrollX", 0.0),
        ("scrollY", 0.0),
        ("pageXOffset", 0.0),
        ("pageYOffset", 0.0),
    ] {
        let _ = context.register_global_property(JsString::from(name), val, Attribute::all());
    }

    let screen = ObjectInitializer::new(context)
        .property(js_string!("width"), 1200, Attribute::all())
        .property(js_string!("height"), 800, Attribute::all())
        .property(js_string!("availWidth"), 1200, Attribute::all())
        .property(js_string!("availHeight"), 800, Attribute::all())
        .property(js_string!("colorDepth"), 24, Attribute::all())
        .property(js_string!("pixelDepth"), 24, Attribute::all())
        .build();
    let _ = context.register_global_property(js_string!("screen"), screen, Attribute::all());

    // Timer / raf stubs that just return a fresh id.
    let win_cap = WindowCapture {
        storage: Rc::new(RefCell::new(BTreeMap::new())),
        session: Rc::new(RefCell::new(BTreeMap::new())),
        next_timer: Rc::new(RefCell::new(1)),
    };

    let timer_id_fn =
        |_this: &JsValue, _args: &[JsValue], cap: &WindowCapture, _ctx: &mut Context| {
            let mut next = cap.next_timer.borrow_mut();
            let id = *next;
            *next += 1;
            Ok(JsValue::from(id))
        };
    let _ = global_obj.set(
        js_string!("setTimeout"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("setInterval"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("requestAnimationFrame"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("requestIdleCallback"),
        native_to_jsfn(
            context,
            NativeFunction::from_copy_closure_with_captures(timer_id_fn, win_cap.clone()),
        ),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("clearTimeout"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("clearInterval"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("cancelAnimationFrame"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("cancelIdleCallback"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );

    // queueMicrotask: invoke the callback right now.
    let queue_micro = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        if let Some(cb) = args.get(0).and_then(|v| v.as_callable()).cloned() {
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(JsValue::undefined())
    });
    let _ = global_obj.set(
        js_string!("queueMicrotask"),
        native_to_jsfn(context, queue_micro),
        false,
        context,
    );

    // Alert / confirm / prompt stubs.
    let _ = global_obj.set(
        js_string!("alert"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("confirm"),
        native_to_jsfn(context, return_bool(false)),
        false,
        context,
    );
    let prompt_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null()));
    let _ = global_obj.set(
        js_string!("prompt"),
        native_to_jsfn(context, prompt_fn),
        false,
        context,
    );

    // matchMedia returns an object that never matches.
    let match_media = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let media = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("matches"), false, Attribute::all())
            .property(js_string!("media"), JsString::from(media), Attribute::all())
            .function(noop_native(), js_string!("addListener"), 1)
            .function(noop_native(), js_string!("removeListener"), 1)
            .function(noop_native(), js_string!("addEventListener"), 2)
            .function(noop_native(), js_string!("removeEventListener"), 2)
            .function(return_bool(true), js_string!("dispatchEvent"), 1)
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("matchMedia"),
        native_to_jsfn(context, match_media),
        false,
        context,
    );

    // getComputedStyle returns an empty CSSStyleDeclaration-ish object.
    let gcs = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!("")))),
                js_string!("getPropertyValue"),
                1,
            )
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("getComputedStyle"),
        native_to_jsfn(context, gcs),
        false,
        context,
    );

    // Scrolling
    let _ = global_obj.set(
        js_string!("scrollTo"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("scrollBy"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("scroll"),
        native_to_jsfn(context, noop_native()),
        false,
        context,
    );

    // atob / btoa
    let atob = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let s = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let decoded = base64_decode(&s).unwrap_or_default();
        Ok(JsValue::from(JsString::from(decoded)))
    });
    let btoa = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let s = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        Ok(JsValue::from(JsString::from(base64_encode(s.as_bytes()))))
    });
    let _ = global_obj.set(
        js_string!("atob"),
        native_to_jsfn(context, atob),
        false,
        context,
    );
    let _ = global_obj.set(
        js_string!("btoa"),
        native_to_jsfn(context, btoa),
        false,
        context,
    );

    // Storage: localStorage, sessionStorage.
    install_storage(
        context,
        &global_obj,
        "localStorage",
        win_cap.storage.clone(),
    );
    install_storage(
        context,
        &global_obj,
        "sessionStorage",
        win_cap.session.clone(),
    );

    // Location with plenty of fields.
    let location = ObjectInitializer::new(context)
        .property(
            js_string!("href"),
            js_string!("http://localhost/"),
            Attribute::all(),
        )
        .property(
            js_string!("origin"),
            js_string!("http://localhost"),
            Attribute::all(),
        )
        .property(
            js_string!("protocol"),
            js_string!("http:"),
            Attribute::all(),
        )
        .property(
            js_string!("host"),
            js_string!("localhost"),
            Attribute::all(),
        )
        .property(
            js_string!("hostname"),
            js_string!("localhost"),
            Attribute::all(),
        )
        .property(js_string!("port"), js_string!(""), Attribute::all())
        .property(js_string!("pathname"), js_string!("/"), Attribute::all())
        .property(js_string!("search"), js_string!(""), Attribute::all())
        .property(js_string!("hash"), js_string!(""), Attribute::all())
        .function(noop_native(), js_string!("assign"), 1)
        .function(noop_native(), js_string!("replace"), 1)
        .function(noop_native(), js_string!("reload"), 0)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
                Ok(JsValue::from(js_string!("http://localhost/")))
            }),
            js_string!("toString"),
            0,
        )
        .build();
    let _ = context.register_global_property(js_string!("location"), location, Attribute::all());

    // Navigator
    let navigator = ObjectInitializer::new(context)
        .property(
            js_string!("userAgent"),
            js_string!("Aurora/0.1"),
            Attribute::all(),
        )
        .property(
            js_string!("appName"),
            js_string!("Netscape"),
            Attribute::all(),
        )
        .property(
            js_string!("appVersion"),
            js_string!("5.0"),
            Attribute::all(),
        )
        .property(
            js_string!("platform"),
            js_string!("Linux x86_64"),
            Attribute::all(),
        )
        .property(
            js_string!("language"),
            js_string!("en-US"),
            Attribute::all(),
        )
        .property(js_string!("vendor"), js_string!(""), Attribute::all())
        .property(js_string!("onLine"), true, Attribute::all())
        .property(js_string!("cookieEnabled"), false, Attribute::all())
        .property(js_string!("doNotTrack"), js_string!("1"), Attribute::all())
        .property(js_string!("hardwareConcurrency"), 4, Attribute::all())
        .property(js_string!("maxTouchPoints"), 0, Attribute::all())
        .build();
    // languages array.
    if let Ok(langs) = JsArray::from_iter(
        [
            JsValue::from(js_string!("en-US")),
            JsValue::from(js_string!("en")),
        ],
        context,
    )
    .pipe(Ok::<_, JsError>)
    {
        let _ = navigator.set(js_string!("languages"), langs, false, context);
    }
    let _ = context.register_global_property(js_string!("navigator"), navigator, Attribute::all());

    // History
    let history = ObjectInitializer::new(context)
        .property(js_string!("length"), 1, Attribute::all())
        .property(js_string!("state"), JsValue::null(), Attribute::all())
        .function(noop_native(), js_string!("pushState"), 3)
        .function(noop_native(), js_string!("replaceState"), 3)
        .function(noop_native(), js_string!("back"), 0)
        .function(noop_native(), js_string!("forward"), 0)
        .function(noop_native(), js_string!("go"), 1)
        .build();
    let _ = context.register_global_property(js_string!("history"), history, Attribute::all());

    // Performance
    let perf = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(0.0))),
            js_string!("now"),
            0,
        )
        .function(noop_native(), js_string!("mark"), 1)
        .function(noop_native(), js_string!("measure"), 3)
        .function(noop_native(), js_string!("clearMarks"), 0)
        .function(noop_native(), js_string!("clearMeasures"), 0)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
            js_string!("getEntries"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
            js_string!("getEntriesByType"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
            js_string!("getEntriesByName"),
            2,
        )
        .build();
    let _ = perf.set(js_string!("timeOrigin"), 0.0, false, context);
    let _ = context.register_global_property(js_string!("performance"), perf, Attribute::all());

    // Crypto (stub): randomUUID returns a fixed-ish value.
    let crypto = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
                Ok(JsValue::from(js_string!(
                    "00000000-0000-0000-0000-000000000000"
                )))
            }),
            js_string!("randomUUID"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, args, _ctx| {
                // Return the same typed array unchanged.
                Ok(args.get(0).cloned().unwrap_or(JsValue::undefined()))
            }),
            js_string!("getRandomValues"),
            1,
        )
        .build();
    let _ = context.register_global_property(js_string!("crypto"), crypto, Attribute::all());

    // Event constructor (minimal).
    let event_ctor = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsString::from(type_name),
                Attribute::all(),
            )
            .property(js_string!("bubbles"), false, Attribute::all())
            .property(js_string!("cancelable"), false, Attribute::all())
            .property(js_string!("defaultPrevented"), false, Attribute::all())
            .function(noop_native(), js_string!("preventDefault"), 0)
            .function(noop_native(), js_string!("stopPropagation"), 0)
            .function(noop_native(), js_string!("stopImmediatePropagation"), 0)
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("Event"),
        native_to_jsfn(context, event_ctor),
        false,
        context,
    );
    let custom_event = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
        let obj = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsString::from(type_name),
                Attribute::all(),
            )
            .property(js_string!("detail"), JsValue::null(), Attribute::all())
            .function(noop_native(), js_string!("preventDefault"), 0)
            .function(noop_native(), js_string!("stopPropagation"), 0)
            .build();
        Ok(obj.into())
    });
    let _ = global_obj.set(
        js_string!("CustomEvent"),
        native_to_jsfn(context, custom_event),
        false,
        context,
    );

    // Silence the unused parameter warning from matching `document`.
    let _ = document;
}

fn install_dom_constructors(context: &mut Context) {
    let constructors = r#"
        (function(global) {
            function install(name, parentName) {
                if (typeof global[name] !== "function") {
                    global[name] = function() {};
                }
                if (!global[name].prototype) {
                    global[name].prototype = {};
                }
                if (parentName && global[parentName] && global[parentName].prototype) {
                    Object.setPrototypeOf(global[name].prototype, global[parentName].prototype);
                }
                global[name].prototype.constructor = global[name];
            }

            install("EventTarget");
            install("Node", "EventTarget");
            install("Document", "Node");
            install("DocumentFragment", "Node");
            install("CharacterData", "Node");
            install("Text", "CharacterData");
            install("Comment", "CharacterData");
            install("Element", "Node");
            install("HTMLElement", "Element");
            install("HTMLAnchorElement", "HTMLElement");
            install("HTMLBodyElement", "HTMLElement");
            install("HTMLDivElement", "HTMLElement");
            install("HTMLFormElement", "HTMLElement");
            install("HTMLHeadElement", "HTMLElement");
            install("HTMLHtmlElement", "HTMLElement");
            install("HTMLImageElement", "HTMLElement");
            install("HTMLInputElement", "HTMLElement");
            install("HTMLLinkElement", "HTMLElement");
            install("HTMLMetaElement", "HTMLElement");
            install("HTMLOptionElement", "HTMLElement");
            install("HTMLScriptElement", "HTMLElement");
            install("HTMLSelectElement", "HTMLElement");
            install("HTMLStyleElement", "HTMLElement");
            install("HTMLTableElement", "HTMLElement");
            install("HTMLTextAreaElement", "HTMLElement");
        })(globalThis);
    "#;
    let _ = context.eval(Source::from_bytes(constructors.as_bytes()));
}

fn set_object_prototype_from_constructor(
    obj: &JsObject,
    constructor_name: &str,
    context: &mut Context,
) {
    let global = context.global_object().clone();
    let Ok(constructor) = global.get(JsString::from(constructor_name), context) else {
        return;
    };
    let Some(constructor) = constructor.as_object() else {
        return;
    };
    let Ok(prototype) = constructor.get(js_string!("prototype"), context) else {
        return;
    };
    let Some(prototype) = prototype.as_object() else {
        return;
    };
    let _ = obj.set_prototype(Some(prototype.clone()));
}

fn constructor_for_node(node: &Node) -> &'static str {
    match node {
        Node::Document { .. } => "Document",
        Node::Text(_) => "Text",
        Node::Element(el) => match el.tag_name.as_str() {
            "#document-fragment" => "DocumentFragment",
            "a" => "HTMLAnchorElement",
            "body" => "HTMLBodyElement",
            "div" => "HTMLDivElement",
            "form" => "HTMLFormElement",
            "head" => "HTMLHeadElement",
            "html" => "HTMLHtmlElement",
            "img" => "HTMLImageElement",
            "input" => "HTMLInputElement",
            "link" => "HTMLLinkElement",
            "meta" => "HTMLMetaElement",
            "option" => "HTMLOptionElement",
            "script" => "HTMLScriptElement",
            "select" => "HTMLSelectElement",
            "style" => "HTMLStyleElement",
            "table" => "HTMLTableElement",
            "textarea" => "HTMLTextAreaElement",
            _ => "HTMLElement",
        },
    }
}

// Mini Pipe to keep the chain readable above.
trait Pipe: Sized {
    fn pipe<F, T>(self, f: F) -> T
    where
        F: FnOnce(Self) -> T,
    {
        f(self)
    }
}
impl<T> Pipe for T {}

fn install_storage(
    context: &mut Context,
    global: &JsObject,
    name: &str,
    backing: Rc<RefCell<BTreeMap<String, String>>>,
) {
    #[derive(Clone)]
    struct StorageCap(Rc<RefCell<BTreeMap<String, String>>>);
    unsafe impl Trace for StorageCap {
        empty_trace!();
    }
    impl Finalize for StorageCap {}

    let cap = StorageCap(backing);

    let get_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            match cap.0.borrow().get(&key) {
                Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );

    let set_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let val = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            cap.0.borrow_mut().insert(key, val);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let remove_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            cap.0.borrow_mut().remove(&key);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let clear = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &StorageCap, _ctx| {
            cap.0.borrow_mut().clear();
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let key_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let map = cap.0.borrow();
            match map.keys().nth(idx) {
                Some(k) => Ok(JsValue::from(JsString::from(k.clone()))),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );

    let storage = ObjectInitializer::new(context)
        .function(get_item, js_string!("getItem"), 1)
        .function(set_item, js_string!("setItem"), 2)
        .function(remove_item, js_string!("removeItem"), 1)
        .function(clear, js_string!("clear"), 0)
        .function(key_fn, js_string!("key"), 1)
        .property(js_string!("length"), 0, Attribute::all())
        .build();

    let _ = global.set(JsString::from(name), storage, false, context);
}

fn install_observers(context: &mut Context) {
    let global = context.global_object().clone();

    for name in [
        "MutationObserver",
        "IntersectionObserver",
        "ResizeObserver",
        "PerformanceObserver",
    ] {
        let ctor = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let obj = ObjectInitializer::new(ctx)
                .function(noop_native(), js_string!("observe"), 2)
                .function(noop_native(), js_string!("unobserve"), 1)
                .function(noop_native(), js_string!("disconnect"), 0)
                .function(
                    NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
                    js_string!("takeRecords"),
                    0,
                )
                .build();
            Ok(obj.into())
        });
        let _ = global.set(
            JsString::from(name),
            native_to_jsfn(context, ctor),
            false,
            context,
        );
    }
}

fn install_xhr_and_fetch(context: &mut Context) {
    let xhr_polyfill = r#"
        globalThis.XMLHttpRequest = function() {
            this.readyState = 0;
            this.status = 0;
            this.responseText = "";
            this.response = null;
            this.responseType = "";
            this.onreadystatechange = null;
            this.onload = null;
            this.onerror = null;
        };
        globalThis.XMLHttpRequest.prototype.open = function(method, url) {
            this._method = method;
            this._url = url;
            this.readyState = 1;
        };
        globalThis.XMLHttpRequest.prototype.send = function() {
            this.readyState = 4;
            this.status = 0;
            this.responseText = "";
            if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
            if (typeof this.onerror === 'function') this.onerror();
        };
        globalThis.XMLHttpRequest.prototype.setRequestHeader = function() {};
        globalThis.XMLHttpRequest.prototype.getResponseHeader = function() { return null; };
        globalThis.XMLHttpRequest.prototype.getAllResponseHeaders = function() { return ""; };
        globalThis.XMLHttpRequest.prototype.abort = function() {};
        globalThis.XMLHttpRequest.prototype.addEventListener = function() {};
        globalThis.XMLHttpRequest.prototype.removeEventListener = function() {};
        globalThis.XMLHttpRequest.UNSENT = 0;
        globalThis.XMLHttpRequest.OPENED = 1;
        globalThis.XMLHttpRequest.HEADERS_RECEIVED = 2;
        globalThis.XMLHttpRequest.LOADING = 3;
        globalThis.XMLHttpRequest.DONE = 4;

        // fetch returns a Promise that rejects — callers using .catch survive.
        globalThis.fetch = function(url) {
            return Promise.reject(new Error("Aurora: network fetch disabled in JS runtime"));
        };

        // Headers, Request, Response, URL(SearchParams), Blob, FormData, File, FileReader — minimal stubs.
        globalThis.Headers = function(init) {
            var m = {};
            if (init) for (var k in init) m[k.toLowerCase()] = init[k];
            this._m = m;
            this.get = function(k) { return m[(''+k).toLowerCase()] || null; };
            this.set = function(k, v) { m[(''+k).toLowerCase()] = ''+v; };
            this.has = function(k) { return (''+k).toLowerCase() in m; };
            this.append = this.set;
            this.delete = function(k) { delete m[(''+k).toLowerCase()]; };
            this.forEach = function(fn) { for (var k in m) fn(m[k], k, this); };
        };
        globalThis.Request = function(url, init) { this.url = url; this.method = (init && init.method) || 'GET'; };
        globalThis.Response = function(body, init) {
            this.body = body; this.status = (init && init.status) || 200; this.ok = this.status >= 200 && this.status < 300;
            this.text = function() { return Promise.resolve(String(body)); };
            this.json = function() { try { return Promise.resolve(JSON.parse(String(body))); } catch (e) { return Promise.reject(e); } };
            this.arrayBuffer = function() { return Promise.resolve(new ArrayBuffer(0)); };
            this.blob = function() { return Promise.resolve({}); };
        };
        globalThis.URL = function(u, base) {
            this.href = u; this.origin = ''; this.protocol = ''; this.host = ''; this.hostname = '';
            this.port = ''; this.pathname = ''; this.search = ''; this.hash = '';
            this.toString = function() { return this.href; };
        };
        globalThis.URL.createObjectURL = function() { return ""; };
        globalThis.URL.revokeObjectURL = function() {};
        globalThis.URLSearchParams = function(init) {
            var m = {}; if (typeof init === 'string') {
                init.replace(/^\?/, '').split('&').forEach(function(p){ if (!p) return; var i = p.indexOf('='); if (i<0) m[p]=''; else m[p.slice(0,i)] = decodeURIComponent(p.slice(i+1)); });
            }
            this._m = m;
            this.get = function(k){ return k in m ? m[k] : null; };
            this.set = function(k,v){ m[k] = ''+v; };
            this.has = function(k){ return k in m; };
            this.append = this.set;
            this.delete = function(k){ delete m[k]; };
            this.toString = function(){ var o=[]; for (var k in m) o.push(encodeURIComponent(k)+'='+encodeURIComponent(m[k])); return o.join('&'); };
            this.forEach = function(fn){ for (var k in m) fn(m[k], k, this); };
        };
        globalThis.Blob = function(parts, opts) { this.size = 0; this.type = (opts && opts.type) || ''; };
        globalThis.File = function(parts, name, opts) { globalThis.Blob.call(this, parts, opts); this.name = name; };
        globalThis.FormData = function() {
            var m = {};
            this.append = function(k,v){ m[k] = v; };
            this.get = function(k){ return k in m ? m[k] : null; };
            this.has = function(k){ return k in m; };
            this.delete = function(k){ delete m[k]; };
        };
        globalThis.FileReader = function() {
            this.readAsText = function(){};
            this.readAsDataURL = function(){};
            this.readAsArrayBuffer = function(){};
            this.onload = null;
            this.onerror = null;
        };
        globalThis.DOMParser = function() {
            this.parseFromString = function(str, type) {
                return { documentElement: null, body: null, head: null, querySelector: function(){return null;}, querySelectorAll: function(){return [];} };
            };
        };
        globalThis.AbortController = function() {
            this.signal = { aborted: false, addEventListener: function(){}, removeEventListener: function(){} };
            this.abort = function(){ this.signal.aborted = true; };
        };
        globalThis.WebSocket = function() { throw new Error("Aurora: WebSocket not supported"); };
        globalThis.Worker = function() { throw new Error("Aurora: Worker not supported"); };
        globalThis.SharedWorker = function() { throw new Error("Aurora: SharedWorker not supported"); };
    "#;
    let _ = context.eval(Source::from_bytes(xhr_polyfill.as_bytes()));
}

// ---------------------------------------------------------------------------
// document object.
// ---------------------------------------------------------------------------

fn install_document(context: &mut Context, document: &NodePtr, registry: &NodeRegistry) {
    let doc_cap = DocCapture {
        document: document.clone(),
        registry: registry.clone(),
    };

    let document_obj = ObjectInitializer::new(context)
        .property(
            js_string!("readyState"),
            js_string!("complete"),
            Attribute::all(),
        )
        .property(
            js_string!("compatMode"),
            js_string!("CSS1Compat"),
            Attribute::all(),
        )
        .property(js_string!("charset"), js_string!("UTF-8"), Attribute::all())
        .property(
            js_string!("contentType"),
            js_string!("text/html"),
            Attribute::all(),
        )
        .property(js_string!("cookie"), js_string!(""), Attribute::all())
        .property(js_string!("title"), js_string!(""), Attribute::all())
        .property(js_string!("referrer"), js_string!(""), Attribute::all())
        .property(
            js_string!("URL"),
            js_string!("http://localhost/"),
            Attribute::all(),
        )
        .property(
            js_string!("domain"),
            js_string!("localhost"),
            Attribute::all(),
        )
        .property(js_string!("hidden"), false, Attribute::all())
        .property(js_string!("nodeType"), 9, Attribute::all())
        .property(
            js_string!("nodeName"),
            js_string!("#document"),
            Attribute::all(),
        )
        .property(
            js_string!("visibilityState"),
            js_string!("visible"),
            Attribute::all(),
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let id = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    if let Some(node) = find_by_id(&cap.document, &id) {
                        Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                    } else {
                        Ok(JsValue::null())
                    }
                },
                doc_cap.clone(),
            ),
            js_string!("getElementById"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let tag =
                        js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                    let mut acc = Vec::new();
                    collect_by_tag(&cap.document, &tag, &mut acc);
                    build_nodelist(acc, &cap.registry, &cap.document, ctx)
                },
                doc_cap.clone(),
            ),
            js_string!("getElementsByTagName"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let mut acc = Vec::new();
                    collect_by_class(&cap.document, &cls, &mut acc);
                    build_nodelist(acc, &cap.registry, &cap.document, ctx)
                },
                doc_cap.clone(),
            ),
            js_string!("getElementsByClassName"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let mut acc = Vec::new();
                    collect_by_attr(&cap.document, "name", &name, &mut acc);
                    build_nodelist(acc, &cap.registry, &cap.document, ctx)
                },
                doc_cap.clone(),
            ),
            js_string!("getElementsByName"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    match query_first(&cap.document, &sel) {
                        Some(node) => Ok(create_js_node(node, &cap.registry, &cap.document, ctx)),
                        None => Ok(JsValue::null()),
                    }
                },
                doc_cap.clone(),
            ),
            js_string!("querySelector"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let found = query_all(&cap.document, &sel);
                    build_nodelist(found, &cap.registry, &cap.document, ctx)
                },
                doc_cap.clone(),
            ),
            js_string!("querySelectorAll"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let tag =
                        js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                    let node = Node::element(tag, vec![]);
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                },
                doc_cap.clone(),
            ),
            js_string!("createElement"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    // Namespaced element creation — ignore the namespace.
                    let tag =
                        js_string_of(args.get(1).unwrap_or(&JsValue::undefined())).to_lowercase();
                    let node = Node::element(tag, vec![]);
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                },
                doc_cap.clone(),
            ),
            js_string!("createElementNS"),
            2,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let node = Node::text(text);
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                },
                doc_cap.clone(),
            ),
            js_string!("createTextNode"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let node = Node::Text(text);
                    let node = Rc::new(RefCell::new(node));
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                },
                doc_cap.clone(),
            ),
            js_string!("createComment"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, _args, cap: &DocCapture, ctx| {
                    let node = Node::element("#document-fragment", vec![]);
                    Ok(create_js_node(node, &cap.registry, &cap.document, ctx))
                },
                doc_cap.clone(),
            ),
            js_string!("createDocumentFragment"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, _args, _cap: &DocCapture, ctx| {
                    let obj = ObjectInitializer::new(ctx)
                        .property(js_string!("type"), js_string!(""), Attribute::all())
                        .function(noop_native(), js_string!("preventDefault"), 0)
                        .function(noop_native(), js_string!("stopPropagation"), 0)
                        .function(noop_native(), js_string!("initEvent"), 3)
                        .build();
                    Ok(obj.into())
                },
                doc_cap.clone(),
            ),
            js_string!("createEvent"),
            1,
        )
        .function(noop_native(), js_string!("addEventListener"), 2)
        .function(noop_native(), js_string!("removeEventListener"), 2)
        .function(return_bool(true), js_string!("dispatchEvent"), 1)
        .function(noop_native(), js_string!("open"), 0)
        .function(noop_native(), js_string!("close"), 0)
        .function(noop_native(), js_string!("write"), 1)
        .function(noop_native(), js_string!("writeln"), 1)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!("")))),
            js_string!("execCommand"),
            3,
        )
        .function(return_bool(false), js_string!("hasFocus"), 0)
        .build();

    // Set dynamic document fields: documentElement/body/head/forms/links/images.
    let html_root = find_by_tag(document, "html")
        .or_else(|| Some(document.clone()))
        .unwrap();
    let body = find_by_tag(document, "body").unwrap_or_else(|| html_root.clone());
    let head = find_by_tag(document, "head").unwrap_or_else(|| html_root.clone());

    let body_js = create_js_node(body.clone(), registry, document, context);
    let head_js = create_js_node(head.clone(), registry, document, context);
    let root_js = create_js_node(html_root.clone(), registry, document, context);

    let _ = document_obj.set(js_string!("body"), body_js, false, context);
    let _ = document_obj.set(js_string!("head"), head_js, false, context);
    let _ = document_obj.set(js_string!("documentElement"), root_js, false, context);
    let _ = document_obj.set(
        js_string!("scrollingElement"),
        JsValue::null(),
        false,
        context,
    );
    let _ = document_obj.set(js_string!("activeElement"), JsValue::null(), false, context);
    let _ = document_obj.set(
        js_string!("defaultView"),
        context.global_object().clone(),
        false,
        context,
    );

    let mut forms_vec = Vec::new();
    collect_by_tag(document, "form", &mut forms_vec);
    if let Ok(arr) = build_nodelist(forms_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("forms"), arr, false, context);
    }
    let mut links_vec = Vec::new();
    collect_by_tag(document, "a", &mut links_vec);
    if let Ok(arr) = build_nodelist(links_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("links"), arr, false, context);
    }
    let mut images_vec = Vec::new();
    collect_by_tag(document, "img", &mut images_vec);
    if let Ok(arr) = build_nodelist(images_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("images"), arr, false, context);
    }
    let mut scripts_vec = Vec::new();
    collect_by_tag(document, "script", &mut scripts_vec);
    if let Ok(arr) = build_nodelist(scripts_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("scripts"), arr, false, context);
    }

    // title mirror
    if let Some(title_node) = find_by_tag(document, "title") {
        let text = collect_text(&title_node);
        let _ = document_obj.set(js_string!("title"), JsString::from(text), false, context);
    }

    let implementation = build_document_implementation(document, registry, context);
    let _ = document_obj.set(js_string!("implementation"), implementation, false, context);
    set_object_prototype_from_constructor(&document_obj, "Document", context);

    let _ =
        context.register_global_property(js_string!("document"), document_obj, Attribute::all());
}

fn build_document_implementation(
    document: &NodePtr,
    registry: &NodeRegistry,
    context: &mut Context,
) -> JsObject {
    let doc_cap = DocCapture {
        document: document.clone(),
        registry: registry.clone(),
    };

    ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let title = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let title_node = Node::element("title", vec![Node::text(title)]);
                    let head = Node::element("head", vec![title_node]);
                    let body = Node::element("body", vec![]);
                    let html = Node::element("html", vec![head, body]);
                    let doc = Node::document(vec![html]);
                    let doc_obj = create_js_node(doc, &cap.registry, &cap.document, ctx);
                    Ok(doc_obj)
                },
                doc_cap,
            ),
            js_string!("createHTMLDocument"),
            1,
        )
        .function(return_bool(true), js_string!("hasFeature"), 2)
        .build()
}

// ---------------------------------------------------------------------------
// Node JS object factory.
// ---------------------------------------------------------------------------

fn create_js_node(
    node: NodePtr,
    registry: &NodeRegistry,
    document: &NodePtr,
    context: &mut Context,
) -> JsValue {
    let id = registry.register(node.clone());

    let cap = NodeCapture {
        node: node.clone(),
        registry: registry.clone(),
        document: document.clone(),
    };

    // Compute static tag/node info now.
    let (tag_name, node_type, node_name): (String, i32, String) = {
        let b = node.borrow();
        match &*b {
            Node::Element(el) => (el.tag_name.clone(), 1, el.tag_name.to_uppercase()),
            Node::Text(_) => (String::new(), 3, "#text".to_string()),
            Node::Document { .. } => (String::new(), 9, "#document".to_string()),
        }
    };

    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__node_id"),
        id,
        Attribute::READONLY | Attribute::NON_ENUMERABLE,
    );
    init.property(
        js_string!("tagName"),
        JsString::from(tag_name.to_uppercase()),
        Attribute::all(),
    );
    init.property(
        js_string!("localName"),
        JsString::from(tag_name.to_lowercase()),
        Attribute::all(),
    );
    init.property(
        js_string!("nodeName"),
        JsString::from(node_name.clone()),
        Attribute::all(),
    );
    init.property(js_string!("nodeType"), node_type, Attribute::all());
    init.property(
        js_string!("namespaceURI"),
        JsValue::null(),
        Attribute::all(),
    );
    init.property(js_string!("prefix"), JsValue::null(), Attribute::all());
    init.property(
        js_string!("baseURI"),
        js_string!("http://localhost/"),
        Attribute::all(),
    );
    init.property(
        js_string!("ownerDocument"),
        JsValue::null(),
        Attribute::all(),
    );
    init.property(js_string!("isConnected"), true, Attribute::all());
    init.property(js_string!("scrollTop"), 0, Attribute::all());
    init.property(js_string!("scrollLeft"), 0, Attribute::all());
    init.property(js_string!("scrollWidth"), 0, Attribute::all());
    init.property(js_string!("scrollHeight"), 0, Attribute::all());
    init.property(js_string!("clientWidth"), 0, Attribute::all());
    init.property(js_string!("clientHeight"), 0, Attribute::all());
    init.property(js_string!("clientTop"), 0, Attribute::all());
    init.property(js_string!("clientLeft"), 0, Attribute::all());
    init.property(js_string!("offsetTop"), 0, Attribute::all());
    init.property(js_string!("offsetLeft"), 0, Attribute::all());
    init.property(js_string!("offsetWidth"), 0, Attribute::all());
    init.property(js_string!("offsetHeight"), 0, Attribute::all());

    // appendChild(child)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let Some(child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) else {
                    return Ok(args.get(0).cloned().unwrap_or(JsValue::null()));
                };
                append_child_ptr(&cap.node, &child);
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("appendChild"),
        1,
    );

    // insertBefore(newChild, refChild)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let Some(new_child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) else {
                    return Ok(JsValue::null());
                };
                let ref_child = args
                    .get(1)
                    .and_then(|v| node_from_js(v, &cap.registry, ctx));
                insert_before_ptr(&cap.node, &new_child, ref_child.as_ref());
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("insertBefore"),
        2,
    );

    // removeChild(child)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(child) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    remove_child_ptr(&cap.node, &child);
                }
                Ok(args.get(0).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("removeChild"),
        1,
    );

    // replaceChild(newChild, oldChild)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let new_child = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                );
                let old_child = node_from_js(
                    args.get(1).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                );
                if let (Some(new_c), Some(old_c)) = (new_child, old_child) {
                    replace_child_ptr(&cap.node, &new_c, &old_c);
                }
                Ok(args.get(1).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("replaceChild"),
        2,
    );

    // remove() — detach from parent.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, _ctx| {
                if let Some(parent) = find_parent(&cap.document, &cap.node) {
                    remove_child_ptr(&parent, &cap.node);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("remove"),
        0,
    );

    // cloneNode(deep)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let deep = args.get(0).map(|v| v.to_boolean()).unwrap_or(false);
                let cloned = clone_node(&cap.node, deep);
                Ok(create_js_node(cloned, &cap.registry, &cap.document, ctx))
            },
            cap.clone(),
        ),
        js_string!("cloneNode"),
        1,
    );

    // contains(other)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(other) = node_from_js(
                    args.get(0).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    Ok(JsValue::from(contains_ptr(&cap.node, &other)))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("contains"),
        1,
    );

    // setAttribute(name, value)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let value = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.insert(name, value);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("setAttribute"),
        2,
    );

    // getAttribute(name)
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    match el.attributes.get(&name) {
                        Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                        None => Ok(JsValue::null()),
                    }
                } else {
                    Ok(JsValue::null())
                }
            },
            cap.clone(),
        ),
        js_string!("getAttribute"),
        1,
    );

    // removeAttribute
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                    el.attributes.remove(&name);
                }
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("removeAttribute"),
        1,
    );

    // hasAttribute
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let name = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    Ok(JsValue::from(el.attributes.contains_key(&name)))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("hasAttribute"),
        1,
    );

    // hasAttributes
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, _ctx| {
                let b = cap.node.borrow();
                if let Node::Element(el) = &*b {
                    Ok(JsValue::from(!el.attributes.is_empty()))
                } else {
                    Ok(JsValue::from(false))
                }
            },
            cap.clone(),
        ),
        js_string!("hasAttributes"),
        0,
    );

    // getAttributeNames → array of strings
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, _args, cap: &NodeCapture, ctx| {
                let names: Vec<JsValue> = {
                    let b = cap.node.borrow();
                    if let Node::Element(el) = &*b {
                        el.attributes
                            .keys()
                            .map(|k| JsValue::from(JsString::from(k.clone())))
                            .collect()
                    } else {
                        Vec::new()
                    }
                };
                Ok(JsArray::from_iter(names, ctx).into())
            },
            cap.clone(),
        ),
        js_string!("getAttributeNames"),
        0,
    );

    // querySelector / querySelectorAll
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                match query_first(&cap.node, &sel) {
                    Some(n) => Ok(create_js_node(n, &cap.registry, &cap.document, ctx)),
                    None => Ok(JsValue::null()),
                }
            },
            cap.clone(),
        ),
        js_string!("querySelector"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let found = query_all(&cap.node, &sel);
                build_nodelist(found, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("querySelectorAll"),
        1,
    );

    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let tag = js_string_of(args.get(0).unwrap_or(&JsValue::undefined())).to_lowercase();
                let mut acc = Vec::new();
                collect_by_tag(&cap.node, &tag, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("getElementsByTagName"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut acc = Vec::new();
                collect_by_class(&cap.node, &cls, &mut acc);
                build_nodelist(acc, &cap.registry, &cap.document, ctx)
            },
            cap.clone(),
        ),
        js_string!("getElementsByClassName"),
        1,
    );

    // matches(selector) — best-effort.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                Ok(JsValue::from(selector_matches(&cap.node, &sel)))
            },
            cap.clone(),
        ),
        js_string!("matches"),
        1,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                let sel = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                let mut current = Some(cap.node.clone());
                while let Some(n) = current {
                    if selector_matches(&n, &sel) {
                        return Ok(create_js_node(n, &cap.registry, &cap.document, ctx));
                    }
                    current = find_parent(&cap.document, &n);
                }
                Ok(JsValue::null())
            },
            cap.clone(),
        ),
        js_string!("closest"),
        1,
    );

    // getBoundingClientRect
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| {
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("x"), 0, Attribute::all())
                .property(js_string!("y"), 0, Attribute::all())
                .property(js_string!("top"), 0, Attribute::all())
                .property(js_string!("right"), 0, Attribute::all())
                .property(js_string!("bottom"), 0, Attribute::all())
                .property(js_string!("left"), 0, Attribute::all())
                .property(js_string!("width"), 0, Attribute::all())
                .property(js_string!("height"), 0, Attribute::all())
                .build();
            Ok(obj.into())
        }),
        js_string!("getBoundingClientRect"),
        0,
    );
    init.function(
        NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx).into())),
        js_string!("getClientRects"),
        0,
    );

    // insertAdjacentHTML / insertAdjacentElement / insertAdjacentText — stubs that append.
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentHTML"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, _ctx| {
                let text = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
                let text_node = Node::text(text);
                append_child_ptr(&cap.node, &text_node);
                Ok(JsValue::undefined())
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentText"),
        2,
    );
    init.function(
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, cap: &NodeCapture, ctx| {
                if let Some(el) = node_from_js(
                    args.get(1).unwrap_or(&JsValue::undefined()),
                    &cap.registry,
                    ctx,
                ) {
                    append_child_ptr(&cap.node, &el);
                }
                Ok(args.get(1).cloned().unwrap_or(JsValue::null()))
            },
            cap.clone(),
        ),
        js_string!("insertAdjacentElement"),
        2,
    );

    // Event listeners / dispatch — stubs.
    init.function(noop_native(), js_string!("addEventListener"), 3);
    init.function(noop_native(), js_string!("removeEventListener"), 3);
    init.function(return_bool(true), js_string!("dispatchEvent"), 1);

    // focus / blur / click — stubs.
    init.function(noop_native(), js_string!("focus"), 0);
    init.function(noop_native(), js_string!("blur"), 0);
    init.function(noop_native(), js_string!("click"), 0);
    init.function(noop_native(), js_string!("scrollIntoView"), 0);
    init.function(noop_native(), js_string!("scrollTo"), 0);
    init.function(noop_native(), js_string!("scrollBy"), 0);

    // normalize — no-op.
    init.function(noop_native(), js_string!("normalize"), 0);

    // Read/write property hacks for JS code that assigns .textContent = "x".
    // Because we can't easily install accessors here without more plumbing,
    // we also expose setText/setHtml and explicit getters. But we also install
    // an accessor for textContent via a tiny post-build step below.
    let obj = init.build();
    let constructor_name = {
        let b = cap.node.borrow();
        constructor_for_node(&b)
    };
    set_object_prototype_from_constructor(&obj, constructor_name, context);

    // Dynamic accessor installation (textContent, innerHTML, id, className, children, etc.)
    install_accessors(&obj, &cap, context);

    // attributes: snapshot as an object with name/value entries. Not fully-live.
    {
        let b = cap.node.borrow();
        if let Node::Element(el) = &*b {
            let mut attrs_init = ObjectInitializer::new(context);
            attrs_init.property(
                js_string!("length"),
                el.attributes.len() as u32,
                Attribute::all(),
            );
            for (k, v) in &el.attributes {
                attrs_init.property(
                    JsString::from(k.clone()),
                    JsString::from(v.clone()),
                    Attribute::all(),
                );
            }
            let attrs = attrs_init.build();
            let _ = obj.set(js_string!("attributes"), attrs, false, context);
        }
    }

    // For text nodes: data / nodeValue / length
    if node_type == 3 {
        let text_val = {
            let b = cap.node.borrow();
            if let Node::Text(t) = &*b {
                t.clone()
            } else {
                String::new()
            }
        };
        let _ = obj.set(
            js_string!("data"),
            JsString::from(text_val.clone()),
            false,
            context,
        );
        let _ = obj.set(
            js_string!("nodeValue"),
            JsString::from(text_val.clone()),
            false,
            context,
        );
        let _ = obj.set(
            js_string!("length"),
            text_val.chars().count() as u32,
            false,
            context,
        );
    }

    install_element_reflection_properties(&obj, &cap, context);

    obj.into()
}

fn install_element_reflection_properties(obj: &JsObject, cap: &NodeCapture, context: &mut Context) {
    let tag_name = {
        let b = cap.node.borrow();
        match &*b {
            Node::Element(el) => el.tag_name.clone(),
            _ => return,
        }
    };

    for attr in [
        "type", "name", "value", "href", "src", "rel", "target", "alt",
    ] {
        install_attribute_reflector(obj, cap, context, attr, attr, "");
    }

    if tag_name == "input" {
        install_bool_attribute_reflector(obj, cap, context, "checked", "checked");
        install_bool_attribute_reflector(obj, cap, context, "disabled", "disabled");
        install_bool_attribute_reflector(obj, cap, context, "selected", "selected");
    }
}

fn install_attribute_reflector(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
    property: &str,
    attribute: &str,
    fallback: &'static str,
) {
    #[derive(Clone)]
    struct AttrCap {
        node: NodePtr,
        attribute: String,
        fallback: &'static str,
    }
    unsafe impl Trace for AttrCap {
        empty_trace!();
    }
    impl Finalize for AttrCap {}

    let attr_cap = AttrCap {
        node: cap.node.clone(),
        attribute: attribute.to_string(),
        fallback,
    };
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &AttrCap, _ctx| {
            let b = cap.node.borrow();
            let value = match &*b {
                Node::Element(el) => el
                    .attributes
                    .get(&cap.attribute)
                    .cloned()
                    .unwrap_or_else(|| cap.fallback.to_string()),
                _ => cap.fallback.to_string(),
            };
            Ok(JsValue::from(JsString::from(value)))
        },
        attr_cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &AttrCap, _ctx| {
            let value = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert(cap.attribute.clone(), value);
            }
            Ok(JsValue::undefined())
        },
        attr_cap,
    );
    install_accessor(obj, context, property, Some(getter), Some(setter));
}

fn install_bool_attribute_reflector(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
    property: &str,
    attribute: &str,
) {
    #[derive(Clone)]
    struct BoolAttrCap {
        node: NodePtr,
        attribute: String,
    }
    unsafe impl Trace for BoolAttrCap {
        empty_trace!();
    }
    impl Finalize for BoolAttrCap {}

    let attr_cap = BoolAttrCap {
        node: cap.node.clone(),
        attribute: attribute.to_string(),
    };
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &BoolAttrCap, _ctx| {
            let b = cap.node.borrow();
            let present = match &*b {
                Node::Element(el) => el.attributes.contains_key(&cap.attribute),
                _ => false,
            };
            Ok(JsValue::from(present))
        },
        attr_cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &BoolAttrCap, _ctx| {
            let enabled = args.get(0).map(|v| v.to_boolean()).unwrap_or(false);
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                if enabled {
                    el.attributes
                        .insert(cap.attribute.clone(), cap.attribute.clone());
                } else {
                    el.attributes.remove(&cap.attribute);
                }
            }
            Ok(JsValue::undefined())
        },
        attr_cap,
    );
    install_accessor(obj, context, property, Some(getter), Some(setter));
}

// Install getter/setter accessors for properties that depend on the live DOM.
fn install_accessors(obj: &JsObject, cap: &NodeCapture, context: &mut Context) {
    // textContent
    let tc_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(collect_text(&cap.node))))
        },
        cap.clone(),
    );
    let tc_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            set_text_content(&cap.node, &text);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "textContent", Some(tc_get), Some(tc_set));

    // innerText (alias — simplified)
    let it_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(collect_text(&cap.node))))
        },
        cap.clone(),
    );
    let it_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            set_text_content(&cap.node, &text);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "innerText", Some(it_get), Some(it_set));

    // innerHTML: getter returns textual serialization; setter parses as plain text.
    let ih_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(serialize_inner_html(
                &cap.node,
            ))))
        },
        cap.clone(),
    );
    let ih_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let text = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            // Minimal: treat as HTML fragment via the existing Parser.
            let parsed = crate::html::Parser::new(&text).parse_document();
            let new_children: Vec<NodePtr> = match &*parsed.borrow() {
                Node::Document { children } => children.clone(),
                _ => Vec::new(),
            };
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.children = new_children;
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "innerHTML", Some(ih_get), Some(ih_set));

    // outerHTML — readonly-ish.
    let oh_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            Ok(JsValue::from(JsString::from(serialize_outer_html(
                &cap.node,
            ))))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "outerHTML", Some(oh_get), None);

    // id
    let id_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            let v = if let Node::Element(el) = &*b {
                el.attributes.get("id").cloned().unwrap_or_default()
            } else {
                String::new()
            };
            Ok(JsValue::from(JsString::from(v)))
        },
        cap.clone(),
    );
    let id_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let v = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert("id".to_string(), v);
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "id", Some(id_get), Some(id_set));

    // className
    let cn_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            let v = if let Node::Element(el) = &*b {
                el.attributes.get("class").cloned().unwrap_or_default()
            } else {
                String::new()
            };
            Ok(JsValue::from(JsString::from(v)))
        },
        cap.clone(),
    );
    let cn_set = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let v = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            if let Node::Element(el) = &mut *cap.node.borrow_mut() {
                el.attributes.insert("class".to_string(), v);
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "className", Some(cn_get), Some(cn_set));

    // parentNode / parentElement
    let p_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match find_parent(&cap.document, &cap.node) {
            Some(p) => Ok(create_js_node(p, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "parentNode", Some(p_get), None);
    let p_get2 = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match find_parent(&cap.document, &cap.node) {
            Some(p) => {
                let is_elem = matches!(&*p.borrow(), Node::Element(_));
                if is_elem {
                    Ok(create_js_node(p, &cap.registry, &cap.document, ctx))
                } else {
                    Ok(JsValue::null())
                }
            }
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "parentElement", Some(p_get2), None);

    // children (elements only), childNodes (all)
    let ch_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kids: Vec<NodePtr> = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el
                        .children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .cloned()
                        .collect(),
                    Node::Document { children } => children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .cloned()
                        .collect(),
                    _ => Vec::new(),
                }
            };
            build_nodelist(kids, &cap.registry, &cap.document, ctx)
        },
        cap.clone(),
    );
    install_accessor(obj, context, "children", Some(ch_get), None);

    let cn2_get = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kids: Vec<NodePtr> = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el.children.clone(),
                    Node::Document { children } => children.clone(),
                    _ => Vec::new(),
                }
            };
            build_nodelist(kids, &cap.registry, &cap.document, ctx)
        },
        cap.clone(),
    );
    install_accessor(obj, context, "childNodes", Some(cn2_get), None);

    // firstChild / lastChild / firstElementChild / lastElementChild
    let fc = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| {
            let kid = first_child(&cap.node);
            match kid {
                Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );
    install_accessor(obj, context, "firstChild", Some(fc), None);

    let lc = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match last_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "lastChild", Some(lc), None);

    let fec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match first_element_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "firstElementChild", Some(fec), None);

    let lec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match last_element_child(&cap.node) {
            Some(k) => Ok(create_js_node(k, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "lastElementChild", Some(lec), None);

    // nextSibling / previousSibling / nextElementSibling / previousElementSibling
    let ns = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, 1, false) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "nextSibling", Some(ns), None);

    let ps = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, -1, false) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "previousSibling", Some(ps), None);

    let nes = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, 1, true) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "nextElementSibling", Some(nes), None);

    let pes = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, ctx| match sibling(&cap.document, &cap.node, -1, true) {
            Some(s) => Ok(create_js_node(s, &cap.registry, &cap.document, ctx)),
            None => Ok(JsValue::null()),
        },
        cap.clone(),
    );
    install_accessor(obj, context, "previousElementSibling", Some(pes), None);

    // childElementCount
    let cec = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let count = {
                let b = cap.node.borrow();
                match &*b {
                    Node::Element(el) => el
                        .children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .count(),
                    Node::Document { children } => children
                        .iter()
                        .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                        .count(),
                    _ => 0,
                }
            };
            Ok(JsValue::from(count as u32))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "childElementCount", Some(cec), None);

    // style: per-node backing map exposed as a plain object with methods.
    let style_obj = build_style_object(cap.clone(), context);
    let _ = obj.set(js_string!("style"), style_obj, false, context);

    // classList
    let cl_obj = build_classlist_object(cap.clone(), context);
    let _ = obj.set(js_string!("classList"), cl_obj, false, context);

    // dataset — flat object mirroring data-* attributes.
    let dataset = {
        let b = cap.node.borrow();
        let mut init = ObjectInitializer::new(context);
        if let Node::Element(el) = &*b {
            for (k, v) in &el.attributes {
                if let Some(rest) = k.strip_prefix("data-") {
                    let camel = kebab_to_camel(rest);
                    init.property(
                        JsString::from(camel),
                        JsString::from(v.clone()),
                        Attribute::all(),
                    );
                }
            }
        }
        init.build()
    };
    let _ = obj.set(js_string!("dataset"), dataset, false, context);
}

fn install_accessor(
    obj: &JsObject,
    context: &mut Context,
    name: &str,
    getter: Option<NativeFunction>,
    setter: Option<NativeFunction>,
) {
    use boa_engine::property::PropertyDescriptorBuilder;

    let mut builder = PropertyDescriptorBuilder::new()
        .configurable(true)
        .enumerable(true);

    if let Some(g) = getter {
        let f = FunctionObjectBuilder::new(context.realm(), g)
            .name(JsString::from(name))
            .length(0)
            .constructor(false)
            .build();
        builder = builder.get(f);
    }
    if let Some(s) = setter {
        let f = FunctionObjectBuilder::new(context.realm(), s)
            .name(JsString::from(name))
            .length(1)
            .constructor(false)
            .build();
        builder = builder.set(f);
    }

    let descriptor = builder.build();
    let _ = obj.define_property_or_throw(JsString::from(name), descriptor, context);
}

fn build_style_object(cap: NodeCapture, context: &mut Context) -> JsObject {
    #[derive(Clone)]
    struct StyleCap {
        style: Rc<RefCell<BTreeMap<String, String>>>,
    }
    unsafe impl Trace for StyleCap {
        empty_trace!();
    }
    impl Finalize for StyleCap {}

    let style = Rc::new(RefCell::new(BTreeMap::<String, String>::new()));
    // Seed style map from the element's style attribute if present.
    {
        let b = cap.node.borrow();
        if let Node::Element(el) = &*b {
            if let Some(css) = el.attributes.get("style") {
                for part in css.split(';') {
                    if let Some((k, v)) = part.split_once(':') {
                        style
                            .borrow_mut()
                            .insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
        }
    }

    let scap = StyleCap {
        style: style.clone(),
    };

    let get_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            match cap.style.borrow().get(&k) {
                Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                None => Ok(JsValue::from(js_string!(""))),
            }
        },
        scap.clone(),
    );
    let set_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let v = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            cap.style.borrow_mut().insert(k, v);
            Ok(JsValue::undefined())
        },
        scap.clone(),
    );
    let remove_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            cap.style.borrow_mut().remove(&k);
            Ok(JsValue::undefined())
        },
        scap.clone(),
    );
    let item_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let m = cap.style.borrow();
            match m.keys().nth(idx) {
                Some(k) => Ok(JsValue::from(JsString::from(k.clone()))),
                None => Ok(JsValue::from(js_string!(""))),
            }
        },
        scap.clone(),
    );

    let obj = ObjectInitializer::new(context)
        .function(get_prop, js_string!("getPropertyValue"), 1)
        .function(set_prop, js_string!("setProperty"), 2)
        .function(remove_prop, js_string!("removeProperty"), 1)
        .function(item_fn, js_string!("item"), 1)
        .property(js_string!("cssText"), js_string!(""), Attribute::all())
        .property(js_string!("length"), 0, Attribute::all())
        .build();

    obj
}

fn build_classlist_object(cap: NodeCapture, context: &mut Context) -> JsObject {
    let add = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            for a in args {
                let cls = js_string_of(a);
                classlist_modify(&cap.node, |set| {
                    set.insert(cls.clone());
                });
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    let remove = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            for a in args {
                let cls = js_string_of(a);
                classlist_modify(&cap.node, |set| {
                    set.remove(&cls);
                });
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    let contains = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    return Ok(JsValue::from(v.split_whitespace().any(|c| c == cls)));
                }
            }
            Ok(JsValue::from(false))
        },
        cap.clone(),
    );
    let toggle = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let mut present = false;
            classlist_modify(&cap.node, |set| {
                if set.contains(&cls) {
                    set.remove(&cls);
                } else {
                    set.insert(cls.clone());
                    present = true;
                }
            });
            Ok(JsValue::from(present))
        },
        cap.clone(),
    );
    let replace = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let old_cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let new_cls = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            classlist_modify(&cap.node, |set| {
                if set.remove(&old_cls) {
                    set.insert(new_cls.clone());
                }
            });
            Ok(JsValue::from(true))
        },
        cap.clone(),
    );
    let item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    if let Some(cls) = v.split_whitespace().nth(idx) {
                        return Ok(JsValue::from(JsString::from(cls.to_string())));
                    }
                }
            }
            Ok(JsValue::null())
        },
        cap.clone(),
    );
    let to_string = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    return Ok(JsValue::from(JsString::from(v.clone())));
                }
            }
            Ok(JsValue::from(js_string!("")))
        },
        cap.clone(),
    );

    ObjectInitializer::new(context)
        .function(add, js_string!("add"), 1)
        .function(remove, js_string!("remove"), 1)
        .function(contains, js_string!("contains"), 1)
        .function(toggle, js_string!("toggle"), 1)
        .function(replace, js_string!("replace"), 2)
        .function(item, js_string!("item"), 1)
        .function(to_string, js_string!("toString"), 0)
        .build()
}

fn classlist_modify<F: FnOnce(&mut std::collections::BTreeSet<String>)>(node: &NodePtr, f: F) {
    use std::collections::BTreeSet;
    if let Node::Element(el) = &mut *node.borrow_mut() {
        let mut set: BTreeSet<String> = el
            .attributes
            .get("class")
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        f(&mut set);
        let joined = set.into_iter().collect::<Vec<_>>().join(" ");
        if joined.is_empty() {
            el.attributes.remove("class");
        } else {
            el.attributes.insert("class".to_string(), joined);
        }
    }
}

// ---------------------------------------------------------------------------
// DOM traversal / mutation helpers.
// ---------------------------------------------------------------------------

fn find_by_id(node: &NodePtr, id: &str) -> Option<NodePtr> {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.attributes.get("id").map(|s| s.as_str()) == Some(id) {
                drop(b);
                return Some(node.clone());
            }
            for c in &el.children {
                if let Some(found) = find_by_id(c, id) {
                    return Some(found);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if let Some(found) = find_by_id(c, id) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn find_by_tag(node: &NodePtr, tag: &str) -> Option<NodePtr> {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.tag_name.eq_ignore_ascii_case(tag) {
                drop(b);
                return Some(node.clone());
            }
            for c in &el.children {
                if let Some(found) = find_by_tag(c, tag) {
                    return Some(found);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if let Some(found) = find_by_tag(c, tag) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn collect_by_tag(node: &NodePtr, tag: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if tag == "*" || el.tag_name.eq_ignore_ascii_case(tag) {
                out.push(node.clone());
            }
            for c in &el.children {
                collect_by_tag(c, tag, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_tag(c, tag, out);
            }
        }
        _ => {}
    }
}

fn collect_by_class(node: &NodePtr, cls: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if let Some(v) = el.attributes.get("class") {
                if v.split_whitespace().any(|c| c == cls) {
                    out.push(node.clone());
                }
            }
            for c in &el.children {
                collect_by_class(c, cls, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_class(c, cls, out);
            }
        }
        _ => {}
    }
}

fn collect_by_attr(node: &NodePtr, key: &str, value: &str, out: &mut Vec<NodePtr>) {
    let b = node.borrow();
    match &*b {
        Node::Element(el) => {
            if el.attributes.get(key).map(|s| s.as_str()) == Some(value) {
                out.push(node.clone());
            }
            for c in &el.children {
                collect_by_attr(c, key, value, out);
            }
        }
        Node::Document { children } => {
            for c in children {
                collect_by_attr(c, key, value, out);
            }
        }
        _ => {}
    }
}

fn collect_text(node: &NodePtr) -> String {
    let b = node.borrow();
    match &*b {
        Node::Text(t) => t.clone(),
        Node::Element(el) => el
            .children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
        Node::Document { children } => children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn set_text_content(node: &NodePtr, text: &str) {
    let new_text = Node::text(text.to_string());
    if let Node::Element(el) = &mut *node.borrow_mut() {
        el.children = vec![new_text];
    }
}

fn append_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Node::Element(el) = &mut *parent.borrow_mut() {
        el.children.push(child.clone());
    } else if let Node::Document { children } = &mut *parent.borrow_mut() {
        children.push(child.clone());
    }
}

fn insert_before_ptr(parent: &NodePtr, new_child: &NodePtr, ref_child: Option<&NodePtr>) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    if let Some(rc) = ref_child {
        if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, rc)) {
            kids.insert(pos, new_child.clone());
            return;
        }
    }
    kids.push(new_child.clone());
}

fn remove_child_ptr(parent: &NodePtr, child: &NodePtr) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    kids.retain(|c| !Rc::ptr_eq(c, child));
}

fn replace_child_ptr(parent: &NodePtr, new_child: &NodePtr, old_child: &NodePtr) {
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children } => children,
        _ => return,
    };
    if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
        kids[pos] = new_child.clone();
    }
}

fn first_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.first().cloned(),
        Node::Document { children } => children.first().cloned(),
        _ => None,
    }
}
fn last_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.last().cloned(),
        Node::Document { children } => children.last().cloned(),
        _ => None,
    }
}
fn first_element_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el
            .children
            .iter()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        Node::Document { children } => children
            .iter()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        _ => None,
    }
}
fn last_element_child(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el
            .children
            .iter()
            .rev()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        Node::Document { children } => children
            .iter()
            .rev()
            .find(|c| matches!(&*c.borrow(), Node::Element(_)))
            .cloned(),
        _ => None,
    }
}

fn find_parent(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
    let b = root.borrow();
    match &*b {
        Node::Element(el) => {
            for c in &el.children {
                if Rc::ptr_eq(c, target) {
                    drop(b);
                    return Some(root.clone());
                }
                if let Some(p) = find_parent(c, target) {
                    return Some(p);
                }
            }
            None
        }
        Node::Document { children } => {
            for c in children {
                if Rc::ptr_eq(c, target) {
                    drop(b);
                    return Some(root.clone());
                }
                if let Some(p) = find_parent(c, target) {
                    return Some(p);
                }
            }
            None
        }
        _ => None,
    }
}

fn sibling(root: &NodePtr, target: &NodePtr, delta: i32, element_only: bool) -> Option<NodePtr> {
    let parent = find_parent(root, target)?;
    let b = parent.borrow();
    let kids: &Vec<NodePtr> = match &*b {
        Node::Element(el) => &el.children,
        Node::Document { children } => children,
        _ => return None,
    };
    let idx = kids.iter().position(|c| Rc::ptr_eq(c, target))?;
    let mut i = idx as i32 + delta;
    while i >= 0 && (i as usize) < kids.len() {
        let c = &kids[i as usize];
        if !element_only || matches!(&*c.borrow(), Node::Element(_)) {
            return Some(c.clone());
        }
        i += delta;
    }
    None
}

fn contains_ptr(root: &NodePtr, needle: &NodePtr) -> bool {
    if Rc::ptr_eq(root, needle) {
        return true;
    }
    match &*root.borrow() {
        Node::Element(el) => el.children.iter().any(|c| contains_ptr(c, needle)),
        Node::Document { children } => children.iter().any(|c| contains_ptr(c, needle)),
        _ => false,
    }
}

fn clone_node(node: &NodePtr, deep: bool) -> NodePtr {
    match &*node.borrow() {
        Node::Text(t) => Node::text(t.clone()),
        Node::Document { children } => {
            let kids = if deep {
                children.iter().map(|c| clone_node(c, true)).collect()
            } else {
                Vec::new()
            };
            Node::document(kids)
        }
        Node::Element(el) => {
            let kids = if deep {
                el.children.iter().map(|c| clone_node(c, true)).collect()
            } else {
                Vec::new()
            };
            Node::element_with_attributes(el.tag_name.clone(), el.attributes.clone(), kids)
        }
    }
}

fn build_nodelist(
    nodes: Vec<NodePtr>,
    registry: &NodeRegistry,
    document: &NodePtr,
    context: &mut Context,
) -> JsResult<JsValue> {
    let values: Vec<JsValue> = nodes
        .into_iter()
        .map(|n| create_js_node(n, registry, document, context))
        .collect();
    Ok(JsArray::from_iter(values, context).into())
}

// ---------------------------------------------------------------------------
// Minimal selector engine: handles comma lists of compound selectors with
// descendant combinators. A compound selector is a sequence of:
//   tagName | #id | .class | [attr] | [attr=val] | [attr~=val]
// Pseudo-classes and attribute-selector edge cases are ignored gracefully.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SimpleSel {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, Option<String>)>,
    universal: bool,
}

fn parse_simple(s: &str) -> Option<SimpleSel> {
    let mut sel = SimpleSel {
        tag: None,
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
        universal: false,
    };
    let mut chars = s.chars().peekable();
    let mut acc = String::new();
    let mut mode = 't'; // 't' tag, 'i' id, 'c' class
    let flush = |mode: char, acc: &mut String, sel: &mut SimpleSel| {
        if acc.is_empty() {
            return;
        }
        match mode {
            't' => {
                if acc == "*" {
                    sel.universal = true;
                } else {
                    sel.tag = Some(acc.to_lowercase());
                }
            }
            'i' => sel.id = Some(std::mem::take(acc)),
            'c' => sel.classes.push(std::mem::take(acc)),
            _ => {}
        }
        acc.clear();
    };
    while let Some(&ch) = chars.peek() {
        match ch {
            '#' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                mode = 'i';
            }
            '.' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                mode = 'c';
            }
            '[' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                let mut attr = String::new();
                let mut val: Option<String> = None;
                let mut in_val = false;
                while let Some(c) = chars.next() {
                    if c == ']' {
                        break;
                    } else if c == '=' {
                        in_val = true;
                        val = Some(String::new());
                    } else if in_val {
                        if c == '"' || c == '\'' {
                            continue;
                        }
                        val.as_mut().unwrap().push(c);
                    } else if c == '~' || c == '|' || c == '^' || c == '$' || c == '*' {
                        // Treat prefix operators as "present with value" — lossy but safe.
                        continue;
                    } else {
                        attr.push(c);
                    }
                }
                sel.attrs.push((attr, val));
            }
            ':' => {
                // Pseudo-class: skip everything up to next space/comma/combinator.
                chars.next();
                // Skip nested `(...)`.
                let mut depth = 0;
                while let Some(&c) = chars.peek() {
                    if c == '(' {
                        depth += 1;
                        chars.next();
                    } else if c == ')' {
                        if depth > 0 {
                            depth -= 1;
                        }
                        chars.next();
                    } else if depth == 0
                        && (c == ' '
                            || c == ','
                            || c == '>'
                            || c == '+'
                            || c == '~'
                            || c == '.'
                            || c == '#'
                            || c == '[')
                    {
                        break;
                    } else {
                        chars.next();
                    }
                }
            }
            _ if ch.is_whitespace() => {
                break;
            }
            _ => {
                acc.push(ch);
                chars.next();
            }
        }
    }
    flush(mode, &mut acc, &mut sel);
    Some(sel)
}

fn simple_matches(node: &NodePtr, sel: &SimpleSel) -> bool {
    let b = node.borrow();
    let el = match &*b {
        Node::Element(e) => e,
        _ => return false,
    };
    if let Some(t) = &sel.tag {
        if !el.tag_name.eq_ignore_ascii_case(t) {
            return false;
        }
    }
    if let Some(id) = &sel.id {
        if el.attributes.get("id").map(|s| s.as_str()) != Some(id.as_str()) {
            return false;
        }
    }
    for cls in &sel.classes {
        let present = el
            .attributes
            .get("class")
            .map(|s| s.split_whitespace().any(|c| c == cls))
            .unwrap_or(false);
        if !present {
            return false;
        }
    }
    for (k, v) in &sel.attrs {
        match v {
            Some(val) => {
                if el.attributes.get(k).map(|s| s.as_str()) != Some(val.as_str()) {
                    return false;
                }
            }
            None => {
                if !el.attributes.contains_key(k) {
                    return false;
                }
            }
        }
    }
    true
}

fn selector_matches(node: &NodePtr, selector: &str) -> bool {
    // Only checks the final compound in each comma-group against `node` directly.
    for group in selector.split(',') {
        let parts: Vec<&str> = group.split_whitespace().collect();
        if let Some(last) = parts.last() {
            if let Some(sel) = parse_simple(last) {
                if simple_matches(node, &sel) {
                    return true;
                }
            }
        }
    }
    false
}

fn query_first(root: &NodePtr, selector: &str) -> Option<NodePtr> {
    let groups = parse_selector_groups(selector);
    query_first_rec(root, &groups, 0, true)
}

fn query_all(root: &NodePtr, selector: &str) -> Vec<NodePtr> {
    let groups = parse_selector_groups(selector);
    let mut out = Vec::new();
    query_all_rec(root, &groups, &mut out, true);
    out
}

fn parse_selector_groups(selector: &str) -> Vec<Vec<SimpleSel>> {
    selector
        .split(',')
        .filter_map(|g| {
            let parts: Vec<SimpleSel> = g.split_whitespace().filter_map(parse_simple).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts)
            }
        })
        .collect()
}

fn matches_any_group(node: &NodePtr, groups: &[Vec<SimpleSel>], root: &NodePtr) -> bool {
    for g in groups {
        if matches_group(node, g, root) {
            return true;
        }
    }
    false
}

fn matches_group(node: &NodePtr, group: &[SimpleSel], root: &NodePtr) -> bool {
    if group.is_empty() {
        return false;
    }
    let last = group.last().unwrap();
    if !simple_matches(node, last) {
        return false;
    }
    // Walk ancestors to verify descendant chain.
    let mut idx = group.len() as i32 - 2;
    let mut cursor = find_parent(root, node);
    while idx >= 0 {
        let sel = &group[idx as usize];
        let mut matched = None;
        while let Some(n) = cursor.clone() {
            if simple_matches(&n, sel) {
                matched = Some(n);
                break;
            }
            cursor = find_parent(root, &n);
        }
        match matched {
            Some(m) => {
                cursor = find_parent(root, &m);
                idx -= 1;
            }
            None => return false,
        }
    }
    true
}

fn query_first_rec(
    node: &NodePtr,
    groups: &[Vec<SimpleSel>],
    _depth: usize,
    skip_self: bool,
) -> Option<NodePtr> {
    if !skip_self {
        if matches_any_group(node, groups, node) {
            return Some(node.clone());
        }
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children } => children.clone(),
        _ => Vec::new(),
    };
    for c in kids {
        if matches_any_group(&c, groups, node) {
            return Some(c);
        }
        if let Some(found) = query_first_rec(&c, groups, _depth + 1, true) {
            return Some(found);
        }
    }
    None
}

fn query_all_rec(
    node: &NodePtr,
    groups: &[Vec<SimpleSel>],
    out: &mut Vec<NodePtr>,
    skip_self: bool,
) {
    if !skip_self && matches_any_group(node, groups, node) {
        out.push(node.clone());
    }
    let kids: Vec<NodePtr> = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children } => children.clone(),
        _ => Vec::new(),
    };
    for c in kids {
        if matches_any_group(&c, groups, node) {
            out.push(c.clone());
        }
        query_all_rec(&c, groups, out, true);
    }
}

// ---------------------------------------------------------------------------
// Serialization (innerHTML / outerHTML).
// ---------------------------------------------------------------------------

fn serialize_outer_html(node: &NodePtr) -> String {
    let mut out = String::new();
    serialize(node, &mut out);
    out
}

fn serialize_inner_html(node: &NodePtr) -> String {
    let mut out = String::new();
    match &*node.borrow() {
        Node::Element(el) => {
            for c in &el.children {
                serialize(c, &mut out);
            }
        }
        Node::Document { children } => {
            for c in children {
                serialize(c, &mut out);
            }
        }
        _ => {}
    }
    out
}

fn serialize(node: &NodePtr, out: &mut String) {
    match &*node.borrow() {
        Node::Text(t) => out.push_str(t),
        Node::Element(el) => {
            out.push('<');
            out.push_str(&el.tag_name);
            for (k, v) in &el.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(v);
                out.push('"');
            }
            out.push('>');
            for c in &el.children {
                serialize(c, out);
            }
            out.push_str("</");
            out.push_str(&el.tag_name);
            out.push('>');
        }
        Node::Document { children } => {
            for c in children {
                serialize(c, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small utilities.
// ---------------------------------------------------------------------------

fn log_native() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let msg = args
            .iter()
            .map(|v| v.display().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("JS Console: {}", msg);
        Ok(JsValue::undefined())
    })
}

fn noop_native() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()))
}

fn return_bool(v: bool) -> NativeFunction {
    if v {
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(true)))
    } else {
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(false)))
    }
}

fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for ch in s.chars() {
        if ch == '-' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// Base64 — minimal self-contained implementation for atob/btoa parity.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn base64_decode(input: &str) -> Option<String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&c| c != b'\n' && c != b'\r' && c != b' ')
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = bytes[i + 2];
        let d = bytes[i + 3];
        let n = ((a as u32) << 18) | ((b as u32) << 12);
        out.push(((n >> 16) & 0xff) as u8);
        if c != b'=' {
            let cv = val(c)?;
            let n = n | ((cv as u32) << 6);
            out.push(((n >> 8) & 0xff) as u8);
            if d != b'=' {
                let dv = val(d)?;
                let n = n | (dv as u32);
                out.push((n & 0xff) as u8);
            }
        }
        i += 4;
    }
    String::from_utf8(out).ok()
}

// Unused but kept for API symmetry; silences warnings.
#[allow(dead_code)]
fn _keep_types_alive(_: ElementNode) {}
