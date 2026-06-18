use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;
use std::time::{Duration, Instant};

use crate::css::Stylesheet;
use crate::dom::{Node, NodePtr};
use crate::layout::{LayoutTree, ViewportSize};
use crate::window::SnapshotRebuildReason;

use super::capture::WindowCapture;
use super::node_create::create_js_node;
use super::registry::NodeRegistry;
use super::selectors::query;
use super::tree::mutation;

// V8 allows exactly one platform per process, initialized before the first
// isolate and never torn down. V8 tolerates the platform living for the process.
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
    window: Rc<RefCell<WindowCapture>>,
    registry: Rc<NodeRegistry>,
    document: NodePtr,
}

struct DocumentData {
    document: NodePtr,
    registry: Rc<NodeRegistry>,
}

impl V8Runtime {
    pub(crate) fn new(document: NodePtr) -> Self {
        Self::with_render_document(document, None)
    }

    pub(crate) fn with_render_document(
        document: NodePtr,
        render_document: Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>,
    ) -> Self {
        ensure_platform();
        // Establish the parent back-pointer invariant for the initial tree so
        // connectivity/ancestor queries are O(depth); the parent pointer is then
        // treated as authoritative (see `selectors::query::find_parent`).
        crate::dom::reparent_subtree(&document);
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        let window = Rc::new(RefCell::new(WindowCapture::new()));
        let registry = Rc::new(NodeRegistry::new());
        // Make the document reachable from the registry (used by e.g.
        // MutationObserver record construction, which only has the registry).
        *registry.document.borrow_mut() = Some(document.clone());
        registry.set_render_document(render_document);

        let context = {
            v8::scope!(let scope, &mut isolate);

            // Create a global template and bind some basic globals.
            let global_template = v8::ObjectTemplate::new(scope);

            let context = v8::Context::new(
                scope,
                v8::ContextOptions {
                    global_template: Some(global_template),
                    ..Default::default()
                },
            );
            let scope = &mut v8::ContextScope::new(scope, context);

            let global = context.global(scope);

            // window, self, globalThis aliases.
            global.set(scope, v8_str(scope, "window").into(), global.into());
            global.set(scope, v8_str(scope, "self").into(), global.into());

            // Simple console.log implementation.
            let console_template = v8::ObjectTemplate::new(scope);
            let log_fn = v8::FunctionTemplate::new(scope, console_log);
            console_template.set(v8_str(scope, "log").into(), log_fn.into());
            console_template.set(v8_str(scope, "info").into(), log_fn.into());
            console_template.set(v8_str(scope, "warn").into(), log_fn.into());
            console_template.set(v8_str(scope, "error").into(), log_fn.into());

            let console_obj = console_template.new_instance(scope).unwrap();
            global.set(scope, v8_str(scope, "console").into(), console_obj.into());

            // Document.
            let doc_data = Box::into_raw(Box::new(DocumentData {
                document: document.clone(),
                registry: registry.clone(),
            })) as *mut _;
            let doc_external = v8::External::new(scope, doc_data);

            let document_template = v8::ObjectTemplate::new(scope);

            let get_element_by_id_fn = v8::FunctionTemplate::builder(get_element_by_id)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "getElementById").into(),
                get_element_by_id_fn.into(),
            );

            let get_elements_by_tag_name_fn =
                v8::FunctionTemplate::builder(get_elements_by_tag_name)
                    .data(doc_external.into())
                    .build(scope);
            document_template.set(
                v8_str(scope, "getElementsByTagName").into(),
                get_elements_by_tag_name_fn.into(),
            );

