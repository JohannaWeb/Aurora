        (function() {
            var registry = {};
            var pending = Object.create(null);
            var domModules = Object.create(null);
            var patchedCreateElement = false;
            var originalCreateElement = null;
            var hasOwn = Object.prototype.hasOwnProperty;
            var suppressTrackedConnect = 0;
            // Genuine Object.prototype methods that prop bags may legitimately
            // expose; everything else inherited-and-callable (our fallback
            // `style`/`__shady_*` shims) is treated as a stray signal value.
            var BUILTIN_OBJECT_METHODS = {
                hasOwnProperty: true, isPrototypeOf: true, propertyIsEnumerable: true,
                toLocaleString: true, toString: true, valueOf: true, constructor: true
            };

            (function installShadyEventFallbacks() {
                function defineFallback(name, fn) {
                    if (Object.prototype[name]) return;
                    try {
                        Object.defineProperty(Object.prototype, name, {
                            value: fn,
                            configurable: true,
                            writable: true
                        });
                    } catch (e) {}
                }
                defineFallback('__shady_addEventListener', function(){});
                defineFallback('__shady_removeEventListener', function(){});
                defineFallback('__shady_dispatchEvent', function(){ return true; });
            })();

            // Native construction-stack semantics for HTMLElement. ES5
            // bundles (YouTube kevlar) wrap HTMLElement with Polymer's
            // custom-elements-es5-adapter, whose constructors run
            // `Reflect.construct(HTMLElement, [], this.constructor)`. That
            // call must return the element currently being upgraded, and a
            // direct `new MyElement()` must produce a real DOM element with
            // the subclass prototype — not a plain object.
            var upgradeStack = [];
            (function patchHTMLElementForUpgrades() {
                var Native = globalThis.HTMLElement;
                if (typeof Native !== 'function') return;
                function PatchedHTMLElement() {
                    if (upgradeStack.length) {
                        return upgradeStack[upgradeStack.length - 1];
                    }
                    var ctor = (typeof new.target === 'function' ? new.target : null) ||
                        (this && this.constructor);
                    var definition = ctor && ctor.__aurora_ce_definition__;
                    if (definition && definition.name) {
                        ensureCreateElementPatch();
                        if (originalCreateElement) {
                            var el = originalCreateElement(definition.name);
                            try { Object.setPrototypeOf(el, ctor.prototype); } catch (e) {}
                            el.__ce_upgraded__ = true;
                            attachDefinitionMetadata(el, definition);
                            return el;
                        }
                    }
                    if (new.target) {
                        return this && typeof this === 'object'
                            ? this
                            : Object.create((ctor && ctor.prototype) || PatchedHTMLElement.prototype);
                    }
                    // Plain `HTMLElement.call(this)` outside an upgrade:
                    // behave like the previous native (return undefined so
                    // `... || this` keeps the caller's element).
                    return undefined;
                }
                PatchedHTMLElement.prototype = Native.prototype;
                try {
                    Object.defineProperty(PatchedHTMLElement.prototype, 'constructor', {
                        value: PatchedHTMLElement,
                        configurable: true,
                        writable: true
                    });
                } catch (e) {}
                try { Object.setPrototypeOf(PatchedHTMLElement, Native); } catch (e) {}
                globalThis.HTMLElement = PatchedHTMLElement;
            })();
            function trace(msg) {
                console.log('[yt-life] ' + msg);
            }
            function shouldTraceName(name) {
                return true;
            }
            function shouldSuppressLifecycle(name) {
                return name === 'snackbar-container' || name === 'yt-ephemeral-actions';
            }
            function traceError(where, error) {
                var message = error && (error.name || 'Error') + ': ' + (error.message || '');
                var stack = error && error.stack ? ('\n' + error.stack) : '';
                console.log('[yt-life] ERROR ' + where + ': ' + (message || String(error)) + stack);
            }

            (function installGlobalErrorTracing() {
                if (globalThis.__aurora_global_error_tracing__) return;
                try {
                    Object.defineProperty(globalThis, '__aurora_global_error_tracing__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    globalThis.__aurora_global_error_tracing__ = true;
                }
                try {
                    globalThis.addEventListener('error', function(event) {
                        try {
                            traceError('window.error',
                                event && event.error ? event.error :
                                (event && event.message ? new Error(event.message) : event));
                        } catch (e) {}
                    });
                } catch (e) {}
                try {
                    globalThis.addEventListener('unhandledrejection', function(event) {
                        try {
                            var reason = event && 'reason' in event ? event.reason : event;
                            traceError('unhandledrejection',
                                reason && reason.error ? reason.error :
                                (reason instanceof Error ? reason : new Error(String(reason))));
                        } catch (e) {}
                    });
                } catch (e) {}
            })();

            function debugProbeName(name) {
                return name === 'ytd-app' || name === 'ytd-browse' || name === 'ytd-masthead' || name === 'yt-mdx-manager';
            }

            function shouldTrack(name) {
                return !!name && name.indexOf('-') >= 0;
            }

            function getElementId(el) {
                if (!el) return '';
                try {
                    if (typeof el.id === 'string' && el.id) return el.id;
                } catch (e) {}
                try {
                    if (typeof el.getAttribute === 'function') {
                        return el.getAttribute('id') || '';
                    }
                } catch (e) {}
                return '';
            }

            function isActuallyConnected(el) {
                if (!el) return false;
                try {
                    if (el.isConnected === true) return true;
                } catch (e) {}
                var node = el;
                var guard = 0;
                while (node && guard++ < 1000) {
                    if (node === document) return true;
                    try {
                        if (node.nodeType === 9) return true;
                    } catch (e2) {}
                    var next = null;
                    try { next = node.parentNode || null; } catch (e3) {}
                    if (!next) {
                        try { next = node.host || null; } catch (e4) {}
                    }
                    node = next;
                }
                return false;
            }

            function camelCaseId(id) {
                return String(id).replace(/-([a-zA-Z0-9_])/g, function(_, ch) {
                    return String(ch).toUpperCase();
                });
            }

            function cssEscapeId(id) {
                if (globalThis.CSS && typeof CSS.escape === 'function') {
                    try { return CSS.escape(id); } catch (e) {}
                }
                return String(id).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
            }

            function findStampedId(host, id) {
                function ensureEventMethods(node) {
                    if (node && typeof node.addEventListener !== 'function' && globalThis.EventTarget) {
                        try {
                            node.addEventListener = EventTarget.prototype.addEventListener;
                            node.removeEventListener = EventTarget.prototype.removeEventListener;
                            node.dispatchEvent = EventTarget.prototype.dispatchEvent;
                        } catch (e) {}
                    }
                    return node;
                }
                if (!host) return undefined;
                try {
                    if (host.$ && host.$[id]) return ensureEventMethods(host.$[id]);
                } catch (e) {}
                var root = null;
                try { root = host.root || host.shadowRoot || host.__shady_shadowRoot || null; } catch (e) {}
                if (root && typeof root.querySelector === 'function') {
                    try {
                        var found = root.querySelector('#' + cssEscapeId(id));
                        if (found) return ensureEventMethods(found);
                    } catch (e) {
                        try {
                            var quoted = root.querySelector('[id="' + cssEscapeId(id) + '"]');
                            if (quoted) return ensureEventMethods(quoted);
                        } catch (e2) {}
                    }
                }
                if (typeof host.querySelector === 'function') {
                    try {
                        var light = host.querySelector('#' + cssEscapeId(id));
                        if (light) return ensureEventMethods(light);
                    } catch (e3) {}
                }
                return undefined;
            }

            function installTemplateIdAccessors(ctor, template) {
                if (!ctor || !ctor.prototype || !template || !template.content) return;
                var seen = ctor.__aurora_template_id_accessors__;
                if (!seen) {
                    seen = Object.create(null);
                    try {
                        Object.defineProperty(ctor, '__aurora_template_id_accessors__', {
                            value: seen,
                            configurable: true
                        });
                    } catch (e) {
                        ctor.__aurora_template_id_accessors__ = seen;
                    }
                }
                var nodes = [];
                try {
                    var all = template.content.querySelectorAll('*');
                    for (var i = 0; i < all.length; i++) nodes.push(all[i]);
                } catch (e) {
                    return;
                }
                function install(name, id) {
                    var existing = Object.getOwnPropertyDescriptor(ctor.prototype, name);
                    if (!name) return;
                    if (seen[name] && !(existing && existing.get && existing.get.__aurora_template_id_getter__)) return;
                    if (existing && existing.get !== undefined && !existing.get.__aurora_template_id_getter__) return;
                    seen[name] = true;
                    var getter = function() { return findStampedId(this, id); };
                    try {
                        Object.defineProperty(getter, '__aurora_template_id_getter__', {
                            value: true,
                            configurable: true
                        });
                    } catch (e) {
                        getter.__aurora_template_id_getter__ = true;
                    }
                    try {
                        Object.defineProperty(ctor.prototype, name, {
                            configurable: true,
                            enumerable: false,
                            get: getter,
                            set: function(value) {
                                Object.defineProperty(this, name, {
                                    configurable: true,
                                    enumerable: false,
                                    writable: true,
                                    value: value
                                });
                            }
                        });
                    } catch (e) {}
                }
                for (var n = 0; n < nodes.length; n++) {
                    var id = getElementId(nodes[n]);
                    if (!id) continue;
                    install(id, id);
                    var camel = camelCaseId(id);
                    if (camel !== id) install(camel, id);
                }
            }

            function installInstanceTemplateIdAccessors(el, ctor) {
                if (!el || !ctor) return;
                var template = null;
                try { template = ctor.template || null; } catch (e) {}
                if (!template) {
                    try { template = el._template || null; } catch (e2) {}
                }
                if (!template || !template.content || typeof template.content.querySelectorAll !== 'function') return;
                var nodes = [];
                try {
                    var all = template.content.querySelectorAll('*');
                    for (var i = 0; i < all.length; i++) nodes.push(all[i]);
                } catch (e) {
                    return;
                }
                function install(name, id) {
                    if (!name) return;
                    var existing = null;
                    try { existing = Object.getOwnPropertyDescriptor(el, name); } catch (e) {}
                    if (existing && existing.configurable === false) return;
                    try {
                        Object.defineProperty(el, name, {
                            configurable: true,
                            enumerable: false,
                            get: function() { return findStampedId(this, id); },
                            set: function(value) {
                                Object.defineProperty(this, name, {
                                    configurable: true,
                                    enumerable: false,
                                    writable: true,
                                    value: value
                                });
                            }
                        });
                    } catch (e) {}
                }
                for (var n = 0; n < nodes.length; n++) {
                    var id = getElementId(nodes[n]);
                    if (!id) continue;
                    install(id, id);
                    var camel = camelCaseId(id);
                    if (camel !== id) install(camel, id);
                }
            }

            function findTemplateForDomModule(el) {
                if (!el) return null;
                try {
                    if (el.__aurora_template__) return el.__aurora_template__;
                } catch (e) {}

                var tpl = null;
                try {
                    if (typeof el.querySelector === 'function') {
                        tpl = el.querySelector('template');
                    }
                } catch (e) {}

                if (!tpl) {
                    try {
                        if (el.content && el.content.nodeType === 11) {
                            tpl = el.content;
                        }
                    } catch (e) {}
                }

                if (tpl) {
                    try { el.__aurora_template__ = tpl; } catch (e) {}
                }
                return tpl;
            }

            // ── ShadyCSS-lite ────────────────────────────────────────────────
            // Aurora flattens shadow DOM into light DOM (attachShadow proxies the
            // host's own subtree) and paints via blitz/Stylo from a serialized
            // light-DOM HTML string. A component's <style> lives inside its
            // <dom-module><template>, which is inert and never serialized — and
            // even when reached, its shadow-scoped selectors (:host, ::slotted)
            // match nothing in the flattened tree, so components stamp but render
            // unstyled (collapsed layout boxes).
            //
            // This shim hoists each dom-module's <style> into <head> (light DOM,
            // so it serializes and paints) and rewrites the shadow-scoped
            // selectors to target the flattened tree, scoping rules by the
            // component's tag name so they don't leak across components.
            var shimmedStyleScopes = Object.create(null);

            // Split `str` on top-level `sep`, ignoring it inside (), [], "", ''.
            function splitTopLevel(str, sep) {
                var out = [], depth = 0, quote = null, start = 0;
                for (var i = 0; i < str.length; i++) {
                    var c = str[i];
                    if (quote) { if (c === quote && str[i - 1] !== '\\') quote = null; continue; }
                    if (c === '"' || c === "'") { quote = c; continue; }
                    if (c === '(' || c === '[') depth++;
                    else if (c === ')' || c === ']') depth--;
                    else if (c === sep && depth === 0) { out.push(str.slice(start, i)); start = i + 1; }
                }
                out.push(str.slice(start));
                return out;
            }

            // Rewrite one complex selector for the flattened (no-shadow) tree.
            function rewriteScopedSelector(sel, tag) {
                sel = sel.trim();
                if (!sel) return sel;
                // :host-context(x) y  ->  x tag y   (approx: ancestor context)
                var ctx = sel.match(/^:host-context\(([^)]*)\)\s*([\s\S]*)$/);
                if (ctx) {
                    var rest = ctx[2].trim();
                    return (ctx[1].trim() + ' ' + tag + (rest ? ' ' + rest : '')).trim();
                }
                if (sel.indexOf(':host') !== -1) {
                    // :host(x) -> tagx ; :host -> tag  (combinators preserved)
                    return sel.replace(/:host\(([^)]*)\)/g, tag + '$1').replace(/:host/g, tag);
                }
                if (sel.indexOf('::slotted') !== -1) {
                    // flattened: slotted light children are plain descendants
                    return sel.replace(/::slotted\(([^)]*)\)/g, tag + ' $1');
                }
                // Component-internal rule: scope as a descendant of the host tag.
                return tag + ' ' + sel;
            }

            function rewriteSelectorList(list, tag) {
                return splitTopLevel(list, ',').map(function(s) {
                    return rewriteScopedSelector(s, tag);
                }).join(', ');
            }

            // Walk top-level rules, rewriting selector preludes. Recurses into
            // @media/@supports; leaves @keyframes/@font-face/@import untouched.
            function scopeCss(css, tag) {
                var out = '', i = 0, n = css.length, prelude = '';
                while (i < n) {
                    var c = css[i];
                    if (c === '{') {
                        var depth = 1, j = i + 1;
                        while (j < n && depth > 0) {
                            if (css[j] === '{') depth++;
                            else if (css[j] === '}') depth--;
                            j++;
                        }
                        var body = css.slice(i + 1, j - 1);
                        var pre = prelude.trim();
                        if (pre.charAt(0) === '@') {
                            var low = pre.toLowerCase();
                            if (low.indexOf('@media') === 0 || low.indexOf('@supports') === 0) {
                                out += pre + '{' + scopeCss(body, tag) + '}';
                            } else {
                                out += pre + '{' + body + '}';
                            }
                        } else if (pre) {
                            out += rewriteSelectorList(pre, tag) + '{' + body + '}';
                        } else {
                            out += '{' + body + '}';
                        }
                        prelude = '';
                        i = j;
                    } else {
                        prelude += c;
                        i++;
                    }
                }
                return out + prelude; // keep any trailing @import/;-rule verbatim
            }

            function shimDomModuleStyles(id, tpl) {
                if (!id || shimmedStyleScopes[id]) return;
                var content = tpl && tpl.content;
                if (!content || typeof content.querySelectorAll !== 'function') return;
                var styles;
                try { styles = content.querySelectorAll('style'); } catch (e) { return; }
                if (!styles || !styles.length) return;
                shimmedStyleScopes[id] = true;
                var head = document.head || document.documentElement;
                if (!head) return;
                for (var s = 0; s < styles.length; s++) {
                    var cssText = '';
                    try { cssText = styles[s].textContent || ''; } catch (e) {}
                    // strip comments first so braces/quotes inside them can't
                    // unbalance the rule scanner
                    cssText = cssText.replace(/\/\*[\s\S]*?\*\//g, '');
                    if (!cssText.trim()) continue;
                    var scoped;
                    try { scoped = scopeCss(cssText, id); } catch (e) { scoped = null; }
                    if (!scoped) continue;
                    try {
                        var out = document.createElement('style');
                        out.setAttribute('data-style-scope', id);
                        out.textContent = scoped;
                        head.appendChild(out);
                    } catch (e) {}
                }
            }

            function registerDomModule(el) {
                var id = getElementId(el);
                if (!id) return null;
                var tpl = findTemplateForDomModule(el);
                if (!tpl) return null;
                domModules[id] = tpl;
                try { shimDomModuleStyles(id, tpl); } catch (e) {}
                if (globalThis.__aurora_debug_youtube__ && debugProbeName(id)) {
                    trace('dom-module registered ' + id +
                        ' template=' + (!!tpl) +
                        ' content=' + (!!(tpl && tpl.content)) +
                        ' contentKids=' + (tpl && tpl.content && tpl.content.childNodes ? tpl.content.childNodes.length : '?'));
                }
                return tpl;
            }

            var probedTemplateBuild = false;
            function probeCustomElementState(name, el, ctor) {
                if (!globalThis.__aurora_debug_youtube__ || !debugProbeName(name)) return;
                if (!probedTemplateBuild) {
                    probedTemplateBuild = true;
                    try {
                        var ptpl = document.createElement('template');
                        ptpl.innerHTML = '<div><span>probe</span></div>';
                        var pshared = document.createElement('template');
                        var pclone = pshared.content.cloneNode(true);
                        ptpl.content.insertBefore(pclone, ptpl.content.firstChild);
                        trace('template-build-smoke kids=' +
                            (ptpl.content && ptpl.content.childNodes ? ptpl.content.childNodes.length : '?'));
                    } catch (e) { traceError('template-build-smoke', e); }
                    try {
                        var t2 = document.createElement('template');
                        t2.innerHTML = '<div id="a"><span id="b">x</span></div><p id="c">y</p>';
                        var c2 = t2.content;
                        trace('content childNodes.length=' + (c2 && c2.childNodes ? c2.childNodes.length : 'n/a'));
                        var fc2 = c2 && c2.firstChild;
                        trace('content.firstChild=' + (fc2 ? (fc2.tagName || fc2.nodeName) : String(fc2)));
                        if (fc2) {
                            trace('firstChild.nextSibling=' + (fc2.nextSibling ? (fc2.nextSibling.tagName || fc2.nextSibling.nodeName) : String(fc2.nextSibling)));
                            trace('firstChild.parentNode===content=' + (fc2.parentNode === c2));
                        }
                        var clone2 = c2.cloneNode(true);
                        trace('clone.childNodes.length=' + (clone2 && clone2.childNodes ? clone2.childNodes.length : 'n/a'));
                        trace('typeof importNode=' + typeof document.importNode);
                        if (typeof document.importNode === 'function') {
                            var imp2 = document.importNode(c2, true);
                            trace('importNode.childNodes.length=' + (imp2 && imp2.childNodes ? imp2.childNodes.length : 'n/a'));
                        }
                    } catch (e) { traceError('template-content-probe', e); }
                    try {
                        var t3 = document.createElement('template');
                        t3.innerHTML = '<!--css-build:shady--><!--scope--><yt-guide-manager id="guide-service" disabled="[[standalone]]" guide-persistent-and-visible="[[guidePersistentAndVisible]]"></yt-guide-manager><div id="x">y</div>';
                        var c3 = t3.content;
                        trace('comment-prefixed childNodes.length=' + (c3 && c3.childNodes ? c3.childNodes.length : 'n/a'));
                        var fc3 = c3 && c3.firstChild;
                        trace('comment-prefixed firstChild=' + (fc3 ? (fc3.nodeType + ':' + (fc3.tagName || fc3.nodeName)) : String(fc3)));
                        if (fc3) {
                            trace('comment-prefixed firstChild.nextSibling=' + (fc3.nextSibling ? (fc3.nextSibling.nodeType + ':' + (fc3.nextSibling.tagName || fc3.nextSibling.nodeName)) : String(fc3.nextSibling)));
                        }
                        if (c3 && c3.childNodes) {
                            for (var ci = 0; ci < c3.childNodes.length; ci++) {
                                var cn = c3.childNodes[ci];
                                trace('comment-prefixed childNodes[' + ci + ']=' + cn.nodeType + ':' + (cn.tagName || cn.nodeName));
                            }
                        }
                    } catch (e) { traceError('comment-prefixed-probe', e); }
                }
                try {
                    var app = el || (typeof document !== 'undefined' && document.querySelector
                        ? document.querySelector(name)
                        : null);
                    var regCtor = ctor || (globalThis.customElements && typeof customElements.get === 'function'
                        ? customElements.get(name)
                        : null);
                    var mod = typeof document !== 'undefined' && document.querySelector
                        ? document.querySelector('dom-module#' + name)
                        : null;
                    var modTemplate = null;
                    try {
                        modTemplate = mod && typeof mod.querySelector === 'function'
                            ? mod.querySelector('template')
                            : null;
                    } catch (e) {}
                    // Reading `template` can make Polymer cache an own
                    // `_template` on the ctor before templates are wired up;
                    // observe without mutating by undoing a cache we created.
                    var hadOwnTplCache = regCtor && hasOwn.call(regCtor, '_template');
                    var ctorTemplate;
                    try { ctorTemplate = regCtor && regCtor.template; } catch (e) { ctorTemplate = 'THREW:' + e.message; }
                    if (regCtor && !hadOwnTplCache && hasOwn.call(regCtor, '_template')) {
                        try { delete regCtor._template; } catch (e) {}
                    }
                    var ctorOwnTemplate;
                    try { ctorOwnTemplate = regCtor && regCtor._template; } catch (e) { ctorOwnTemplate = 'THREW:' + e.message; }
                    var appTemplate;
                    try { appTemplate = app && app._template; } catch (e) { appTemplate = 'THREW:' + e.message; }
                    var appRoot;
                    try { appRoot = app && app.root; } catch (e) { appRoot = 'THREW:' + e.message; }
                    var appShadowRoot;
                    try { appShadowRoot = app && app.shadowRoot; } catch (e) { appShadowRoot = 'THREW:' + e.message; }
                    var protoTplDesc = 'none';
                    var protoTplValue = 'unread';
                    try {
                        var ptd = regCtor && regCtor.prototype
                            ? Object.getOwnPropertyDescriptor(regCtor.prototype, '_template')
                            : null;
                        if (ptd) {
                            protoTplDesc = ptd.get ? 'getter' : 'value';
                            try {
                                var ptv = ptd.get ? ptd.get.call(app || regCtor.prototype) : ptd.value;
                                protoTplValue = ptv === undefined ? 'undefined' : ptv === null ? 'null' : typeof ptv;
                            } catch (e) { protoTplValue = 'THREW:' + e.message; }
                        }
                    } catch (e) { protoTplDesc = 'THREW:' + e.message; }
                    var staticChain = '';
                    try {
                        var sc = regCtor;
                        var depth = 0;
                        while (sc && sc !== Function.prototype && depth < 8) {
                            var sd = Object.getOwnPropertyDescriptor(sc, 'template');
                            if (sd) {
                                staticChain += (staticChain ? ',' : '') + depth + ':' +
                                    (sd.get ? (sc.__aurora_template_accessor__ ? 'aurora-getter' : 'getter') : 'value');
                                if (sd.get && !sc.__aurora_template_accessor__) {
                                    var hadOwnBefore = hasOwn.call(regCtor, '_template');
                                    var rawResult;
                                    try {
                                        var raw = sd.get.call(regCtor);
                                        rawResult = raw === null ? 'null' : raw === undefined ? 'undefined' : typeof raw;
                                    } catch (e) { rawResult = 'THREW:' + e.message; }
                                    trace('static-template depth=' + depth +
                                        ' raw=' + rawResult +
                                        ' ownTplAfter=' + (hasOwn.call(regCtor, '_template') ? String(regCtor._template) : 'no-own') +
                                        ' src=' + String(sd.get).replace(/\s+/g, ' ').slice(0, 300));
                                    if (!hadOwnBefore && hasOwn.call(regCtor, '_template')) {
                                        try { delete regCtor._template; } catch (e) {}
                                    }
                                }
                            }
                            sc = Object.getPrototypeOf(sc);
                            depth++;
                        }
                        if (!staticChain) staticChain = 'none';
                    } catch (e) { staticChain = 'THREW:' + e.message; }
                    try {
                        var ptd2 = regCtor && regCtor.prototype
                            ? Object.getOwnPropertyDescriptor(regCtor.prototype, '_template')
                            : null;
                        if (ptd2 && ptd2.get) {
                            trace('proto-template-getter src=' + String(ptd2.get).replace(/\s+/g, ' ').slice(0, 300));
                        }
                    } catch (e) {}
                    trace(
                        'probe ' + name +
                        ' app=' + (!!app) +
                        ' ctor=' + (!!regCtor) +
                        ' ctor.template=' + (ctorTemplate === undefined ? 'undefined' : ctorTemplate === null ? 'null' : typeof ctorTemplate) +
                        ' ctor._template=' + (ctorOwnTemplate === undefined ? 'undefined' : ctorOwnTemplate === null ? 'null' : typeof ctorOwnTemplate) +
                        ' app._template=' + (appTemplate === undefined ? 'undefined' : appTemplate === null ? 'null' : typeof appTemplate) +
                        ' app.root=' + (appRoot === undefined ? 'undefined' : appRoot === null ? 'null' : typeof appRoot) +
                        ' app.shadowRoot=' + (appShadowRoot === undefined ? 'undefined' : appShadowRoot === null ? 'null' : typeof appShadowRoot) +
                        ' proto._template=' + protoTplDesc + '/' + protoTplValue +
                        ' staticTemplates=' + staticChain +
                        ' dom-module=' + (!!mod) +
                        ' dom-module-template=' + (!!modTemplate) +
                        ' dom-module-content=' + (!!(modTemplate && modTemplate.content)) +
                        ' dom-module-content-kids=' + (modTemplate && modTemplate.content && modTemplate.content.childNodes ? modTemplate.content.childNodes.length : '?') +
                        ' kids=' + (app && app.children ? app.children.length : '?') +
                        ' enable=' + (app ? typeof app._enableProperties : 'undefined') +
                        ' dataEnabled=' + (app && app.__dataEnabled) +
                        ' dataReady=' + (app && app.__dataReady) +
                        ' ready=' + (app ? typeof app.ready : 'undefined') +
                        ' stamp=' + (app ? typeof app._stampTemplate : 'undefined') +
                        ' attachDom=' + (app ? typeof app._attachDom : 'undefined')
                    );
                } catch (e) {
                    traceError('probe ' + name, e);
                }
            }

            function getDefinition(nameOrCtor) {
                if (!nameOrCtor) return null;
                if (typeof nameOrCtor === 'string') {
                    return registry[nameOrCtor] || null;
                }
                if (typeof nameOrCtor === 'function') {
                    var tagName = nameOrCtor.__aurora_ce_name__;
                    return tagName ? registry[tagName] || null : null;
                }
                return null;
            }

            function ensureDefinitionMetadata(name, ctor) {
                var existing = registry[name];
                if (existing) {
                    existing.ctor = ctor;
                    return existing;
                }
                var definition = { name: name, ctor: ctor };
                registry[name] = definition;
                return definition;
            }

            function attachDefinitionMetadata(target, definition) {
                if (!target || !definition) return;
                try {
                    Object.defineProperty(target, '__aurora_ce_definition__', {
                        value: definition,
                        configurable: true,
                        writable: true
                    });
                } catch (e) {
                    target.__aurora_ce_definition__ = definition;
                }
                if (definition.name) {
                    try {
                        Object.defineProperty(target, '__aurora_ce_name__', {
                            value: definition.name,
                            configurable: true,
                            writable: true
                        });
                    } catch (e) {
                        target.__aurora_ce_name__ = definition.name;
                    }
                }
                if (definition.ctor) {
                    try {
                        Object.defineProperty(target, '__aurora_ce_ctor__', {
                            value: definition.ctor,
                            configurable: true,
                            writable: true
                        });
                    } catch (e) {
                        target.__aurora_ce_ctor__ = definition.ctor;
                    }
                }
            }

            function installTemplateAccessor(name, ctor) {
                if (!ctor || ctor.__aurora_template_accessor__) return;
                var definition = ensureDefinitionMetadata(name, ctor);
                var descriptor = Object.getOwnPropertyDescriptor(ctor, 'template');
                if (!descriptor) {
                    // If the framework already provides a static `template`
                    // somewhere on the constructor chain (Polymer 3's
                    // ElementMixin getter, kevlar base classes), leave it
                    // alone. Shadowing it breaks resolution order, and even
                    // reading it early poisons Polymer's own-property
                    // `_template` cache before templates are wired up.
                    var parent = Object.getPrototypeOf(ctor);
                    while (parent && parent !== Function.prototype) {
                        if (Object.getOwnPropertyDescriptor(parent, 'template')) return;
                        parent = Object.getPrototypeOf(parent);
                    }
                }
                var originalGetter = descriptor && descriptor.get;
                var originalValue = descriptor && hasOwn.call(descriptor, 'value')
                    ? descriptor.value
                    : undefined;
                var ownTemplate = originalValue;

                try {
                    Object.defineProperty(ctor, 'template', {
                        configurable: true,
                        enumerable: descriptor ? descriptor.enumerable : false,
                        get: function() {
                            var template = ownTemplate;
                            if (!template && originalGetter) {
                                try {
                                    template = originalGetter.call(this);
                                } catch (e) {
                                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(definition.name)) {
                                        traceError('template own-getter ' + definition.name, e);
                                    }
                                    template = null;
                                }
                            }
                            if (!template) {
                                // Defer to an inherited static `template`
                                // (Polymer 3's ElementMixin getter, or kevlar
                                // bundles assigning one on a base class)
                                // before the dom-module fallback. Resolved at
                                // get time because frameworks assign it
                                // lazily, after customElements.define.
                                var parent = Object.getPrototypeOf(this || ctor);
                                while (parent && parent !== Function.prototype) {
                                    var inherited = Object.getOwnPropertyDescriptor(parent, 'template');
                                    if (inherited) {
                                        try {
                                            template = inherited.get
                                                ? inherited.get.call(this)
                                                : inherited.value;
                                        } catch (e) {
                                            if (globalThis.__aurora_debug_youtube__ && debugProbeName(definition.name)) {
                                                traceError('template inherited-getter ' + definition.name, e);
                                            }
                                            template = null;
                                        }
                                        break;
                                    }
                                    parent = Object.getPrototypeOf(parent);
                                }
                            }
                            if (template) {
                                installTemplateIdAccessors(ctor, template);
                                return template;
                            }
                            var moduleId = definition.name || (this && this.is) || '';
                            if (moduleId && domModules[moduleId]) {
                                installTemplateIdAccessors(ctor, domModules[moduleId]);
                                return domModules[moduleId];
                            }
                            return null;
                        },
                        set: function(value) {
                            ownTemplate = value;
                        }
                    });
                    ctor.__aurora_template_accessor__ = true;
                } catch (e) {
                    // If the property is not configurable, leave the original in place.
                }

                if (!hasOwn.call(ctor, 'is')) {
                    try {
                        Object.defineProperty(ctor, 'is', {
                            configurable: true,
                            enumerable: false,
                            get: function() {
                                return definition.name;
                            }
                        });
                    } catch (e) {}
                }

                attachDefinitionMetadata(ctor, definition);
                if (ctor.prototype && typeof ctor.prototype === 'object') {
                    try {
                        Object.defineProperty(ctor.prototype, 'constructor', {
                            value: ctor,
                            configurable: true,
                            writable: true
                        });
                    } catch (e) {}
                    attachDefinitionMetadata(ctor.prototype, definition);
                }
            }

            function rebuildPolymerIdMap(el) {
                if (!el || el.nodeType !== 1) return;
                var root = null;
                try { root = el.root || el.shadowRoot || el.__shady_shadowRoot || null; } catch (e) {}
                var map = el.$ && typeof el.$ === 'object' ? el.$ : {};
                var nodes = [];
                function collectIds(from) {
                    if (!from || typeof from.querySelectorAll !== 'function') return;
                    suppressTrackedConnect++;
                    try {
                        var all = from.querySelectorAll('*');
                        for (var i = 0; i < all.length; i++) nodes.push(all[i]);
                    } catch (e) {
                    } finally {
                        suppressTrackedConnect--;
                    }
                }
                function installAlias(name, child) {
                    if (!name) return;
                    var currentValue;
                    var ownDescriptor = null;
                    var shouldInstall = !(name in el);
                    try { ownDescriptor = Object.getOwnPropertyDescriptor(el, name); } catch (e) {}
                    if (!shouldInstall) {
                        try {
                            currentValue = el[name];
                            shouldInstall = currentValue == null;
                        } catch (e) {}
                    }
                    if (!shouldInstall) return;
                    if (ownDescriptor && ownDescriptor.configurable) {
                        try { delete el[name]; } catch (e) {}
                    }
                    try {
                        Object.defineProperty(el, name, {
                            configurable: true,
                            enumerable: false,
                            writable: true,
                            value: child
                        });
                    } catch (e) {
                        try { el[name] = child; } catch (e2) {}
                        try {
                            if (el[name] == null) {
                                Object.defineProperty(el, '__aurora_id_alias_' + name, {
                                    configurable: true,
                                    enumerable: false,
                                    writable: true,
                                    value: child
                                });
                            }
                        } catch (e3) {}
                    }
                }
                collectIds(root);
                // Some Polymer components stamp into the host/light subtree
                // before `root` is exposed. The id contract still needs to be
                // available by `ready()`, so use the host subtree as a fallback.
                collectIds(el);
                try {
                    if (root && root.nodeType === 1) nodes.push(root);
                } catch (e) {}
                if (!nodes.length) return;
                for (var n = 0; n < nodes.length; n++) {
                    var child = nodes[n];
                    var id = getElementId(child);
                    if (!id) continue;
                    if (typeof child.addEventListener !== 'function' && globalThis.EventTarget) {
                        try {
                            child.addEventListener = EventTarget.prototype.addEventListener;
                            child.removeEventListener = EventTarget.prototype.removeEventListener;
                            child.dispatchEvent = EventTarget.prototype.dispatchEvent;
                        } catch (e) {}
                    }
                    map[id] = child;
                    installAlias(id, child);
                    var camel = camelCaseId(id);
                    if (camel !== id) installAlias(camel, child);
                }
                try { el.$ = map; } catch (e) {}
                if (globalThis.__aurora_debug_youtube__ && (el.localName === 'ytd-app' || el.localName === 'tp-yt-app-drawer')) {
                    try { trace('id-map ' + el.localName + ' keys=' + Object.keys(map).join(',')); } catch (e) {}
                }
            }

            function installPolymerIdMapHooks(el) {
                if (!el || el.__aurora_id_map_hooks__) return;
                try {
                    Object.defineProperty(el, '__aurora_id_map_hooks__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    el.__aurora_id_map_hooks__ = true;
                }
                if (typeof el._attachDom === 'function' && !el._attachDom.__aurora_id_map_wrapped__) {
                    var originalAttachDom = el._attachDom;
                    var wrappedAttachDom = function() {
                        if (globalThis.__aurora_debug_youtube__ && debugProbeName(this.localName)) {
                            try {
                                var w = (globalThis.ShadyDOM && ShadyDOM.wrap) ? ShadyDOM.wrap(this) : this;
                                trace('attachDom-pre ' + this.localName +
                                    ' wrapIsNode=' + (w === this) +
                                    ' typeof w.attachShadow=' + typeof w.attachShadow +
                                    ' w.shadowRoot=' + String(w.shadowRoot) +
                                    ' typeof this.__shady_attachShadow=' + typeof this.__shady_attachShadow +
                                    ' ownShadyAttach=' + Object.prototype.hasOwnProperty.call(this, '__shady_attachShadow') +
                                    ' this.shadowRoot=' + String(this.shadowRoot) +
                                    ' this.__shady_shadowRoot=' + String(this.__shady_shadowRoot));
                            } catch (e) { traceError('attachDom-pre', e); }
                        }
                        suppressTrackedConnect++;
                        try {
                            return originalAttachDom.apply(this, arguments);
                        } finally {
                            suppressTrackedConnect--;
                            rebuildPolymerIdMap(this);
                        }
                    };
                    try {
                        Object.defineProperty(wrappedAttachDom, '__aurora_id_map_wrapped__', {
                            value: true,
                            configurable: true
                        });
                    } catch (e) {
                        wrappedAttachDom.__aurora_id_map_wrapped__ = true;
                    }
                    try { el._attachDom = wrappedAttachDom; } catch (e) {}
                }
            }

            function resolveSignalValue(value) {
                for (var depth = 0; depth < 4 && typeof value === 'function'; depth++) {
                    try {
                        value = value();
                    } catch (e) {
                        return undefined;
                    }
                }
                return typeof value === 'function' ? undefined : value;
            }

            function sanitizePropBag(bag) {
                if (!bag || typeof bag !== 'object') return bag;
                var keys = Object.keys(bag);
                for (var i = 0; i < keys.length; i++) {
                    var key = keys[i];
                    var value = bag[key];
                    if (typeof value === 'function') {
                        value = resolveSignalValue(value);
                        bag[key] = value;
                    }
                    if (value && typeof value === 'object' && !Array.isArray(value)) {
                        sanitizePropBag(value);
                    }
                }
                return bag;
            }

            function normalizeAttributedStringProps(el) {
                if (!el || el.localName !== 'yt-attributed-string') return;
                var props = [
                    'ariaHidden', 'ariaLabel', 'ellipsisTruncate', 'isOverlay',
                    'linkInheritColor', 'noEndpoints', 'noStyleRuns', 'noLinkColor',
                    'noPreWrap', 'noWrap', 'skipOnClick', 'userInput', 'headerRuns',
                    'isHeadline', 'data', 'id', 'className', 'hidden', 'style'
                ];
                var raw = el.rawProps && typeof el.rawProps === 'object' ? el.rawProps : null;
                for (var i = 0; i < props.length; i++) {
                    var prop = props[i];
                    var value;
                    var hasValue = false;
                    if (raw && hasOwn.call(raw, prop)) {
                        value = raw[prop];
                        hasValue = true;
                    } else {
                        try {
                            value = el[prop];
                            hasValue = true;
                        } catch (e) {}
                    }
                    if (!hasValue || typeof value !== 'function') continue;
                    for (var depth = 0; depth < 4 && typeof value === 'function'; depth++) {
                        try { value = value.call(el); } catch (e) { value = undefined; }
                    }
                    if (typeof value === 'function') value = undefined;
                    if (!raw) {
                        raw = {};
                        try { el.rawProps = raw; } catch (e) {}
                    }
                    if (raw) raw[prop] = value;
                }
            }

            function installSetUpPropsHook(ctor, name) {
                if (!ctor || !ctor.prototype || ctor.prototype.__aurora_setUpProps_hooked__) return;
                var original = ctor.prototype.setUpProps;
                if (typeof original !== 'function') return;
                try {
                    Object.defineProperty(ctor.prototype, '__aurora_setUpProps_hooked__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    ctor.prototype.__aurora_setUpProps_hooked__ = true;
                }
                ctor.prototype.setUpProps = function() {
                    sanitizePropBag(this.rawProps);
                    sanitizePropBag(this.componentProps);
                    sanitizePropBag(this.slotProps);
                    return original.apply(this, arguments);
                };
            }

            function invokeBeforeRegister(ctor, name) {
                if (!ctor || ctor.__aurora_before_register_called__) return;
                var target = ctor.prototype || ctor;
                var fn = target && typeof target.beforeRegister === 'function'
                    ? target.beforeRegister
                    : typeof ctor.beforeRegister === 'function'
                        ? ctor.beforeRegister
                        : null;
                if (!fn) return;
                try {
                    Object.defineProperty(ctor, '__aurora_before_register_called__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    ctor.__aurora_before_register_called__ = true;
                }
                try {
                    fn.call(target);
                } catch (e) {
                    traceError('beforeRegister ' + name, e);
                }
            }

            function maybeCallCreated(el, name) {
                if (!el || el.__ce_created__ || typeof el.created !== 'function') return;
                el.__ce_created__ = true;
                if (shouldTraceName(name)) trace('created ' + name);
                try {
                    el.created();
                } catch (e) {
                    traceError('created ' + name, e);
                }
            }

            function installInstanceSetUpPropsHook(el) {
                if (!el || el.localName !== 'yt-attributed-string' || el.__aurora_setUpProps_instance_hooked__) return;
                var original = el.setUpProps;
                if (typeof original !== 'function') return;
                try {
                    Object.defineProperty(el, '__aurora_setUpProps_instance_hooked__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    el.__aurora_setUpProps_instance_hooked__ = true;
                }
                el.setUpProps = function() {
                    // YouTube's setUpProps copies declared prop values into
                    // `rawProps` and then throws "Function props must be
                    // configured as STATIC, not SIGNAL." if any SIGNAL prop's
                    // value is a Function. The validation reads `rawProps[name]`
                    // for every declared prop, which walks the prototype chain.
                    // Our bootstrap installs a callable fallback `style` (and
                    // `__shady_*` helpers) on Object.prototype, so for the
                    // declared `style` prop `rawProps.style` resolves to that
                    // callable and trips the check. Wrap rawProps in a Proxy that
                    // neutralizes any such function to its resolved (unset) value,
                    // while leaving genuine Object.prototype builtins intact.
                    try {
                        var realRaw = this.rawProps;
                        if (realRaw && typeof realRaw === 'object' && !realRaw.__aurora_raw_proxy__) {
                            this.rawProps = new Proxy(realRaw, {
                                set: function(target, key, value) {
                                    if (typeof value === 'function') value = resolveSignalValue(value);
                                    target[key] = value;
                                    return true;
                                },
                                get: function(target, key) {
                                    var value = target[key];
                                    // Neutralize own data-prop functions and any
                                    // inherited callable that isn't a genuine
                                    // Object.prototype builtin (e.g. our polluting
                                    // `style`/`__shady_*` shims). Use own-property
                                    // checks throughout: the builtin table itself
                                    // inherits the polluted `style` getter.
                                    if (typeof value === 'function'
                                        && (hasOwn.call(target, key)
                                            || !hasOwn.call(BUILTIN_OBJECT_METHODS, key))) {
                                        return resolveSignalValue(value);
                                    }
                                    return value;
                                }
                            });
                            try {
                                Object.defineProperty(realRaw, '__aurora_raw_proxy__', {
                                    value: true,
                                    configurable: true
                                });
                            } catch (e) {}
                        }
                    } catch (e) {}
                    sanitizePropBag(this.rawProps);
                    sanitizePropBag(this.componentProps);
                    sanitizePropBag(this.slotProps);
                    return original.apply(this, arguments);
                };
            }

            function installRichGridFallback(el) {
                if (!el || el.localName !== 'ytd-rich-grid-renderer' || el.__aurora_rich_grid_fallback__) return;
                // Synthesizes placeholder sections/boxes into document.body when real
                // Polymer rendering hasn't populated #contents yet. Now that
                // drive_polymer_page_manager_navigation() makes real dataChanged()-driven
                // rendering work, this fallback only adds duplicate/misleading debug
                // boxes on top of real content, so restrict it to debug runs.
                if (!globalThis.__aurora_debug_youtube__) return;
                try {
                    Object.defineProperty(el, '__aurora_rich_grid_fallback__', {
                        value: true,
                        configurable: true
                    });
                } catch (e) {
                    el.__aurora_rich_grid_fallback__ = true;
                }

                if (globalThis.__aurora_debug_youtube__) {
                    trace('rich-grid fallback install');
                }

                var fallbackAttempts = 0;
                var fallbackTimer = null;

                function describeContentValue(value) {
                    if (!value || typeof value !== 'object') return String(value || '');
                    try {
                        return Object.keys(value).slice(0, 8).join(',');
                    } catch (e) {
                        return 'object';
                    }
                }

                function describeNode(node) {
                    if (!node) return 'none';
                    var type = 'n/a';
                    var name = 'n/a';
                    var id = '';
                    var connected = 'n/a';
                    var kids = 'n/a';
                    try { type = String(node.nodeType); } catch (e) {}
                    try { name = node.tagName || node.nodeName || node.constructor && node.constructor.name || 'n/a'; } catch (e2) {}
                    try { id = node.id || ''; } catch (e3) {}
                    try { connected = String(!!node.isConnected); } catch (e4) {}
                    try { kids = node.children ? String(node.children.length) : (node.childNodes ? String(node.childNodes.length) : 'n/a'); } catch (e5) {}
                    return name + '#' + id + ' type=' + type + ' connected=' + connected + ' kids=' + kids;
                }

                function resolveContents() {
                    try {
                        var data = el.data || (el.__data && el.__data.data);
                        return data && Array.isArray(data.contents) ? data.contents : null;
                    } catch (e) {
                        return null;
                    }
                }

                function resolveContainer() {
                    function walk(root) {
                        var queue = [root];
                        var seen = 0;
                        while (queue.length && seen++ < 2000) {
                            var node = queue.shift();
                            if (!node) continue;
                            try {
                                var nodeId = getElementId(node);
                                if (nodeId === 'contents') return node;
                            } catch (e) {}
                            try {
                                if (node.id === 'contents') return node;
                            } catch (e2) {}
                            try {
                                if (node.$ && node.$.contents) return node.$.contents;
                            } catch (e3) {}
                            try {
                                if (node.children) {
                                    for (var i = 0; i < node.children.length; i++) {
                                        queue.push(node.children[i]);
                                    }
                                }
                            } catch (e4) {}
                            try {
                                if (node.childNodes) {
                                    for (var j = 0; j < node.childNodes.length; j++) {
                                        queue.push(node.childNodes[j]);
                                    }
                                }
                            } catch (e4b) {}
                            try {
                                if (node.shadowRoot) queue.push(node.shadowRoot);
                            } catch (e5) {}
                            try {
                                if (node.root && node.root !== node.shadowRoot) queue.push(node.root);
                            } catch (e6) {}
                            try {
                                if (node.content) queue.push(node.content);
                            } catch (e7) {}
                        }
                        return null;
                    }
                    function walkAncestors(node) {
                        var current = node;
                        var guard = 0;
                        while (current && guard++ < 200) {
                            var found = walk(current);
                            if (found) return found;
                            var next = null;
                            try { next = current.parentNode || null; } catch (e) {}
                            if (!next) {
                                try { next = current.host || null; } catch (e2) {}
                            }
                            current = next;
                        }
                        return null;
                    }
                    var container = null;
                    try {
                        if (el.$ && el.$.contents) container = el.$.contents;
                    } catch (e) {}
                    if (!container) {
                        try {
                            container = walk(el.root || el.shadowRoot || el.__shady_shadowRoot || el);
                        } catch (e3) {}
                    }
                    if (!container) {
                        try { container = walk(el); } catch (e2) {}
                    }
                    if (!container) {
                        try { container = walkAncestors(el.parentNode || el.host || el); } catch (e3) {}
                    }
                    return container || el;
                }

                function createItemRenderer(item) {
                    if (!item || typeof item !== 'object') return null;
                    var tag = null;
                    var data = null;
                    var summary = null;
                    if (item.richItemRenderer) {
                        tag = 'ytd-rich-item-renderer';
                        data = item.richItemRenderer;
                        summary = 'richItemRenderer';
                    } else if (item.richSectionRenderer) {
                        tag = 'ytd-rich-section-renderer';
                        data = item.richSectionRenderer;
                        summary = 'richSectionRenderer';
                    } else if (item.continuationItemRenderer) {
                        tag = 'ytd-continuation-item-renderer';
                        data = item.continuationItemRenderer;
                        summary = 'continuationItemRenderer';
                    } else {
                        var keys = [];
                        try { keys = Object.keys(item); } catch (e) {}
                        for (var ki = 0; ki < keys.length; ki++) {
                            var key = keys[ki];
                            if (!key || !/Renderer$/.test(key)) continue;
                            if (!item[key] || typeof item[key] !== 'object') continue;
                            var derivedTag = 'ytd-' + key
                                .replace(/Renderer$/, '')
                                .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
                                .replace(/_/g, '-')
                                .toLowerCase() + '-renderer';
                            tag = derivedTag;
                            data = item[key];
                            summary = key;
                            break;
                        }
                    }
                    if (!tag) return null;
                    var child = document.createElement(tag);
                    try {
                        child.data = data;
                    } catch (e) {
                        try {
                            child.__data = child.__data || {};
                            child.__data.data = data;
                        } catch (e2) {}
                    }
                    if (item.richSectionRenderer && data && data.content) {
                        try {
                            child.content = data.content;
                        } catch (e3) {
                            try {
                                child.__data = child.__data || {};
                                child.__data.content = data.content;
                            } catch (e4) {}
                        }
                        try {
                            child.__aurora_rich_section_data__ = data;
                        } catch (e5) {}
                    }
                    if (globalThis.__aurora_debug_youtube__) {
                        try {
                            var nested = data && data.content;
                            var nestedKey = nested ? Object.keys(nested).slice(0, 8).join(',') : '';
                            child.setAttribute('data-aurora-summary', summary + (nestedKey ? ' ' + nestedKey : ''));
                        } catch (e3) {}
                    }
                    return child;
                }

                function createSummaryNode(item) {
                    if (!item || typeof item !== 'object') return null;
                    var label = null;
                    var detail = '';
                    if (item.richSectionRenderer) {
                        label = 'section';
                        try {
                            var content = item.richSectionRenderer.content || {};
                            var contentKey = Object.keys(content).slice(0, 6).join(',');
                            detail = contentKey ? ' ' + contentKey : '';
                        } catch (e) {}
                    } else if (item.richItemRenderer) {
                        label = 'item';
                    } else if (item.continuationItemRenderer) {
                        label = 'continuation';
                    } else if (item.richShelfRenderer) {
                        label = 'shelf';
                    }
                    if (!label) return null;
                    var box = document.createElement('div');
                    try {
                        box.textContent = 'aurora ' + label + detail;
                        box.setAttribute('style', 'display:block;box-sizing:border-box;margin:8px;padding:12px;border:1px solid #c7c7c7;background:#fff;color:#111;font:14px/1.4 sans-serif;min-height:44px;');
                    } catch (e) {}
                    return box;
                }

                function installFeedNudgeFallback(feedNudge) {
                    if (!feedNudge || feedNudge.localName !== 'ytd-feed-nudge-renderer' || feedNudge.__aurora_feed_nudge_fallback__) return;
                    try {
                        Object.defineProperty(feedNudge, '__aurora_feed_nudge_fallback__', {
                            value: true,
                            configurable: true
                        });
                    } catch (e) {
                        feedNudge.__aurora_feed_nudge_fallback__ = true;
                    }

                    var attempts = 0;
                    function extractLabel(data) {
                        if (!data || typeof data !== 'object') return '';
                        var parts = [];
                        try {
                            var keys = Object.keys(data);
                            for (var i = 0; i < keys.length && parts.length < 6; i++) {
                                var key = keys[i];
                                if (!key) continue;
                                if (typeof data[key] === 'string' && data[key]) {
                                    parts.push(key + '=' + data[key].slice(0, 60));
                                    continue;
                                }
                                if (data[key] && typeof data[key] === 'object') {
                                    parts.push(key + '=' + Object.keys(data[key]).slice(0, 4).join(','));
                                }
                            }
                        } catch (e) {}
                        return parts.join(' ');
                    }
                    function render() {
                        attempts++;
                        var data = feedNudge.data || (feedNudge.__data && feedNudge.__data.data) || null;
                        var label = extractLabel(data);
                        var existing = null;
                        try {
                            existing = feedNudge.querySelector && feedNudge.querySelector('[data-aurora-feed-nudge-fallback]');
                        } catch (e2) {}
                        if (existing) return true;
                        if (!data && attempts < 4) return false;
                        var box = document.createElement('div');
                        try {
                            box.setAttribute('data-aurora-feed-nudge-fallback', '1');
                            box.setAttribute('style',
                                'display:block;box-sizing:border-box;margin:8px 0;padding:12px 14px;border:2px solid #2196f3;' +
                                'background:#fff;color:#111;font:600 16px/1.4 sans-serif;min-height:44px;');
                            box.textContent = 'aurora feedNudge ' + (label || describeContentValue(data));
                            if (document.body) {
                                document.body.insertBefore(box, document.body.firstChild);
                            } else {
                                feedNudge.appendChild(box);
                            }
                            if (globalThis.__aurora_debug_youtube__) trace('feed-nudge fallback appended ' + describeNode(feedNudge));
                        } catch (e3) {}
                        return true;
                    }
                    function schedule() {
                        if (attempts >= 12) return;
                        setTimeout(function() {
                            if (!render()) schedule();
                        }, attempts < 4 ? 0 : 50);
                    }
                    render();
                    schedule();
                }

                function installRichSectionFallback(section) {
                    if (!section || section.localName !== 'ytd-rich-section-renderer' || section.__aurora_rich_section_fallback__) return;
                    try {
                        Object.defineProperty(section, '__aurora_rich_section_fallback__', {
                            value: true,
                            configurable: true
                        });
                    } catch (e) {
                        section.__aurora_rich_section_fallback__ = true;
                    }

                    var attempts = 0;
                    function render() {
                        attempts++;
                        var data = section.data || (section.__data && section.__data.data) || section.__aurora_rich_section_data__ || null;
                        var content = data && data.content ? data.content : null;
                        var hasChildren = false;
                        try { hasChildren = !!(section.children && section.children.length); } catch (e) {}
                        if (!content) return false;
                        if (hasChildren) return true;
                        var existing = null;
                        try {
                            existing = section.querySelector && section.querySelector('[data-aurora-rich-section-fallback]');
                        } catch (e2) {}
                        if (existing) return true;
                        var box = document.createElement('div');
                        var nestedFrag = document.createDocumentFragment();
                        var nestedCount = 0;
                        try {
                            var contentKeys = Object.keys(content);
                            for (var ci = 0; ci < contentKeys.length; ci++) {
                                var contentKey = contentKeys[ci];
                                if (!contentKey || !/Renderer$/.test(contentKey)) continue;
                                var renderer = createItemRenderer((function() {
                                    var wrapped = {};
                                    wrapped[contentKey] = content[contentKey];
                                    return wrapped;
                                })());
                                if (renderer) {
                                    if (renderer.localName === 'ytd-feed-nudge-renderer') {
                                        installFeedNudgeFallback(renderer);
                                    }
                                    nestedFrag.appendChild(renderer);
                                    nestedCount++;
                                }
                            }
                        } catch (eN) {}
                        try {
                            box.setAttribute('data-aurora-rich-section-fallback', '1');
                            box.setAttribute('style',
                                'display:block;box-sizing:border-box;padding:16px 18px;border:3px solid #7c4dff;' +
                                'background:#ffffff;color:#111;font:700 20px/1.35 sans-serif;min-height:64px;' +
                                'margin:12px 0;width:100%;max-width:680px;');
                            box.textContent = 'aurora rich-section ' + describeContentValue(content) + (content.feedNudgeRenderer ? ' feedNudgeRenderer' : '');
                            if (nestedCount) {
                                try {
                                    box.appendChild(nestedFrag);
                                } catch (e5) {}
                            }
                        } catch (e3) {}
                        try {
                            if (document.body) {
                                document.body.insertBefore(box, document.body.firstChild);
                            } else {
                                section.appendChild(box);
                            }
                            if (globalThis.__aurora_debug_youtube__) trace('rich-section fallback appended ' + describeNode(section));
                        } catch (e4) {}
                        return true;
                    }

                    function schedule() {
                        if (attempts >= 12) return;
                        setTimeout(function() {
                            if (!render()) schedule();
                        }, attempts < 4 ? 0 : 50);
                    }

                    render();
                    schedule();
                }

                function maybeRenderFallback() {
                    var contents = resolveContents();
                    if (!contents || !contents.length) {
                        if (globalThis.__aurora_debug_youtube__) trace('rich-grid fallback waiting contents=' + (!!contents) + ' len=' + (contents && contents.length ? contents.length : 0));
                        scheduleRetry();
                        return false;
                    }
                    var container = resolveContainer();
                    if (!container) {
                        if (globalThis.__aurora_debug_youtube__) trace('rich-grid fallback waiting container');
                        scheduleRetry();
                        return false;
                    }
                    if (globalThis.__aurora_debug_youtube__) {
                        trace('rich-grid fallback container ' + describeNode(container));
                    }
                    try {
                        if (container.children && container.children.length) {
                            try {
                                for (var pi = 0; pi < container.children.length; pi++) {
                                    installRichSectionFallback(container.children[pi]);
                                }
                            } catch (e1) {}
                            if (globalThis.__aurora_debug_youtube__) trace('rich-grid fallback container already populated len=' + container.children.length);
                            return true;
                        }
                    } catch (e) {}
                    var frag = document.createDocumentFragment();
                    var summaryFrag = document.createDocumentFragment();
                    var sectionNodes = [];
                    var added = 0;
                    for (var i = 0; i < contents.length; i++) {
                        var renderer = createItemRenderer(contents[i]);
                        if (renderer) {
                            if (renderer.localName === 'ytd-rich-section-renderer') sectionNodes.push(renderer);
                            frag.appendChild(renderer);
                            added++;
                        }
                        if (globalThis.__aurora_debug_youtube__) {
                            var summaryNode = createSummaryNode(contents[i]);
                            if (summaryNode) summaryFrag.appendChild(summaryNode);
                        }
                    }
                    if (!added) {
                        if (globalThis.__aurora_debug_youtube__) trace('rich-grid fallback added=0');
                        scheduleRetry();
                        return false;
                    }
                    container.appendChild(frag);
                    try {
                        if (sectionNodes.length) {
                            for (var fi = 0; fi < sectionNodes.length; fi++) {
                                installRichSectionFallback(sectionNodes[fi]);
                            }
                        }
                    } catch (e0) {}
                    if (globalThis.__aurora_debug_youtube__ && summaryFrag.childNodes && summaryFrag.childNodes.length) {
                        try {
                            container.appendChild(summaryFrag);
                        } catch (e) {
                            traceError('rich-grid summary append', e);
                        }
                    }
                    if (globalThis.__aurora_debug_youtube__) {
                        trace('rich-grid fallback appended=' + added +
                            ' containerKids=' + (container.children ? container.children.length : 'n/a') +
                            ' containerChildNodes=' + (container.childNodes ? container.childNodes.length : 'n/a') +
                            ' firstChild=' + describeNode(container.firstChild));
                        Promise.resolve().then(function() {
                            try {
                                trace('rich-grid fallback post-microtask containerKids=' +
                                    (container.children ? container.children.length : 'n/a') +
                                    ' containerChildNodes=' + (container.childNodes ? container.childNodes.length : 'n/a') +
                                    ' firstChild=' + describeNode(container.firstChild));
                            } catch (e) {
                                traceError('rich-grid post-microtask', e);
                            }
                        });
                    }
                    try {
                        if (container.querySelector) {
                            var feedNodes = container.querySelectorAll('ytd-feed-nudge-renderer');
                            for (var fn = 0; fn < feedNodes.length; fn++) {
                                installFeedNudgeFallback(feedNodes[fn]);
                            }
                        }
                    } catch (eFeed2) {}
                    return true;
                }

                function scheduleRetry() {
                    if (fallbackTimer !== null) return;
                    if (fallbackAttempts >= 20) return;
                    fallbackAttempts++;
                    fallbackTimer = setTimeout(function() {
                        fallbackTimer = null;
                        if (!maybeRenderFallback()) scheduleRetry();
                    }, fallbackAttempts < 5 ? 0 : 50);
                }

                var originalDataChanged = typeof el.dataChanged === 'function' ? el.dataChanged : null;
                if (originalDataChanged && !originalDataChanged.__aurora_rich_grid_wrapped__) {
                    var wrappedDataChanged = function() {
                        var result = originalDataChanged.apply(this, arguments);
                        maybeRenderFallback();
                        return result;
                    };
                    try {
                        Object.defineProperty(wrappedDataChanged, '__aurora_rich_grid_wrapped__', {
                            value: true,
                            configurable: true
                        });
                    } catch (e) {
                        wrappedDataChanged.__aurora_rich_grid_wrapped__ = true;
                    }
                    try { el.dataChanged = wrappedDataChanged; } catch (e) {}
                }
                maybeRenderFallback();
                scheduleRetry();
            }

            function shouldReplayConstructor(ctor) {
                if (typeof ctor !== 'function') return false;
                var source = '';
                try {
                    source = Function.prototype.toString.call(ctor);
                } catch (e) {}
                return source.indexOf('class ') !== 0;
            }

            // Upgrade: swap the plain stub element's prototype to the
            // registered class/constructor's prototype and run it bound to
            // the element, then fire connectedCallback. This is exactly what
            // function-style definitions (`function MyEl(){...}`) expect.
            // ES6 `class X extends HTMLElement` constructors throw "class
            // constructor cannot be invoked without 'new'" when called this
            // way. Keep the prototype swap and still fire connectedCallback;
            // most framework element work happens there, and skipping it
            // leaves upgraded nodes inert.
            function tryUpgrade(el, connect) {
                if (!el || el.nodeType !== 1) return;
                var name = el.localName || (el.tagName ? el.tagName.toLowerCase() : '');
                var definition = getDefinition(name);
                if (!definition) return;
                var ctor = definition.ctor;
                if (!ctor) return;
                if (el.__ce_upgraded__) {
                    connectUpgraded(el, name, connect);
                    return;
                }
                el.__ce_upgraded__ = true;
                attachDefinitionMetadata(el, definition);
                if (shouldTraceName(name)) trace('upgrade ' + name + ' connect=' + (connect !== false));
                try {
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' proto-start');
                    }
                    Object.setPrototypeOf(el, ctor.prototype);
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' proto-done');
                    }
                    attachDefinitionMetadata(el, definition);
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' metadata-done');
                    }
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' beforeRegister-start');
                    }
                    invokeBeforeRegister(ctor, name);
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' beforeRegister-done');
                    }
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' ctor-start');
                    }
                    if (shouldReplayConstructor(ctor)) {
                        var hadObjectInitializeProperties = hasOwn.call(Object.prototype, '_initializeProperties');
                        var oldObjectInitializeProperties = Object.prototype._initializeProperties;
                        if (typeof el._initializeProperties !== 'function') {
                            try {
                                Object.defineProperty(el, '_initializeProperties', {
                                    value: function(){},
                                    configurable: true,
                                    writable: true
                                });
                            } catch (e) {
                                el._initializeProperties = function(){};
                            }
                        }
                        if (typeof Object.prototype._initializeProperties !== 'function') {
                            Object.defineProperty(Object.prototype, '_initializeProperties', {
                                value: function(){},
                                configurable: true,
                                writable: true
                            });
                        }
                        upgradeStack.push(el);
                        try {
                            ctor.call(el);
                        } finally {
                            upgradeStack.pop();
                            if (hadObjectInitializeProperties) {
                                Object.prototype._initializeProperties = oldObjectInitializeProperties;
                            } else {
                                delete Object.prototype._initializeProperties;
                            }
                        }
                    } else {
                        // Class-style constructor: replay using Reflect.construct under upgradeStack.
                        upgradeStack.push(el);
                        try {
                            Reflect.construct(ctor, []);
                        } finally {
                            upgradeStack.pop();
                        }
                    }
                    if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                        trace('upgrade-stage ' + name + ' ctor-done');
                    }
                } catch (e) {
                    traceError('constructor ' + name, e);
                }
                maybeCallCreated(el, name);
                installRichGridFallback(el);
                if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                    trace('upgrade-stage ' + name + ' connect-start');
                }
                connectUpgraded(el, name, connect);
                if (globalThis.__aurora_debug_youtube__ && debugProbeName(name)) {
                    trace('upgrade-stage ' + name + ' connect-done');
                }
                if (name === 'dom-module') {
                    registerDomModule(el);
                }
                probeCustomElementState(name, el, ctor);
            }

            function readyUpgraded(el, name) {
                if (el.__ce_ready__ || typeof el.ready !== 'function') return;
                el.__ce_ready__ = true;
                if (shouldTraceName(name)) trace('ready ' + name);
                installPolymerIdMapHooks(el);
                installInstanceTemplateIdAccessors(el, el.__aurora_ce_ctor__ || el.constructor);
                rebuildPolymerIdMap(el);
                el.ready();
                rebuildPolymerIdMap(el);
            }

            function connectUpgraded(el, name, connect) {
                if (connect === false) return;
                try {
                    if (el.__ce_connect_failed__) return;
                    if (!isActuallyConnected(el)) {
                        if (!el.__ce_connect_retry__) {
                            el.__ce_connect_retry__ = 1;
                            setTimeout(function() {
                                try {
                                    el.__ce_connect_retry__ = 0;
                                    connectUpgraded(el, name, true);
                                } catch (e) {}
                            }, 0);
                        } else if (el.__ce_connect_retry__ < 5) {
                            el.__ce_connect_retry__++;
                            setTimeout(function() {
                                try {
                                    connectUpgraded(el, name, true);
                                } catch (e) {}
                            }, el.__ce_connect_retry__ < 3 ? 0 : 50);
                        }
                        return;
                    }
                    readyUpgraded(el, name);
                    if (!el.__ce_connected__) {
                        installPolymerIdMapHooks(el);
                        rebuildPolymerIdMap(el);
                        installInstanceSetUpPropsHook(el);
                        normalizeAttributedStringProps(el);
                        if (shouldSuppressLifecycle(name)) {
                            if (shouldTraceName(name)) trace('suppress lifecycle ' + name);
                            el.__ce_connected__ = true;
                            rebuildPolymerIdMap(el);
                            return;
                        }
                        if (typeof el.connectedCallback === 'function') {
                            if (shouldTraceName(name)) trace('connectedCallback ' + name);
                            el.connectedCallback();
                        } else if (typeof el.attached === 'function') {
                            if (shouldTraceName(name)) trace('attached ' + name);
                            el.attached();
                        } else {
                            el.__ce_connected__ = true;
                            return;
                        }
                        el.__ce_connected__ = true;
                        rebuildPolymerIdMap(el);
                    }
                } catch (e) {
                    try {
                        el.__ce_connect_failed__ = true;
                    } catch (e0) {}
                    traceError('connectedCallback ' + name, e);
                    if (globalThis.__aurora_debug_youtube__ && name === 'ytd-app') {
                        try {
                            trace('post-error $ type=' + typeof el.$ + ' keys=' + (el.$ ? Object.keys(el.$).join(',') : 'n/a'));
                            var gv = el.$ && el.$.guide;
                            trace('post-error $.guide=' + (gv ? (gv.tagName || typeof gv) : String(gv)));
                            trace('post-error root===shadowRoot=' + (el.root === el.shadowRoot) +
                                ' root type=' + typeof el.root);
                            var sr = el.shadowRoot;
                            trace('post-error shadowRoot childNodes=' + (sr && sr.childNodes ? sr.childNodes.length : 'n/a'));
                            var g = sr && typeof sr.querySelector === 'function' ? sr.querySelector('#guide') : null;
                            trace('post-error shadowRoot.querySelector(#guide)=' + (g ? (g.tagName || 'found') : String(g)));
                            trace('post-error _template type=' + typeof el._template +
                                ' typeof _stampTemplate=' + typeof el._stampTemplate);
                        } catch (e2) { traceError('post-error probe', e2); }
                    }
                }
            }

            function rememberPending(el) {
                if (!el || el.nodeType !== 1) return;
                var name = el.localName || (el.tagName ? el.tagName.toLowerCase() : '');
                if (!shouldTrack(name)) return;
                if (getDefinition(name)) {
                    tryUpgrade(el, suppressTrackedConnect ? false : true);
                    return;
                }
                if (!pending[name]) pending[name] = [];
                pending[name].push(el);
            }

            function flushPending(name) {
                var list = pending[name];
                if (!list || !list.length) return;
                pending[name] = [];
                for (var i = 0; i < list.length; i++) {
                    tryUpgrade(list[i], true);
                }
            }

            function primeTree(root) {
                if (!root) return;
                rememberPending(root);
                if (typeof root.querySelectorAll === 'function') {
                    var all = root.querySelectorAll('*');
                    for (var i = 0; i < all.length; i++) { rememberPending(all[i]); }
                }
            }

            function upgradeTree(root) {
                if (!root) return;
                try {
                    primeTree(root);
                    if (typeof root.querySelectorAll === 'function') {
                        var all = root.querySelectorAll('*');
                        for (var i = 0; i < all.length; i++) { tryUpgrade(all[i], true); }
                    }
                } catch (e) {}
            }

            // Newly created elements (`document.createElement('ytd-app')`)
            // need upgrading too — patch it in lazily once `document` exists
            // (it doesn't yet at globals-install time).
            function ensureCreateElementPatch() {
                if (patchedCreateElement) return;
                if (typeof document === 'undefined' || typeof document.createElement !== 'function') return;
                patchedCreateElement = true;
                var orig = document.createElement.bind(document);
                originalCreateElement = orig;
                document.createElement = function(tagName, options) {
                    var el = orig(tagName, options);
                    if (String(tagName).indexOf('-') >= 0 && shouldTraceName(String(tagName))) trace('createElement ' + tagName);
                    rememberPending(el);
                    tryUpgrade(el, false);
                    return el;
                };
            }

            globalThis.customElements = {
                define: function(name, ctor, opts) {
                if (shouldTraceName(name)) trace('define ' + name);
                var definition = ensureDefinitionMetadata(name, ctor);
                attachDefinitionMetadata(ctor, definition);
                invokeBeforeRegister(ctor, name);
                if (name.indexOf('-') >= 0) {
                    installTemplateAccessor(name, ctor);
                }
                installSetUpPropsHook(ctor, name);
                probeCustomElementState(name, null, ctor);
                flushPending(name);
            },
                get: function(name) {
                    var definition = getDefinition(name);
                    return definition ? definition.ctor : undefined;
                },
                whenDefined: function(name) {
                    return getDefinition(name) ? Promise.resolve(getDefinition(name).ctor) : new Promise(function(res) {
                        var orig = customElements.define;
                        customElements.define = function(n, c, o) {
                            orig.call(customElements, n, c, o);
                            if (n === name) res(c);
                        };
                    });
                },
                upgrade: function(root) { trace('customElements.upgrade'); upgradeTree(root); },
                __aurora_track_custom_element__: function(el) { rememberPending(el); }
            };

            globalThis.__aurora_init_custom_elements__ = function() { ensureCreateElementPatch(); };
            globalThis.__aurora_track_custom_element__ = function(el) { rememberPending(el); };
        })();
