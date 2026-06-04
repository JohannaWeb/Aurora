use std::sync::LazyLock;
use std::time::Instant;

use super::*;

static PERF_ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

pub(in crate::js_boa) fn install_platform_objects(context: &mut Context, global_obj: &JsObject) {
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

    // Performance — now() returns real elapsed ms since first call.
    let perf = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
                let _ = *PERF_ORIGIN; // ensure initialized
                Ok(JsValue::from(PERF_ORIGIN.elapsed().as_secs_f64() * 1000.0))
            }),
            js_string!("now"),
            0,
        )
        .function(noop_native(), js_string!("mark"), 1)
        .function(noop_native(), js_string!("measure"), 3)
        .function(noop_native(), js_string!("clearMarks"), 0)
        .function(noop_native(), js_string!("clearMeasures"), 0)
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntries"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntriesByType"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(|_this, _args, ctx| Ok(JsArray::new(ctx)?.into())),
            js_string!("getEntriesByName"),
            2,
        )
        .build();
    let _ = perf.set(js_string!("timeOrigin"), 0.0, false, context);
    let _ = context.register_global_property(js_string!("performance"), perf, Attribute::all());

    // Crypto is intentionally absent-for-now rather than fake-random.
    let crypto = ObjectInitializer::new(context)
        .function(unsupported_crypto(), js_string!("randomUUID"), 0)
        .function(unsupported_crypto(), js_string!("getRandomValues"), 1)
        .build();
    let _ = context.register_global_property(js_string!("crypto"), crypto, Attribute::all());

    // CSS.supports() — always returns false; prevents crashes on feature checks.
    let css_supports = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(false)));
    let css_obj = ObjectInitializer::new(context)
        .function(css_supports, js_string!("supports"), 2)
        .function(noop_native(), js_string!("escape"), 1)
        .build();
    let _ = context.register_global_property(js_string!("CSS"), css_obj, Attribute::all());

    // trustedTypes — YouTube uses Trusted Types; stub a minimal policy factory so
    // `trustedTypes.createPolicy(...)` doesn't throw.
    let trusted_types_js = r#"
        (function() {
            function makeTrusted(val) { return { toString: function(){ return val; } }; }
            globalThis.trustedTypes = {
                createPolicy: function(name, rules) {
                    return {
                        name: name,
                        createHTML: function(s) { return makeTrusted(rules && rules.createHTML ? rules.createHTML(s) : s); },
                        createScript: function(s) { return makeTrusted(rules && rules.createScript ? rules.createScript(s) : s); },
                        createScriptURL: function(s) { return makeTrusted(rules && rules.createScriptURL ? rules.createScriptURL(s) : s); }
                    };
                },
                getAttributeType: function() { return null; },
                getPropertyType: function() { return null; },
                isHTML: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScript: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScriptURL: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                emptyHTML: makeTrusted(''),
                emptyScript: makeTrusted(''),
                defaultPolicy: null
            };
        })();
    "#;
    let _ = context.eval(Source::from_bytes(trusted_types_js.as_bytes()));

    // Image, Event, CustomEvent — defined as real JS functions so `new` works.
    // native_to_jsfn sets .constructor(false) which breaks `new`.
    let constructors_js = r#"
        (function() {
            globalThis.Image = function Image(width, height) {
                this.src = ''; this.width = width || 0; this.height = height || 0;
                this.naturalWidth = 0; this.naturalHeight = 0; this.complete = false;
                this.onload = null; this.onerror = null; this.crossOrigin = null;
                this.decoding = 'auto'; this.loading = 'eager';
                this.addEventListener = function(){}; this.removeEventListener = function(){};
                this.decode = function(){ return Promise.resolve(); };
            };
            globalThis.Image.prototype = Object.create(
                (typeof HTMLImageElement !== 'undefined') ? HTMLImageElement.prototype : Object.prototype
            );

            globalThis.Event = function Event(type, init) {
                var obj = (this instanceof Event) ? this : {};
                init = init || {};
                obj.type = type || '';
                obj.bubbles = !!(init.bubbles);
                obj.cancelable = !!(init.cancelable);
                obj.defaultPrevented = false;
                obj.isTrusted = false;
                obj.timeStamp = 0;
                obj.target = null; obj.currentTarget = null;
                obj.stopPropagation = function(){};
                obj.stopImmediatePropagation = function(){};
                obj.preventDefault = function(){ obj.defaultPrevented = true; };
                obj.composedPath = function(){ return []; };
                if (!(this instanceof Event)) return obj;
            };

            globalThis.CustomEvent = function CustomEvent(type, init) {
                globalThis.Event.call(this, type, init);
                this.detail = (init && init.detail !== undefined) ? init.detail : null;
            };
            globalThis.CustomEvent.prototype = Object.create(globalThis.Event.prototype);
            globalThis.CustomEvent.prototype.constructor = globalThis.CustomEvent;

            globalThis.ErrorEvent = function ErrorEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.message = init.message || ''; this.error = init.error || null;
            };
            globalThis.ErrorEvent.prototype = Object.create(globalThis.Event.prototype);

            globalThis.MessageEvent = function MessageEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.data = init.data !== undefined ? init.data : null;
                this.origin = init.origin || ''; this.source = init.source || null;
            };
            globalThis.MessageEvent.prototype = Object.create(globalThis.Event.prototype);

            globalThis.PromiseRejectionEvent = function PromiseRejectionEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.promise = init.promise || null; this.reason = init.reason;
            };
            globalThis.PromiseRejectionEvent.prototype = Object.create(globalThis.Event.prototype);
        })();
    "#;
    let _ = context.eval(Source::from_bytes(constructors_js.as_bytes()));

    // customElements registry — YouTube defines dozens of custom elements at startup.
    let custom_elements_js = r#"
        (function() {
            var registry = {};
            globalThis.customElements = {
                define: function(name, ctor, opts) { registry[name] = ctor; },
                get: function(name) { return registry[name]; },
                whenDefined: function(name) {
                    return registry[name] ? Promise.resolve(registry[name]) : new Promise(function(res) {
                        var orig = customElements.define;
                        customElements.define = function(n, c, o) {
                            orig.call(customElements, n, c, o);
                            if (n === name) res(c);
                        };
                    });
                },
                upgrade: function() {}
            };
        })();
    "#;
    let _ = context.eval(Source::from_bytes(custom_elements_js.as_bytes()));

}

fn unsupported_crypto() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
        Err(JsNativeError::typ()
            .with_message("crypto randomness is not implemented in Aurora")
            .into())
    })
}