            let query_selector_fn = v8::FunctionTemplate::builder(query_selector)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "querySelector").into(),
                query_selector_fn.into(),
            );

            let query_selector_all_fn = v8::FunctionTemplate::builder(query_selector_all)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "querySelectorAll").into(),
                query_selector_all_fn.into(),
            );

            let create_element_fn = v8::FunctionTemplate::builder(create_element)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "createElement").into(),
                create_element_fn.into(),
            );

            let create_text_node_fn = v8::FunctionTemplate::builder(create_text_node)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "createTextNode").into(),
                create_text_node_fn.into(),
            );

            let element_from_point_fn = v8::FunctionTemplate::builder(element_from_point)
                .data(doc_external.into())
                .build(scope);
            document_template.set(
                v8_str(scope, "elementFromPoint").into(),
                element_from_point_fn.into(),
            );

            let document_obj = document_template.new_instance(scope).unwrap();
            global.set(scope, v8_str(scope, "document").into(), document_obj.into());

            // Set document structure fields
            let bodies = registry.collect_by_tag_dom("body", &document).unwrap_or_else(|| {
                let mut bodies = Vec::new();
                query::collect_by_tag(&document, "body", &mut bodies);
                bodies
            });
            if let Some(body_node) = bodies.first() {
                let js_body = create_js_node(scope, body_node.clone(), &registry, &document);
                document_obj.set(scope, v8_str(scope, "body").into(), js_body.into());
            }

            let heads = registry.collect_by_tag_dom("head", &document).unwrap_or_else(|| {
                let mut heads = Vec::new();
                query::collect_by_tag(&document, "head", &mut heads);
                heads
            });
            if let Some(head_node) = heads.first() {
                let js_head = create_js_node(scope, head_node.clone(), &registry, &document);
                document_obj.set(scope, v8_str(scope, "head").into(), js_head.into());
            }

            let htmls = registry.collect_by_tag_dom("html", &document).unwrap_or_else(|| {
                let mut htmls = Vec::new();
                query::collect_by_tag(&document, "html", &mut htmls);
                htmls
            });
            if let Some(html_node) = htmls.first() {
                let js_html = create_js_node(scope, html_node.clone(), &registry, &document);
                document_obj.set(
                    scope,
                    v8_str(scope, "documentElement").into(),
                    js_html.into(),
                );
            } else {
                let js_doc = create_js_node(scope, document.clone(), &registry, &document);
                document_obj.set(
                    scope,
                    v8_str(scope, "documentElement").into(),
                    js_doc.into(),
                );
            }

            document_obj.set(scope, v8_str(scope, "defaultView").into(), global.into());

            // document.title
            let titles = registry.collect_by_tag_dom("title", &document).unwrap_or_else(|| {
                let mut titles = Vec::new();
                query::collect_by_tag(&document, "title", &mut titles);
                titles
            });
            if let Some(title_node) = titles.first() {
                let text = mutation::collect_text(title_node);
                document_obj.set(
                    scope,
                    v8_str(scope, "title").into(),
                    v8_str(scope, &text).into(),
                );
            }

            // Navigator stub.
            let navigator_template = v8::ObjectTemplate::new(scope);
            let navigator_obj = navigator_template.new_instance(scope).unwrap();
            navigator_obj.set(
                scope,
                v8_str(scope, "userAgent").into(),
                v8_str(scope, crate::fetch::http::CHROME_UA).into(),
            );
            global.set(
                scope,
                v8_str(scope, "navigator").into(),
                navigator_obj.into(),
            );

            // Location stub.
            let location_template = v8::ObjectTemplate::new(scope);
            let location_obj = location_template.new_instance(scope).unwrap();
            location_obj.set(
                scope,
                v8_str(scope, "href").into(),
                v8_str(scope, "about:blank").into(),
            );
            global.set(scope, v8_str(scope, "location").into(), location_obj.into());

            // Timer and rAF support.
            let window_data =
                v8::External::new(scope, Box::into_raw(Box::new(window.clone())) as *mut _);

            let set_timeout_fn = v8::FunctionTemplate::builder(set_timeout)
                .data(window_data.into())
                .build(scope);
            global.set(
                scope,
                v8_str(scope, "setTimeout").into(),
                set_timeout_fn.get_function(scope).unwrap().into(),
            );

            let set_interval_fn = v8::FunctionTemplate::builder(set_interval)
                .data(window_data.into())
                .build(scope);
            global.set(
                scope,
                v8_str(scope, "setInterval").into(),
                set_interval_fn.get_function(scope).unwrap().into(),
            );

            let clear_timer_fn = v8::FunctionTemplate::builder(clear_timer)
                .data(window_data.into())
                .build(scope);
            global.set(
                scope,
                v8_str(scope, "clearTimeout").into(),
                clear_timer_fn.get_function(scope).unwrap().into(),
            );
            global.set(
                scope,
                v8_str(scope, "clearInterval").into(),
                clear_timer_fn.get_function(scope).unwrap().into(),
            );

            let raf_fn = v8::FunctionTemplate::builder(request_animation_frame)
                .data(window_data.into())
                .build(scope);
            global.set(
                scope,
                v8_str(scope, "requestAnimationFrame").into(),
                raf_fn.get_function(scope).unwrap().into(),
            );

            let cancel_raf_fn = v8::FunctionTemplate::builder(cancel_animation_frame)
                .data(window_data.into())
                .build(scope);
            global.set(
                scope,
                v8_str(scope, "cancelAnimationFrame").into(),
                cancel_raf_fn.get_function(scope).unwrap().into(),
            );

            // Event listeners.
            let registry_data =
                v8::External::new(scope, Box::into_raw(Box::new(registry.clone())) as *mut _);

            let add_event_listener_fn = v8::FunctionTemplate::builder(add_event_listener)
                .data(registry_data.into())
                .build(scope);
            let add_event_listener_js = add_event_listener_fn.get_function(scope).unwrap();

            global.set(
                scope,
                v8_str(scope, "addEventListener").into(),
                add_event_listener_js.into(),
            );
            document_obj.set(
                scope,
                v8_str(scope, "addEventListener").into(),
                add_event_listener_js.into(),
            );

            let dispatch_event_fn = v8::FunctionTemplate::builder(dispatch_event_global)
                .data(registry_data.into())
                .build(scope);
            let dispatch_event_js = dispatch_event_fn.get_function(scope).unwrap();
            global.set(
                scope,
                v8_str(scope, "dispatchEvent").into(),
                dispatch_event_js.into(),
            );
            document_obj.set(
                scope,
                v8_str(scope, "dispatchEvent").into(),
                dispatch_event_js.into(),
            );

            // Real MutationObserver (replaces the never-firing JS stub).
            super::mutation_observer::install(scope, global, registry_data);

            // --- Browser APIs ---
            global.set(
                scope,
                v8_str(scope, "__aurora_fetch_sync__").into(),
                v8::FunctionTemplate::new(scope, aurora_fetch_sync)
                    .get_function(scope)
                    .unwrap()
                    .into(),
            );

            // Stubs and no-ops
            let noop = v8::FunctionTemplate::new(scope, noop_callback)
                .get_function(scope)
                .unwrap();
            global.set(scope, v8_str(scope, "alert").into(), noop.into());
            global.set(scope, v8_str(scope, "scrollTo").into(), noop.into());
            global.set(scope, v8_str(scope, "scrollBy").into(), noop.into());

            // atob / btoa
            global.set(
                scope,
                v8_str(scope, "atob").into(),
                v8::FunctionTemplate::new(scope, atob)
                    .get_function(scope)
                    .unwrap()
                    .into(),
            );
            global.set(
                scope,
                v8_str(scope, "btoa").into(),
                v8::FunctionTemplate::new(scope, btoa)
                    .get_function(scope)
                    .unwrap()
                    .into(),
            );

            // structuredClone
            global.set(
                scope,
                v8_str(scope, "structuredClone").into(),
                v8::FunctionTemplate::new(scope, structured_clone)
                    .get_function(scope)
                    .unwrap()
                    .into(),
            );

            // Viewport stubs
            global.set(
                scope,
                v8_str(scope, "innerWidth").into(),
                v8::Number::new(scope, 1200.0).into(),
            );
            global.set(
                scope,
                v8_str(scope, "innerHeight").into(),
                v8::Number::new(scope, 800.0).into(),
            );
            global.set(
                scope,
                v8_str(scope, "devicePixelRatio").into(),
                v8::Number::new(scope, 1.0).into(),
            );

            let screen_template = v8::ObjectTemplate::new(scope);
            screen_template.set(
                v8_str(scope, "width").into(),
                v8::Integer::new(scope, 1200).into(),
            );
            screen_template.set(
                v8_str(scope, "height").into(),
                v8::Integer::new(scope, 800).into(),
            );
            screen_template.set(
                v8_str(scope, "availWidth").into(),
                v8::Integer::new(scope, 1200).into(),
            );
            screen_template.set(
                v8_str(scope, "availHeight").into(),
                v8::Integer::new(scope, 800).into(),
            );
            let screen_obj = screen_template.new_instance(scope).unwrap();
            global.set(scope, v8_str(scope, "screen").into(), screen_obj.into());

            // Storage
            let local_storage = build_storage_object(scope, window.borrow().storage.clone());
            global.set(
                scope,
                v8_str(scope, "localStorage").into(),
                local_storage.into(),
            );
            let session_storage = build_storage_object(scope, window.borrow().session.clone());
            global.set(
                scope,
                v8_str(scope, "sessionStorage").into(),
                session_storage.into(),
            );

            // Networking polyfills
            {
                v8::tc_scope!(let scope, scope);
                let polyfill = r#"
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

                    globalThis.fetch = function(url, options) {
                        var method = (options && options.method) ? options.method.toUpperCase() : 'GET';
                        try {
                            var result = globalThis.__aurora_fetch_sync__(String(url), method);
                            if (result.ok) {
                                var responseText = result.body;
                                var status = result.status;
                                return Promise.resolve({
                                    ok: status >= 200 && status < 300,
                                    status: status,
                                    statusText: String(status),
                                    url: String(url),
                                    headers: new Headers(),
                                    text: function() { return Promise.resolve(responseText); },
                                    json: function() {
                                        try { return Promise.resolve(JSON.parse(responseText)); }
                                        catch(e) { return Promise.reject(e); }
                                    },
                                    arrayBuffer: function() { return Promise.resolve(new ArrayBuffer(0)); },
                                    blob: function() { return Promise.resolve({ text: function() { return Promise.resolve(responseText); } }); },
                                    clone: function() { return this; }
                                });
                            } else {
                                return Promise.reject(new Error("HTTP " + result.status));
                            }
                        } catch(e) {
                            return Promise.reject(e);
                        }
                    };

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
                    globalThis.AbortController = function() {
                        this.signal = { aborted: false, addEventListener: function(){}, removeEventListener: function(){} };
                        this.abort = function(){ this.signal.aborted = true; };
                    };
                "#;
                if let Err(e) = compile_and_run(scope, polyfill) {
                    log::warn!(target: "aurora::js", "[JS] bootstrap networking failed: {e}");
                }

                // V8-specific base/post shims plus shared polyfills. Order matters: v8_base defines
                // HTMLElement/queueMicrotask that event_constructors and
                // custom_elements build on; v8_post wires document-dependent
                // pieces and primes the custom-element registry.
                let bootstrap_blocks: [(&str, &str); 7] = [
                    ("v8-base", include_str!("../js_polyfills/v8_base.js")),
                    (
                        "event-constructors",
                        include_str!("../js_polyfills/event_constructors.js"),
                    ),
                    (
                        "trusted-types",
                        include_str!("../js_polyfills/trusted_types.js"),
                    ),
                    (
                        "custom-elements",
                        include_str!("../js_polyfills/custom_elements.js"),
                    ),
                    ("css-stub", include_str!("../js_polyfills/css_stub.js")),
                    ("v8-post", include_str!("../js_polyfills/v8_post.js")),
                    (
                        "polymer-shim",
                        include_str!("../js_polyfills/polymer_shim.js"),
                    ),
                ];
                for (label, source) in bootstrap_blocks {
                    if let Err(e) = compile_and_run(scope, source) {
                        log::warn!(target: "aurora::js", "[JS] bootstrap {label} failed: {e}");
                    }
                }

                // Wrappers built during context setup (document, body, head,
                // documentElement) predate the JS DOM prototype skeletons, so
                // re-link them to the EventTarget chain now that it exists.
                let _ = compile_and_run(
                    scope,
                    r#"(function(){
                        var nodes = [document, document.body, document.head, document.documentElement];
                        for (var i = 0; i < nodes.length; i++) {
                            var n = nodes[i];
                            if (n && typeof n.addEventListener !== 'function' && typeof HTMLElement !== 'undefined') {
                                try { Object.setPrototypeOf(n, HTMLElement.prototype); } catch (e) {}
                            }
                        }
                    })();"#,
                );

                if matches!(
                    std::env::var("AURORA_DEBUG_YOUTUBE").as_deref(),
                    Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
                ) {
                    let _ = compile_and_run(scope, "globalThis.__aurora_debug_youtube__ = true;");
                }
                if matches!(
                    std::env::var("AURORA_DEBUG_SHADYCSS").as_deref(),
                    Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
                ) {
                    let _ = compile_and_run(scope, "globalThis.__aurora_debug_shadycss__ = true;");
                }
            }

            let context = v8::Global::new(scope, context);

            context
        };
        Self {
            isolate,
            context,
            window,
            registry,
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

    fn ready_timers(&mut self, now: Instant) -> Vec<super::capture::TimerEntry> {
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        let mut window = self.window.borrow_mut();

        for entry in window.timers.drain(..) {
            if entry.deadline <= now && ready.len() < 100 {
                ready.push(entry);
            } else {
                pending.push(entry);
            }
        }
        window.timers = pending;
        ready
    }

    fn run_js_quiet(&mut self, source: &str) {
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        v8::tc_scope!(let scope, scope);
        let _ = compile_and_run(scope, source);
    }

    fn fire_lifecycle_event(&mut self, event_type: &str) {
        // Lifecycle listeners live on the JS `document`/`window` EventTargets, so
        // fire through the JS event model rather than the legacy id-0 registry.
        // These events are delivered to both globals for compatibility, but they
        // must not bubble from document to window and then be dispatched again.
        let source = format!(
            "(function(){{ try {{ \
             document.dispatchEvent(new Event({event_type:?}, {{ bubbles: false }})); \
             window.dispatchEvent(new Event({event_type:?}, {{ bubbles: false }})); \
             }} catch (err) {{}} }})();"
        );
        self.run_js_quiet(&source);
    }
}

