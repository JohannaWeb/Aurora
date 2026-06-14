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
            function traceError(where, error) {
                var message = error && (error.name || 'Error') + ': ' + (error.message || '');
                var stack = error && error.stack ? ('\n' + error.stack) : '';
                console.log('[yt-life] ERROR ' + where + ': ' + (message || String(error)) + stack);
            }

            function debugProbeName(name) {
                return name === 'ytd-app' || name === 'ytd-masthead';
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

            function registerDomModule(el) {
                var id = getElementId(el);
                if (!id) return null;
                var tpl = findTemplateForDomModule(el);
                if (!tpl) return null;
                domModules[id] = tpl;
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
                            if (template) return template;
                            var moduleId = definition.name || (this && this.is) || '';
                            if (moduleId && domModules[moduleId]) {
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
                if (!root || typeof root.querySelectorAll !== 'function') return;
                var map = el.$ && typeof el.$ === 'object' ? el.$ : {};
                var nodes = [];
                try {
                    var all = root.querySelectorAll('*');
                    for (var i = 0; i < all.length; i++) nodes.push(all[i]);
                } catch (e) {
                    return;
                }
                for (var n = 0; n < nodes.length; n++) {
                    var child = nodes[n];
                    var id = getElementId(child);
                    if (!id) continue;
                    map[id] = child;
                    var currentIdValue;
                    var shouldInstallDirectId = !(id in el);
                    if (!shouldInstallDirectId) {
                        try {
                            currentIdValue = el[id];
                            shouldInstallDirectId = currentIdValue == null;
                        } catch (e) {}
                    }
                    if (shouldInstallDirectId) {
                        try {
                            Object.defineProperty(el, id, {
                                configurable: true,
                                enumerable: false,
                                writable: true,
                                value: child
                            });
                        } catch (e) {
                            try { el[id] = child; } catch (e2) {}
                        }
                    }
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
                    Object.setPrototypeOf(el, ctor.prototype);
                    attachDefinitionMetadata(el, definition);
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
                } catch (e) {
                    traceError('constructor ' + name, e);
                }
                connectUpgraded(el, name, connect);
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
                rebuildPolymerIdMap(el);
                el.ready();
                rebuildPolymerIdMap(el);
            }

            function connectUpgraded(el, name, connect) {
                if (connect === false) return;
                try {
                    readyUpgraded(el, name);
                    if (!el.__ce_connected__) {
                        if (typeof el.connectedCallback !== 'function') return;
                        el.__ce_connected__ = true;
                        if (shouldTraceName(name)) trace('connectedCallback ' + name);
                        installPolymerIdMapHooks(el);
                        rebuildPolymerIdMap(el);
                        installInstanceSetUpPropsHook(el);
                        normalizeAttributedStringProps(el);
                        el.connectedCallback();
                        rebuildPolymerIdMap(el);
                    }
                } catch (e) {
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
