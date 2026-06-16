(function() {
    // ── Path utilities ─────────────────────────────────────────────────────
    function getPath(obj, path) {
        var segs = path.split('.');
        for (var i = 0; i < segs.length && obj != null; i++) {
            try { obj = obj[segs[i]]; } catch (e) { return undefined; }
        }
        return obj;
    }
    function setPath(obj, path, value) {
        var segs = path.split('.');
        for (var i = 0; i < segs.length - 1 && obj != null; i++) {
            try { obj = obj[segs[i]]; } catch (e) { return; }
        }
        if (obj != null) try { obj[segs[segs.length - 1]] = value; } catch (e) {}
    }

    // ── Polymer element methods ────────────────────────────────────────────
    // Installed on prototype at define() time and as instance fallbacks at
    // upgrade time. Only fills gaps — real Polymer bundle methods take priority.
    var POLYMER_PROTO = {
        fire: function(name, detail, opts) {
            opts = opts || {};
            var ev = new CustomEvent(name, {
                detail: detail !== undefined ? detail : {},
                bubbles: opts.bubbles !== false,
                composed: opts.composed !== false,
                cancelable: opts.cancelable !== false
            });
            try { (opts.node || this).dispatchEvent(ev); } catch (e) {}
            return ev;
        },
        $$: function(sel) {
            var root;
            try { root = this.root || this.shadowRoot || this.__shady_shadowRoot || this; } catch (e) { root = this; }
            try { return root.querySelector(sel); } catch (e) { return null; }
        },
        async: function(fn, delay) {
            var self = this;
            return setTimeout(function() { try { fn.call(self); } catch (e) {} }, delay || 0);
        },
        cancelAsync: function(handle) { clearTimeout(handle); },
        debounce: function(job, fn, wait) {
            if (!this.__debouncers) this.__debouncers = {};
            var d = this.__debouncers;
            if (d[job]) clearTimeout(d[job]._t);
            var self = this;
            d[job] = { _t: setTimeout(function() { delete d[job]; try { fn.call(self); } catch (e) {} }, wait || 0), fn: fn };
        },
        cancelDebouncer: function(job) {
            if (this.__debouncers && this.__debouncers[job]) {
                clearTimeout(this.__debouncers[job]._t);
                delete this.__debouncers[job];
            }
        },
        isDebouncerActive: function(job) { return !!(this.__debouncers && this.__debouncers[job]); },
        flushDebouncer: function(job) {
            if (!this.__debouncers || !this.__debouncers[job]) return;
            var d = this.__debouncers[job];
            clearTimeout(d._t); delete this.__debouncers[job];
            try { d.fn.call(this); } catch (e) {}
        },
        set: function(path, value) {
            if (this.__data) setPath(this.__data, path, value);
            setPath(this, path, value);
            this.notifyPath(path, value);
        },
        get: function(path) {
            if (this.__data) { var v = getPath(this.__data, path); if (v !== undefined) return v; }
            return getPath(this, path);
        },
        notifyPath: function(path, value) {
            if (typeof this._propertiesChanged === 'function') {
                var ch = {}; ch[path] = value !== undefined ? value : getPath(this, path);
                try { this._propertiesChanged(this.__data || {}, ch, {}); } catch (e) {}
            }
            if (this.__aurora_bindings__ && typeof globalThis.__aurora_apply_stamped_bindings__ === 'function') {
                try { globalThis.__aurora_apply_stamped_bindings__(this, this.__aurora_bindings__); } catch (e) {}
            }
        },
        linkPaths: function(to, from) { if (!this.__linkedPaths) this.__linkedPaths = {}; this.__linkedPaths[to] = from; },
        unlinkPaths: function(path) { if (this.__linkedPaths) delete this.__linkedPaths[path]; },
        push: function(path) {
            var arr = this.get(path); if (!Array.isArray(arr)) return 0;
            var len = arr.push.apply(arr, Array.prototype.slice.call(arguments, 1));
            this.notifyPath(path + '.length', len); return len;
        },
        pop: function(path) {
            var arr = this.get(path); if (!Array.isArray(arr)) return undefined;
            var item = arr.pop(); this.notifyPath(path + '.length', arr.length); return item;
        },
        splice: function(path, start, deleteCount) {
            var arr = this.get(path); if (!Array.isArray(arr)) return [];
            var removed = arr.splice.apply(arr, [start, deleteCount].concat(Array.prototype.slice.call(arguments, 3)));
            this.notifyPath(path + '.length', arr.length); return removed;
        },
        shift: function(path) {
            var arr = this.get(path); if (!Array.isArray(arr)) return undefined;
            var item = arr.shift(); this.notifyPath(path + '.length', arr.length); return item;
        },
        unshift: function(path) {
            var arr = this.get(path); if (!Array.isArray(arr)) return 0;
            var len = arr.unshift.apply(arr, Array.prototype.slice.call(arguments, 1));
            this.notifyPath(path + '.length', len); return len;
        },
        toggleClass: function(name, bool, node) {
            node = node || this;
            if (bool === undefined) try { bool = !node.classList.contains(name); } catch (e) { bool = true; }
            try { node.classList.toggle(name, !!bool); } catch (e) {
                try { if (bool) node.setAttribute('class', ((node.getAttribute('class') || '') + ' ' + name).trim());
                      else node.removeAttribute('class'); } catch (e2) {}
            }
        },
        toggleAttribute: function(name, bool, node) {
            node = node || this;
            if (bool === undefined) try { bool = !node.hasAttribute(name); } catch (e) { bool = true; }
            try { if (bool) node.setAttribute(name, ''); else node.removeAttribute(name); } catch (e) {}
        },
        listen: function(node, eventName, methodName) {
            if (!this.__listeners) this.__listeners = [];
            var self = this;
            var fn = function(e) { try { self[methodName](e); } catch (ex) {} };
            this.__listeners.push({ node: node, eventName: eventName, fn: fn, methodName: methodName });
            try { node.addEventListener(eventName, fn); } catch (e) {}
        },
        unlisten: function(node, eventName, methodName) {
            if (!this.__listeners) return;
            for (var i = 0; i < this.__listeners.length; i++) {
                var l = this.__listeners[i];
                if (l.node === node && l.eventName === eventName && l.methodName === methodName) {
                    try { node.removeEventListener(eventName, l.fn); } catch (e) {}
                    this.__listeners.splice(i, 1); return;
                }
            }
        },
        resolveUrl: function(url) {
            try { return new URL(url, (globalThis.location && globalThis.location.href) || 'https://www.youtube.com/').href; }
            catch (e) { return url; }
        },
        importHref: function(href, onload) {
            if (typeof onload === 'function') queueMicrotask(function() { try { onload({}); } catch (e) {} });
        },
        translate: function(msgid) { return msgid || ''; },
        setScrollDirection: function() {},
        getNormalizedEventForType: function() { return null; },
        create: function(tag, props) {
            var el = document.createElement(tag);
            if (props) for (var k in props) try { el[k] = props[k]; } catch (e) {}
            return el;
        }
    };

    function installPolymerMethods(target) {
        for (var key in POLYMER_PROTO) {
            try {
                if (target[key] == null || typeof target[key] !== 'function')
                    target[key] = POLYMER_PROTO[key];
            } catch (e) {}
        }
    }
    globalThis.__aurora_install_polymer_methods__ = installPolymerMethods;

    // ── dom-repeat ─────────────────────────────────────────────────────────
    var BINDING_EXPR = /\[\[([^\]]+)\]\]|\{\{([^}]+)\}\}/g;

    function resolveExpr(expr, item, as, indexAs, index, host) {
        expr = expr.trim();
        if (indexAs && expr === indexAs) return index;
        if (expr === as) return item;
        var prefix = as + '.';
        if (expr.indexOf(prefix) === 0) return getPath(item, expr.slice(prefix.length));
        var v = getPath(item, expr);
        if (v === undefined && host) v = getPath(host.__data || host, expr);
        return v;
    }

    function substituteBindings(str, item, as, indexAs, index, host) {
        return str.replace(BINDING_EXPR, function(m, p1, p2) {
            var val = resolveExpr((p1 || p2), item, as, indexAs, index, host);
            return val == null ? '' : String(val);
        });
    }

    function stampItem(tpl, item, index, as, indexAs, host) {
        var content = tpl && tpl.content;
        if (!content) return null;
        var clone; try { clone = content.cloneNode(true); } catch (e) { return null; }
        function walkBind(node) {
            var nt; try { nt = node.nodeType; } catch (e) { return; }
            if (nt === 3) {
                var t; try { t = node.textContent || ''; } catch (e) { return; }
                if (t.indexOf('[[') >= 0 || t.indexOf('{{') >= 0)
                    node.textContent = substituteBindings(t, item, as, indexAs, index, host);
            } else if (nt === 1) {
                var attrs; try { attrs = node.attributes; } catch (e) {}
                if (attrs) {
                    for (var ai = 0; ai < attrs.length; ai++) {
                        try {
                            var av = attrs[ai].value;
                            if (av.indexOf('[[') >= 0 || av.indexOf('{{') >= 0)
                                node.setAttribute(attrs[ai].name, substituteBindings(av, item, as, indexAs, index, host));
                        } catch (e) {}
                    }
                }
                // Assign data directly on child custom elements
                var tag; try { tag = node.localName || ''; } catch (e) {}
                if (tag.indexOf('-') >= 0) {
                    try { if (node.data == null) node.data = item; } catch (e) {}
                }
                var child; try { child = node.firstChild; } catch (e) {}
                while (child) { walkBind(child); try { child = child.nextSibling; } catch (e) { break; } }
            }
        }
        var c; try { c = clone.firstChild; } catch (e) {}
        while (c) { walkBind(c); try { c = c.nextSibling; } catch (e) { break; } }
        return clone;
    }

    function DomRepeat() {
        this.__items = []; this.__as = 'item'; this.__indexAs = null;
        this.__stamped = []; this.__tpl = null; this.__queued = false;
    }
    DomRepeat.prototype = Object.create(HTMLElement.prototype);
    DomRepeat.prototype.constructor = DomRepeat;
    DomRepeat.prototype._findHost = function() {
        var p; try { p = this.parentNode; } catch (e) {}
        while (p && p.nodeType !== 9) {
            if (p.__aurora_ce_definition__) return p;
            try { p = p.parentNode; } catch (e) { break; }
        }
        return null;
    };
    DomRepeat.prototype._render = function() {
        if (!this.__tpl) try { this.__tpl = this.querySelector && this.querySelector('template'); } catch (e) {}
        if (!this.__tpl || !this.__tpl.content) return;
        var parent; try { parent = this.parentNode; } catch (e) {}
        if (!parent) return;
        for (var ri = 0; ri < this.__stamped.length; ri++) try { parent.removeChild(this.__stamped[ri]); } catch (e) {}
        this.__stamped = [];
        var as = this.__as, indexAs = this.__indexAs, host = this._findHost();
        try { as = this.getAttribute('as') || as; } catch (e) {}
        try { indexAs = this.getAttribute('index-as') || indexAs; } catch (e) {}
        var items = this.__items || [], ns; try { ns = this.nextSibling; } catch (e) {}
        for (var i = 0; i < items.length; i++) {
            var frag = stampItem(this.__tpl, items[i], i, as, indexAs, host);
            if (!frag) continue;
            var nodes = []; var c; try { c = frag.firstChild; } catch (e) {}
            while (c) { nodes.push(c); try { c = c.nextSibling; } catch (e) { break; } }
            try { if (ns) parent.insertBefore(frag, ns); else parent.appendChild(frag); } catch (e) {}
            for (var ni = 0; ni < nodes.length; ni++) {
                this.__stamped.push(nodes[ni]);
                try {
                    if (nodes[ni].nodeType === 1 && customElements.__aurora_track_custom_element__) {
                        customElements.__aurora_track_custom_element__(nodes[ni]);
                        var desc = nodes[ni].querySelectorAll && nodes[ni].querySelectorAll('*');
                        if (desc) for (var di = 0; di < desc.length; di++) customElements.__aurora_track_custom_element__(desc[di]);
                    }
                } catch (e) {}
            }
        }
    };
    DomRepeat.prototype.render = function() { this._render(); };
    DomRepeat.prototype._scheduleRender = function() {
        if (this.__queued) return; this.__queued = true;
        var self = this; queueMicrotask(function() { self.__queued = false; self._render(); });
    };
    DomRepeat.prototype.connectedCallback = DomRepeat.prototype.attached = function() {
        try { this.__tpl = this.querySelector && this.querySelector('template'); } catch (e) {}
        this._render();
    };
    Object.defineProperty(DomRepeat.prototype, 'items', {
        get: function() { return this.__items; },
        set: function(v) { this.__items = Array.isArray(v) ? v : (v == null ? [] : []); this._scheduleRender(); },
        configurable: true
    });
    customElements.define('dom-repeat', DomRepeat);

    // ── dom-if ─────────────────────────────────────────────────────────────
    function DomIf() { this.__if = false; this.__stamped = false; this.__nodes = []; this.__tpl = null; }
    DomIf.prototype = Object.create(HTMLElement.prototype);
    DomIf.prototype.constructor = DomIf;
    DomIf.prototype._apply = function() {
        if (!this.__tpl) try { this.__tpl = this.querySelector && this.querySelector('template'); } catch (e) {}
        if (!this.__tpl || !this.__tpl.content) return;
        var parent; try { parent = this.parentNode; } catch (e) {}
        if (!parent) return;
        if (this.__if) {
            if (this.__stamped) return;
            var frag; try { frag = this.__tpl.content.cloneNode(true); } catch (e) { return; }
            var nodes = []; var c; try { c = frag.firstChild; } catch (e) {}
            while (c) { nodes.push(c); try { c = c.nextSibling; } catch (e) { break; } }
            var ns; try { ns = this.nextSibling; } catch (e) {}
            try { if (ns) parent.insertBefore(frag, ns); else parent.appendChild(frag); } catch (e) {}
            this.__nodes = nodes; this.__stamped = true;
            for (var i = 0; i < nodes.length; i++) {
                try {
                    if (nodes[i].nodeType === 1 && customElements.__aurora_track_custom_element__) {
                        customElements.__aurora_track_custom_element__(nodes[i]);
                        var desc = nodes[i].querySelectorAll && nodes[i].querySelectorAll('*');
                        if (desc) for (var di = 0; di < desc.length; di++) customElements.__aurora_track_custom_element__(desc[di]);
                    }
                } catch (e) {}
            }
        } else {
            for (var ri = 0; ri < this.__nodes.length; ri++) try { parent.removeChild(this.__nodes[ri]); } catch (e) {}
            this.__nodes = []; this.__stamped = false;
        }
    };
    DomIf.prototype.connectedCallback = DomIf.prototype.attached = function() {
        try { this.__tpl = this.querySelector && this.querySelector('template'); } catch (e) {}
        this._apply();
    };
    Object.defineProperty(DomIf.prototype, 'if', {
        get: function() { return this.__if; },
        set: function(v) { this.__if = !!v; var self = this; queueMicrotask(function() { self._apply(); }); },
        configurable: true
    });
    customElements.define('dom-if', DomIf);

    // ── Polymer.dom() ──────────────────────────────────────────────────────
    function PDom(node) { this.node = node; }
    ['appendChild','insertBefore','removeChild','replaceChild','querySelector',
     'querySelectorAll','setAttribute','removeAttribute','getAttribute','cloneNode','contains'
    ].forEach(function(m) {
        PDom.prototype[m] = function() {
            var fn = this.node && this.node[m];
            return typeof fn === 'function' ? fn.apply(this.node, arguments) : undefined;
        };
    });
    ['parentNode','firstChild','lastChild','childNodes','children','classList','textContent'
    ].forEach(function(p) {
        Object.defineProperty(PDom.prototype, p, {
            get: function() { return this.node && this.node[p]; },
            set: function(v) { if (this.node) this.node[p] = v; },
            configurable: true
        });
    });
    PDom.prototype.getEffectiveChildNodes = function() {
        var nodes = [], c; try { c = this.node && this.node.firstChild; } catch (e) {}
        while (c) { nodes.push(c); try { c = c.nextSibling; } catch (e) { break; } }
        return nodes;
    };
    PDom.prototype.getEffectiveChildren = function() {
        return this.getEffectiveChildNodes().filter(function(n) { return n.nodeType === 1; });
    };
    PDom.prototype.getDistributedNodes = PDom.prototype.getEffectiveChildNodes;
    PDom.prototype.getDestinationInsertionPoints = function() { return []; };
    PDom.prototype.observeNodes = function(cb) {
        if (!this.node || typeof cb !== 'function') return null;
        try {
            var obs = new MutationObserver(function(recs) {
                cb({ addedNodes: recs.reduce(function(a, r) { return a.concat(Array.prototype.slice.call(r.addedNodes)); }, []),
                     removedNodes: recs.reduce(function(a, r) { return a.concat(Array.prototype.slice.call(r.removedNodes)); }, []) });
            });
            obs.observe(this.node, { childList: true });
            return obs;
        } catch (e) { return null; }
    };
    PDom.prototype.unobserveNodes = function(obs) { if (obs && obs.disconnect) obs.disconnect(); };

    globalThis.Polymer = globalThis.Polymer || {};
    if (!globalThis.Polymer.dom || !globalThis.Polymer.dom.__aurora__) {
        globalThis.Polymer.dom = function(node) { return new PDom(node); };
        globalThis.Polymer.dom.__aurora__ = true;
        globalThis.Polymer.dom.flush = function() {};
    }

    // ── Polymer global stubs ───────────────────────────────────────────────
    globalThis.Polymer.telemetry = globalThis.Polymer.telemetry || { instanceCount: 0, registrations: [], trackAlloc: function(){}, trackFree: function(){} };
    globalThis.Polymer.Settings = globalThis.Polymer.Settings || { useNativeCustomElements: true, useNativeShadow: false };
    globalThis.Polymer.RenderStatus = globalThis.Polymer.RenderStatus || {
        afterNextRender: function(ctx, fn, args) { queueMicrotask(function() { try { fn.apply(ctx, args||[]); } catch(e){} }); },
        beforeNextRender: function(ctx, fn, args) { queueMicrotask(function() { try { fn.apply(ctx, args||[]); } catch(e){} }); },
        whenReady: function(fn) { queueMicrotask(function() { try { fn(); } catch(e){} }); }
    };
    globalThis.Polymer.Async = globalThis.Polymer.Async || {
        microTask: { run: function(fn) { queueMicrotask(fn); }, cancel: function(){} },
        timeOut: { run: function(fn, d) { return setTimeout(fn, d||0); }, cancel: clearTimeout },
        animationFrame: { run: function(fn) { return (globalThis.requestAnimationFrame||setTimeout)(fn); }, cancel: function(h) { (globalThis.cancelAnimationFrame||clearTimeout)(h); } },
        idlePeriod: { run: function(fn) { return (globalThis.requestIdleCallback||setTimeout)(fn); }, cancel: function(h) { (globalThis.cancelIdleCallback||clearTimeout)(h); } }
    };

    // Polymer({is:'name', ...}) factory — stub until YouTube's real bundle loads.
    if (typeof globalThis.Polymer !== 'function') {
        var _polymerStubs = globalThis.Polymer;
        globalThis.Polymer = function(def) {
            if (!def || !def.is) return;
            var name = def.is;
            var ctor = function() { HTMLElement.call(this); };
            ctor.prototype = Object.create(HTMLElement.prototype);
            ctor.prototype.constructor = ctor;
            for (var k in def) { if (k !== 'is' && k !== 'properties') ctor.prototype[k] = def[k]; }
            installPolymerMethods(ctor.prototype);
            customElements.define(name, ctor);
            return ctor;
        };
        for (var _k in _polymerStubs) globalThis.Polymer[_k] = _polymerStubs[_k];
    }

    // ── Polymer base classes ───────────────────────────────────────────────
    // Stubs so `class Foo extends Polymer.Element` doesn't throw before
    // YouTube's own bundle defines the real implementations.
    if (!globalThis.Polymer.Element) {
        globalThis.Polymer.Element = function PolymerElement() { HTMLElement.call(this); };
        globalThis.Polymer.Element.prototype = Object.create(HTMLElement.prototype);
        globalThis.Polymer.Element.prototype.constructor = globalThis.Polymer.Element;
        installPolymerMethods(globalThis.Polymer.Element.prototype);
    }
    if (!globalThis.Polymer.LegacyElementMixin) {
        globalThis.Polymer.LegacyElementMixin = function(Base) {
            var Mixin = function() { Base.call(this); };
            Mixin.prototype = Object.create(Base.prototype);
            Mixin.prototype.constructor = Mixin;
            installPolymerMethods(Mixin.prototype);
            return Mixin;
        };
    }
    if (!globalThis.Polymer.mixinBehaviors) {
        globalThis.Polymer.mixinBehaviors = function(behaviors, Base) {
            var proto = Base.prototype;
            (Array.isArray(behaviors) ? behaviors : [behaviors]).forEach(function(b) {
                if (!b || typeof b !== 'object') return;
                for (var k in b) try { if (!proto[k]) proto[k] = b[k]; } catch (e) {}
            });
            return Base;
        };
    }
    if (!globalThis.Polymer.dedupingMixin) {
        globalThis.Polymer.dedupingMixin = function(fn) { return fn; };
    }

    // ── Install methods at define() time ───────────────────────────────────
    // Wrap customElements.define so every registration gets the utility methods
    // on its prototype before any elements are upgraded.
    var _origDef = globalThis.customElements.define;
    if (_origDef && !_origDef.__aurora_polymer__) {
        globalThis.customElements.define = function(name, ctor, opts) {
            if (ctor && ctor.prototype) installPolymerMethods(ctor.prototype);
            return _origDef.call(this, name, ctor, opts);
        };
        try { Object.defineProperty(globalThis.customElements.define, '__aurora_polymer__', { value: true, configurable: true }); } catch (e) {}
        globalThis.customElements.define.__aurora_polymer__ = true;
    }
})();