fn v8_str<'s>(scope: &v8::PinScope<'s, '_, ()>, s: &str) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, s).expect("failed to create V8 string")
}

fn console_log(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let mut output = String::new();
    for i in 0..args.length() {
        if i > 0 {
            output.push(' ');
        }
        output.push_str(&args.get(i).to_rust_string_lossy(scope));
    }
    println!("{}", output);
}

fn set_timeout(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    add_timer(scope, args, &mut retval, false);
}

fn set_interval(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    add_timer(scope, args, &mut retval, true);
}

fn add_timer(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    retval: &mut v8::ReturnValue,
    is_interval: bool,
) {
    let callback = args.get(0);
    if !callback.is_function() {
        return;
    }
    let callback = v8::Local::<v8::Function>::try_from(callback).unwrap();
    let callback_global = v8::Global::new(scope, callback);

    let delay_ms = args.get(1).int32_value(scope).unwrap_or(0).max(0) as u64;
    let duration = Duration::from_millis(delay_ms);

    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let window_ptr = external.value() as *const Rc<RefCell<WindowCapture>>;
    let window_rc = unsafe { &*window_ptr };

    let mut window = window_rc.borrow_mut();
    let id = window.next_timer_id;
    window.next_timer_id += 1;

    window.timers.push(super::capture::TimerEntry {
        id,
        callback: callback_global,
        deadline: Instant::now() + duration,
        interval: if is_interval { Some(duration) } else { None },
    });

    retval.set(v8::Integer::new(scope, id as i32).into());
}

fn clear_timer(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let id = args.get(0).int32_value(_scope).unwrap_or(0) as u32;
    if id == 0 {
        return;
    }

    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let window_ptr = external.value() as *const Rc<RefCell<WindowCapture>>;
    let window_rc = unsafe { &*window_ptr };

    let mut window = window_rc.borrow_mut();
    window.timers.retain(|t| t.id != id);
}

fn request_animation_frame(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let callback = args.get(0);
    if !callback.is_function() {
        return;
    }
    let callback = v8::Local::<v8::Function>::try_from(callback).unwrap();
    let callback_global = v8::Global::new(scope, callback);

    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let window_ptr = external.value() as *const Rc<RefCell<WindowCapture>>;
    let window_rc = unsafe { &*window_ptr };

    let mut window = window_rc.borrow_mut();
    let id = window.next_raf_id;
    window.next_raf_id += 1;

    window
        .animation_frames
        .push(super::capture::AnimationFrameEntry {
            id,
            callback: callback_global,
        });

    retval.set(v8::Integer::new(scope, id as i32).into());
}

fn cancel_animation_frame(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let id = args.get(0).int32_value(_scope).unwrap_or(0) as u32;
    if id == 0 {
        return;
    }

    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let window_ptr = external.value() as *const Rc<RefCell<WindowCapture>>;
    let window_rc = unsafe { &*window_ptr };

    let mut window = window_rc.borrow_mut();
    window.animation_frames.retain(|f| f.id != id);
}

fn add_event_listener(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback = args.get(1);
    if !callback.is_function() {
        return;
    }
    let callback = v8::Local::<v8::Function>::try_from(callback).unwrap();
    let callback_global = v8::Global::new(scope, callback);

    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let registry_ptr = external.value() as *const Rc<NodeRegistry>;
    let registry = unsafe { &*registry_ptr };

    // For now, always register on document (id 0) if called on window/document stub.
    // In a real implementation, we'd check 'this' to get the node id.
    registry.add_event_listener(0, event_type, callback_global);
}

/// `window.dispatchEvent` / `document.dispatchEvent`: fire the document/window
/// listeners (registered under id 0). Without this these were no-op polyfill
/// stubs, so YouTube's document-level listeners (`yt-navigate-finish`, etc.)
/// never ran and the app never navigated past its shell.
fn dispatch_event_global(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let event = args.get(0);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let registry_ptr = external.value() as *const Rc<NodeRegistry>;
    let registry = unsafe { &*registry_ptr };

    let mut event_type = String::new();
    if let Some(event_obj) = event.to_object(scope) {
        if let Some(t) = event_obj.get(scope, v8_str(scope, "type").into()) {
            event_type = t.to_rust_string_lossy(scope);
        }
        let this = args.this();
        event_obj.set(scope, v8_str(scope, "target").into(), this.into());
        event_obj.set(scope, v8_str(scope, "currentTarget").into(), this.into());
    }

    let this = args.this();
    for listener in registry.get_listeners(0, &event_type) {
        let callback = v8::Local::new(scope, listener);
        let _ = callback.call(scope, this.into(), &[event]);
    }
    retval.set(v8::Boolean::new(scope, true).into());
}

fn get_element_by_id(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let id = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let node = if doc_data.registry.has_render_document() {
        doc_data.registry.get_element_by_id_dom(&id)
    } else {
        query::find_by_id(&doc_data.document, &id)
    };
    if let Some(node) = node {
        let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_elements_by_tag_name(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let out = if doc_data.registry.has_render_document() {
        doc_data
            .registry
            .collect_by_tag_dom(&tag, &doc_data.document)
            .unwrap_or_default()
    } else {
        let mut out = Vec::new();
        query::collect_by_tag(&doc_data.document, &tag, &mut out);
        out
    };

    let array = v8::Array::new(scope, out.len() as i32);
    for (i, node) in out.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
        array.set_index(scope, i as u32, js_node.into());
    }
    retval.set(array.into());
}

fn query_selector(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let node = if doc_data.registry.has_render_document() {
        doc_data
            .registry
            .query_selector_all_dom(&selector, &doc_data.document)
            .and_then(|nodes| nodes.into_iter().next())
    } else {
        query::query_first(&doc_data.document, &selector, &doc_data.document)
    };
    if let Some(node) = node {
        let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn query_selector_all(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let found = if doc_data.registry.has_render_document() {
        doc_data
            .registry
            .query_selector_all_dom(&selector, &doc_data.document)
            .unwrap_or_default()
    } else {
        query::query_all(&doc_data.document, &selector, &doc_data.document)
    };
    let array = v8::Array::new(scope, found.len() as i32);
    for (i, node) in found.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
        array.set_index(scope, i as u32, js_node.into());
    }
    retval.set(array.into());
}

fn create_element(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_lowercase();
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let node = Node::element(tag, vec![]);
    let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
    retval.set(js_node.into());
}

fn create_text_node(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let text = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let node = Node::text(text);
    let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
    retval.set(js_node.into());
}

/// `document.elementFromPoint(x, y)` — hit-tests the Blitz layout (Phase 8.2
/// follow-up). Returns the wrapper for the deepest element at the point, or
/// `null` when there is no render document or nothing is hit.
fn element_from_point(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let x = args.get(0).number_value(scope).unwrap_or(f64::NAN) as f32;
    let y = args.get(1).number_value(scope).unwrap_or(f64::NAN) as f32;
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let doc_data_ptr = external.value() as *const DocumentData;
    let doc_data = unsafe { &*doc_data_ptr };

    let hit = if x.is_finite() && y.is_finite() {
        doc_data.registry.hit_test(x, y)
    } else {
        None
    };
    match hit {
        Some(node) => {
            let js_node = create_js_node(scope, node, &doc_data.registry, &doc_data.document);
            retval.set(js_node.into());
        }
        None => retval.set(v8::null(scope).into()),
    }
}

fn atob(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let s = args.get(0).to_rust_string_lossy(scope);
    if let Some(decoded) = base64_decode(&s) {
        retval.set(v8_str(scope, &decoded).into());
    }
}

fn btoa(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let s = args.get(0).to_rust_string_lossy(scope);
    let encoded = base64_encode(s.as_bytes());
    retval.set(v8_str(scope, &encoded).into());
}

fn structured_clone(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let val = args.get(0);
    // Minimal JSON round-trip for parity.
    if let Some(json_str) = v8::json::stringify(scope, val) {
        let json_str = json_str.to_rust_string_lossy(scope);
        let code = v8::String::new(scope, &json_str).unwrap();
        if let Some(parsed) = v8::json::parse(scope, code) {
            retval.set(parsed);
        }
    }
}

fn noop_callback(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
}

fn aurora_fetch_sync(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let url = args.get(0).to_rust_string_lossy(scope);
    let obj = v8::Object::new(scope);

    match crate::fetch::http::fetch_string(&url) {
        Ok(body) => {
            obj.set(
                scope,
                v8_str(scope, "ok").into(),
                v8::Boolean::new(scope, true).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "status").into(),
                v8::Integer::new(scope, 200).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "body").into(),
                v8_str(scope, &body).into(),
            );
        }
        Err(e) => {
            obj.set(
                scope,
                v8_str(scope, "ok").into(),
                v8::Boolean::new(scope, false).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "status").into(),
                v8::Integer::new(scope, 0).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "body").into(),
                v8_str(scope, "").into(),
            );
            obj.set(
                scope,
                v8_str(scope, "error").into(),
                v8_str(scope, &e.to_string()).into(),
            );
        }
    }
    retval.set(obj.into());
}

fn build_storage_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    backing: Rc<RefCell<std::collections::BTreeMap<String, String>>>,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    let data = v8::External::new(scope, Box::into_raw(Box::new(backing)) as *mut _);

    let get_item = v8::FunctionTemplate::builder(storage_get_item)
        .data(data.into())
        .build(scope);
    template.set(v8_str(scope, "getItem").into(), get_item.into());

    let set_item = v8::FunctionTemplate::builder(storage_set_item)
        .data(data.into())
        .build(scope);
    template.set(v8_str(scope, "setItem").into(), set_item.into());

    let remove_item = v8::FunctionTemplate::builder(storage_remove_item)
        .data(data.into())
        .build(scope);
    template.set(v8_str(scope, "removeItem").into(), remove_item.into());

    let clear = v8::FunctionTemplate::builder(storage_clear)
        .data(data.into())
        .build(scope);
    template.set(v8_str(scope, "clear").into(), clear.into());

    let key = v8::FunctionTemplate::builder(storage_key)
        .data(data.into())
        .build(scope);
    template.set(v8_str(scope, "key").into(), key.into());

    template.new_instance(scope).unwrap()
}

fn storage_get_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let map_ptr =
        external.value() as *const Rc<RefCell<std::collections::BTreeMap<String, String>>>;
    let map = unsafe { &*map_ptr };

    if let Some(val) = map.borrow().get(&key) {
        retval.set(v8_str(scope, val).into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn storage_set_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let val = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let map_ptr =
        external.value() as *const Rc<RefCell<std::collections::BTreeMap<String, String>>>;
    let map = unsafe { &*map_ptr };

    map.borrow_mut().insert(key, val);
}

fn storage_remove_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let key = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let map_ptr =
        external.value() as *const Rc<RefCell<std::collections::BTreeMap<String, String>>>;
    let map = unsafe { &*map_ptr };

    map.borrow_mut().remove(&key);
}

fn storage_clear(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let map_ptr =
        external.value() as *const Rc<RefCell<std::collections::BTreeMap<String, String>>>;
    let map = unsafe { &*map_ptr };

    map.borrow_mut().clear();
}

fn storage_key(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let map_ptr =
        external.value() as *const Rc<RefCell<std::collections::BTreeMap<String, String>>>;
    let map = unsafe { &*map_ptr };

    if let Some(k) = map.borrow().keys().nth(idx) {
        retval.set(v8_str(scope, k).into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

// Base64 utilities shared with the JS environment.
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
    let message = scope
        .exception()
        .map(|exc| exc.to_rust_string_lossy(scope))
        .unwrap_or_else(|| fallback.to_string());
    match scope.stack_trace() {
        Some(stack) => format!("{message}\n{}", stack.to_rust_string_lossy(scope)),
        None => message,
    }
}

impl crate::js_engine::JsRuntime for V8Runtime {
    fn execute(&mut self, script: &str) -> Result<(), String> {
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        v8::tc_scope!(let scope, scope);
        compile_and_run(scope, script).map(|_| ())
    }

    fn set_current_script(&mut self, _script: Option<&NodePtr>) {}

    fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<LayoutTree>>,
        stylesheet: Rc<RefCell<Stylesheet>>,
        viewport: Rc<RefCell<ViewportSize>>,
    ) {
        self.registry
            .set_shared_state(layout_tree, stylesheet, viewport, self.document.clone());
    }

    fn set_render_document(
        &mut self,
        render_document: Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>,
    ) {
        self.registry.set_render_document(render_document);
    }

    fn clear_dirty_bits(&mut self) {
        self.registry.clear_dirty_bits();
    }
    fn has_dirty_bits(&self) -> bool {
        self.registry.has_dirty_bits()
    }
    fn take_needs_reflow(&mut self) -> bool {
        self.registry.take_needs_reflow()
    }

    fn take_snapshot_rebuild_reason(&mut self) -> Option<SnapshotRebuildReason> {
        self.registry.take_snapshot_rebuild_reason()
    }

    fn tick(&mut self, now: Instant) -> bool {
        let ready = self.ready_timers(now);
        if ready.is_empty() {
            return false;
        }

        {
            v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
            let context = v8::Local::new(scope, &self.context);
            let global = context.global(scope);

            for entry in ready {
                let callback = v8::Local::new(scope, entry.callback);
                let _ = callback.call(scope, global.into(), &[]);

                if let Some(interval) = entry.interval {
                    self.window
                        .borrow_mut()
                        .timers
                        .push(super::capture::TimerEntry {
                            id: entry.id,
                            callback: v8::Global::new(scope, callback),
                            deadline: now + interval,
                            interval: Some(interval),
                        });
                }
            }
        }

        if self.has_animation_frame_callbacks() {
            let _ = self.drain_animation_frame_callbacks(now);
        }

        true
    }

    fn deliver_mutation_records(&mut self) -> bool {
        if !super::mutation_observer::has_pending(&self.registry) {
            return false;
        }
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        super::mutation_observer::deliver(scope, &self.registry)
    }

    fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool {
        let callbacks = self
            .window
            .borrow_mut()
            .animation_frames
            .drain(..)
            .collect::<Vec<_>>();
        if callbacks.is_empty() {
            return false;
        }

        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        let context = v8::Local::new(scope, &self.context);
        let global = context.global(scope);
        let timestamp = now
            .duration_since(self.window.borrow().time_origin)
            .as_secs_f64()
            * 1000.0;
        let ts_val = v8::Number::new(scope, timestamp);

        for entry in callbacks {
            let callback = v8::Local::new(scope, entry.callback);
            let _ = callback.call(scope, global.into(), &[ts_val.into()]);
        }

        true
    }

    fn dispatch_event(&mut self, _node: &NodePtr, _event_type: &str) -> bool {
        // TODO: Full event dispatch
        false
    }

    fn fire_dom_content_loaded(&mut self) {
        self.run_js_quiet("document.readyState = 'interactive';");
        self.fire_lifecycle_event("DOMContentLoaded");
    }

    fn fire_load(&mut self) {
        self.run_js_quiet("document.readyState = 'complete';");
        self.fire_lifecycle_event("load");
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.window.borrow().timers.iter().map(|t| t.deadline).min()
    }
    fn has_animation_frame_callbacks(&self) -> bool {
        !self.window.borrow().animation_frames.is_empty()
    }
    fn has_ready_work(&self, now: Instant) -> bool {
        self.has_animation_frame_callbacks()
            || self.next_deadline().map(|d| d <= now).unwrap_or(false)
    }
}
