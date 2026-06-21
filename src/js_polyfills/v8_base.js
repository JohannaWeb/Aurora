
// V8 environment shims, run before the shared polyfills
// (event_constructors.js / custom_elements.js need HTMLElement,
// queueMicrotask, etc. to exist).
(function() {
    if (typeof globalThis.queueMicrotask !== 'function') {
        globalThis.queueMicrotask = function(fn) { Promise.resolve().then(fn); };
    }

    // DOM constructor skeletons. Aurora's V8 node wrappers are plain objects,
    // so these exist for prototype-chain patching (Polymer's ES5 adapter wraps
    // HTMLElement; webcomponents probes Element.prototype) rather than as the
    // wrappers' actual prototypes.
    // Real EventTarget: listeners are stored per-object in `__ael`, and
    // `dispatchEvent` runs capture/target/bubble phases over the live DOM path
    // (walked via `parentNode`, extended to `document` and `window`). This is the
    // event model Polymer/ShadyDOM compose with; the previous stubs meant nothing
    // dispatched on a node ever reached its listeners.
    globalThis.EventTarget = function EventTarget() {};
    EventTarget.prototype.addEventListener = function(type, listener, options) {
        if (!type || typeof listener !== 'function' && (!listener || typeof listener.handleEvent !== 'function')) return;
        var capture = options === true || (options && options.capture) || false;
        if (!this.__ael) {
            try { Object.defineProperty(this, '__ael', { value: Object.create(null), writable: true, configurable: true }); }
            catch (e) { this.__ael = Object.create(null); }
        }
        var list = this.__ael[type] || (this.__ael[type] = []);
        for (var i = 0; i < list.length; i++) {
            if (list[i].listener === listener && list[i].capture === capture) return;
        }
        list.push({ listener: listener, capture: capture, once: !!(options && options.once) });
    };
    EventTarget.prototype.removeEventListener = function(type, listener, options) {
        if (!this.__ael) return;
        var list = this.__ael[type];
        if (!list) return;
        var capture = options === true || (options && options.capture) || false;
        for (var i = 0; i < list.length; i++) {
            if (list[i].listener === listener && list[i].capture === capture) { list.splice(i, 1); return; }
        }
    };
    function invokeListeners(node, event, capturePhase) {
        var store = node.__ael;
        if (!store) return;
        var list = store[event.type];
        if (!list || !list.length) return;
        event.currentTarget = node;
        var snapshot = list.slice();
        for (var i = 0; i < snapshot.length; i++) {
            var entry = snapshot[i];
            if (entry.capture !== capturePhase) continue;
            if (entry.once) node.removeEventListener(event.type, entry.listener, entry.capture);
            var fn = typeof entry.listener === 'function' ? entry.listener : entry.listener.handleEvent;
            try { fn.call(node, event); } catch (e) { setTimeout(function(){ throw e; }, 0); }
            if (event.__immediateStop) return;
        }
    }
    EventTarget.prototype.dispatchEvent = function(event) {
        if (!event) return true;
        // Build the propagation path: target up through ancestors, crossing a
        // shadow boundary through ShadowRoot.host for composed events, then
        // document and window (the two globals where delegated listeners live).
        var path = [];
        var n = this;
        var guard = 0;
        while (n && guard++ < 8192) {
            path.push(n);
            var parent = n.parentNode || null;
            if (!parent && event.composed && n.nodeType === 11 && n.host) parent = n.host;
            n = parent;
        }
        if (typeof document !== 'undefined' && path.indexOf(document) < 0) path.push(document);
        if (typeof window !== 'undefined' && path.indexOf(window) < 0) path.push(window);

        event.target = this;
        event.composedPath = function() { return path.slice(); };
        event.__immediateStop = false;
        if (event.cancelBubble === undefined) event.cancelBubble = false;
        // Capture phase: root -> target.
        for (var i = path.length - 1; i >= 1; i--) {
            invokeListeners(path[i], event, true);
            if (event.cancelBubble || event.__immediateStop) break;
        }
        // Target phase.
        if (!event.cancelBubble && !event.__immediateStop) {
            invokeListeners(path[0], event, true);
            invokeListeners(path[0], event, false);
        }
        // Bubble phase: target+1 -> root (only if the event bubbles).
        if (event.bubbles) {
            for (var j = 1; j < path.length; j++) {
                if (event.cancelBubble || event.__immediateStop) break;
                invokeListeners(path[j], event, false);
            }
        }
        event.currentTarget = null;
        return !event.defaultPrevented;
    };

    globalThis.Node = function Node() {};
    Node.prototype = Object.create(EventTarget.prototype);
    Node.prototype.constructor = Node;
    Node.ELEMENT_NODE = 1; Node.ATTRIBUTE_NODE = 2; Node.TEXT_NODE = 3;
    Node.CDATA_SECTION_NODE = 4; Node.COMMENT_NODE = 8;
    Node.DOCUMENT_NODE = 9; Node.DOCUMENT_TYPE_NODE = 10;
    Node.DOCUMENT_FRAGMENT_NODE = 11;

    globalThis.Element = function Element() {};
    Element.prototype = Object.create(Node.prototype);
    Element.prototype.constructor = Element;
    Object.defineProperty(Element.prototype, 'style', {
        get: function() {
            if (!this.__aurora_proto_style__) {
                this.__aurora_proto_style__ = createFallbackStyleDeclaration();
            }
            return this.__aurora_proto_style__;
        },
        set: function(value) {
            this.__aurora_proto_style__ = normalizeFallbackStyle(value);
        },
        configurable: true
    });

    globalThis.HTMLElement = function HTMLElement() { return undefined; };
    HTMLElement.prototype = Object.create(Element.prototype);
    HTMLElement.prototype.constructor = HTMLElement;

    [
        'HTMLDivElement','HTMLSpanElement','HTMLAnchorElement','HTMLImageElement',
        'HTMLInputElement','HTMLButtonElement','HTMLTemplateElement','HTMLScriptElement',
        'HTMLStyleElement','HTMLIFrameElement','HTMLVideoElement','HTMLAudioElement',
        'HTMLMediaElement','HTMLCanvasElement','HTMLFormElement','HTMLSelectElement',
        'HTMLTextAreaElement','HTMLLinkElement','HTMLMetaElement','HTMLTitleElement',
        'HTMLBodyElement','HTMLHtmlElement','HTMLHeadElement','HTMLUnknownElement',
        'HTMLLabelElement','HTMLUListElement','HTMLLIElement','HTMLParagraphElement'
    ].forEach(function(name) {
        var Ctor = function() {};
        Ctor.prototype = Object.create(HTMLElement.prototype);
        Ctor.prototype.constructor = Ctor;
        globalThis[name] = Ctor;
    });

    globalThis.SVGElement = function SVGElement() {};
    SVGElement.prototype = Object.create(Element.prototype);
    globalThis.Document = function Document() {};
    Document.prototype = Object.create(Node.prototype);
    globalThis.HTMLDocument = function HTMLDocument() {};
    HTMLDocument.prototype = Object.create(Document.prototype);
    globalThis.DocumentFragment = function DocumentFragment() {};
    DocumentFragment.prototype = Object.create(Node.prototype);
    globalThis.ShadowRoot = function ShadowRoot() {};
    ShadowRoot.prototype = Object.create(DocumentFragment.prototype);
    globalThis.CharacterData = function CharacterData() {};
    CharacterData.prototype = Object.create(Node.prototype);
    globalThis.Text = function Text() {};
    Text.prototype = Object.create(CharacterData.prototype);
    globalThis.Comment = function Comment() {};
    Comment.prototype = Object.create(CharacterData.prototype);
    globalThis.CDATASection = function CDATASection() {};
    CDATASection.prototype = Object.create(CharacterData.prototype);
    globalThis.ProcessingInstruction = function ProcessingInstruction() {};
    ProcessingInstruction.prototype = Object.create(CharacterData.prototype);
    globalThis.DocumentType = function DocumentType() {};
    DocumentType.prototype = Object.create(Node.prototype);
    globalThis.Attr = function Attr() {};
    Attr.prototype = Object.create(Node.prototype);
    globalThis.Window = function Window() {};
    globalThis.NodeList = function NodeList() {};
    globalThis.HTMLCollection = function HTMLCollection() {};
    globalThis.NamedNodeMap = function NamedNodeMap() {};
    globalThis.DOMTokenList = function DOMTokenList() {};
    globalThis.MutationRecord = function MutationRecord() {};
    globalThis.Range = function Range() {};
    globalThis.Selection = function Selection() {};
    globalThis.CSSStyleDeclaration = function CSSStyleDeclaration() {};
    globalThis.StyleSheetList = function StyleSheetList() {
        this.length = 0;
    };
    StyleSheetList.prototype.item = function(index) { return this[index] || null; };

    globalThis.CSSStyleSheet = function CSSStyleSheet() {
        this.disabled = false;
        this.href = null;
        this.media = { mediaText: '', appendMedium: function(){}, deleteMedium: function(){} };
        this.ownerNode = null;
        this.parentStyleSheet = null;
        this.title = null;
        this.type = 'text/css';
        this.cssRules = [];
    };
    CSSStyleSheet.prototype.insertRule = function(rule, index) { return index || 0; };
    CSSStyleSheet.prototype.deleteRule = function(index) {};

    // MutationObserver is implemented natively (see js_v8/mutation_observer.rs);
    // don't override it here. IntersectionObserver and ResizeObserver are
    // optimistic stubs that fire once to unblock content waiting for visibility/size.
    globalThis.IntersectionObserver = function IntersectionObserver(cb) {
        this._cb = cb;
    };
    IntersectionObserver.prototype.observe = function(el) {
        if (typeof this._cb === 'function') {
            var self = this;
            queueMicrotask(function() {
                if (typeof self._cb === 'function') {
                    try {
                        self._cb([{
                            target: el,
                            isIntersecting: true,
                            intersectionRatio: 1.0,
                            boundingClientRect: (typeof el.getBoundingClientRect === 'function')
                                ? el.getBoundingClientRect()
                                : { top: 0, left: 0, width: 1440, height: 1024, bottom: 1024, right: 1440, x: 0, y: 0 },
                            intersectionRect: { top: 0, left: 0, width: 1440, height: 1024, bottom: 1024, right: 1440, x: 0, y: 0 },
                            rootBounds: { top: 0, left: 0, width: 1440, height: 1024, bottom: 1024, right: 1440, x: 0, y: 0 }
                        }], self);
                    } catch (e) {}
                }
            });
        }
    };
    IntersectionObserver.prototype.unobserve = function() {};
    IntersectionObserver.prototype.disconnect = function() { this._cb = null; };
    IntersectionObserver.prototype.takeRecords = function() { return []; };
    globalThis.IntersectionObserverEntry = function IntersectionObserverEntry() {};

    globalThis.ResizeObserver = function ResizeObserver(cb) {
        this._cb = cb;
    };
    ResizeObserver.prototype.observe = function(el) {
        if (typeof this._cb === 'function') {
            var self = this;
            queueMicrotask(function() {
                if (typeof self._cb === 'function') {
                    try {
                        self._cb([{
                            target: el,
                            contentRect: (typeof el.getBoundingClientRect === 'function')
                                ? el.getBoundingClientRect()
                                : { top: 0, left: 0, width: 1440, height: 1024, bottom: 1024, right: 1440, x: 0, y: 0 }
                        }], self);
                    } catch (e) {}
                }
            });
        }
    };
    ResizeObserver.prototype.unobserve = function() {};
    ResizeObserver.prototype.disconnect = function() { this._cb = null; };

    globalThis.PerformanceObserver = function PerformanceObserver(cb) { this._cb = cb; };
    PerformanceObserver.prototype.observe = function() {};
    PerformanceObserver.prototype.disconnect = function() {};
    PerformanceObserver.prototype.takeRecords = function() { return []; };
    PerformanceObserver.supportedEntryTypes = [];

    globalThis.ReportingObserver = function ReportingObserver(cb) { this._cb = cb; };
    ReportingObserver.prototype.observe = function() {};
    ReportingObserver.prototype.disconnect = function() {};
    ReportingObserver.prototype.takeRecords = function() { return []; };

    globalThis.requestIdleCallback = function(cb) {
        return setTimeout(function() {
            cb({ didTimeout: false, timeRemaining: function() { return 50; } });
        }, 1);
    };
    globalThis.cancelIdleCallback = function(id) { clearTimeout(id); };

    if (typeof WeakMap === 'function') {
        (function(NativeWeakMap) {
            var primitiveBoxes = {};
            function boxWeakKey(key) {
                if ((typeof key === 'object' && key !== null) || typeof key === 'function') {
                    return key;
                }
                var type = typeof key;
                var id = type + ':' + String(key);
                return primitiveBoxes[id] || (primitiveBoxes[id] = { value: key });
            }
            globalThis.WeakMap = function WeakMap(iterable) {
                var map = new NativeWeakMap();
                Object.defineProperty(this, '__aurora_native_weakmap__', { value: map });
                if (iterable) {
                    for (var i = 0; i < iterable.length; i++) this.set(iterable[i][0], iterable[i][1]);
                }
            };
            globalThis.WeakMap.prototype.set = function(key, value) {
                this.__aurora_native_weakmap__.set(boxWeakKey(key), value);
                return this;
            };
            globalThis.WeakMap.prototype.get = function(key) {
                return this.__aurora_native_weakmap__.get(boxWeakKey(key));
            };
            globalThis.WeakMap.prototype.has = function(key) {
                return this.__aurora_native_weakmap__.has(boxWeakKey(key));
            };
            globalThis.WeakMap.prototype.delete = function(key) {
                return this.__aurora_native_weakmap__.delete(boxWeakKey(key));
            };
        })(WeakMap);
    }

    if (!Object.prototype.some) {
        Object.defineProperty(Object.prototype, 'some', {
            value: function(callback, thisArg) {
                if (typeof callback !== 'function') return false;
                var keys = Object.keys(this);
                for (var i = 0; i < keys.length; i++) {
                    if (callback.call(thisArg, this[keys[i]], i, this)) return true;
                }
                return false;
            },
            configurable: true,
            writable: true
        });
    }

    globalThis.matchMedia = function(query) {
        return {
            matches: false,
            media: String(query),
            onchange: null,
            addListener: function() {}, removeListener: function() {},
            addEventListener: function() {}, removeEventListener: function() {},
            dispatchEvent: function() { return true; }
        };
    };

    // VisualViewport: ytd-app.attached() does
    // `window.visualViewport.addEventListener('resize'|'scroll', ...)`; without
    // this the attached callback throws "Cannot read properties of undefined
    // (reading 'addEventListener')" and ytd-app never connects, so navigation
    // and page-content instantiation never run. Built on the real EventTarget
    // so listeners register (we never fire resize/scroll, which is fine).
    globalThis.visualViewport = (function() {
        var vv = new globalThis.EventTarget();
        vv.width = globalThis.innerWidth || 1200;
        vv.height = globalThis.innerHeight || 1024;
        vv.offsetLeft = 0; vv.offsetTop = 0;
        vv.pageLeft = 0; vv.pageTop = 0;
        vv.scale = 1;
        vv.onresize = null; vv.onscroll = null;
        return vv;
    })();

    globalThis.history = {
        length: 1, state: null, scrollRestoration: 'auto',
        pushState: function(state) { this.state = state; },
        replaceState: function(state) { this.state = state; },
        back: function() {}, forward: function() {}, go: function() {}
    };

    globalThis.Screen = function Screen() {
        this.width = 1440; this.height = 1024;
        this.availWidth = 1440; this.availHeight = 1024;
        this.colorDepth = 24; this.pixelDepth = 24;
        this.orientation = { angle: 0, type: 'landscape-primary', onchange: null };
    };
    globalThis.screen = new globalThis.Screen();

    var playerContextDefaults = {
        WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH: {
            contextId: 'WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH',
            serializedExperimentIds: '0',
            serializedExperimentFlags: '0',
            rootElementId: 'movie_player'
        },
        WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_BACKGROUND_AUDIO_PLAYER: {
            contextId: 'WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_BACKGROUND_AUDIO_PLAYER',
            serializedExperimentIds: '0',
            serializedExperimentFlags: '0'
        },
        WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_SFV_AUDIO_ITEM: {
            contextId: 'WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_SFV_AUDIO_ITEM',
            serializedExperimentIds: '0',
            serializedExperimentFlags: '0'
        },
        WEB_PLAYER_CONTEXT_CONFIG_ID_MWEB_SFV_AUDIO_ITEM: {
            contextId: 'WEB_PLAYER_CONTEXT_CONFIG_ID_MWEB_SFV_AUDIO_ITEM',
            serializedExperimentIds: '0',
            serializedExperimentFlags: '0'
        }
    };
    globalThis.yt = globalThis.yt || {};
    globalThis.yt.config_ = globalThis.yt.config_ || {};
    globalThis.ytcfg = globalThis.ytcfg || {};
    globalThis.ytcfg.data_ = globalThis.ytcfg.data_ || {};
    function createPlayerContextConfig(id) {
        id = id && id !== 'undefined' && id !== 'null'
            ? String(id)
            : 'WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH';
        return {
            contextId: id,
            serializedExperimentIds: '0',
            serializedExperimentFlags: '0',
            rootElementId: id === 'WEB_PLAYER_CONTEXT_CONFIG_ID_KEVLAR_WATCH'
                ? 'movie_player'
                : undefined
        };
    }
    function makePlayerContextConfigs(existing) {
        var store = Object.assign({}, playerContextDefaults, existing || {});
        if (typeof Proxy !== 'function') return store;
        return new Proxy(store, {
            get: function(target, prop) {
                if (typeof prop === 'string' && !(prop in target) &&
                    (prop.indexOf('WEB_PLAYER_CONTEXT_CONFIG_ID_') === 0 ||
                     prop === 'undefined' || prop === 'null')) {
                    target[prop] = createPlayerContextConfig(prop);
                }
                return target[prop];
            }
        });
    }
    function installPlayerContextConfigProperty(target) {
        var current = makePlayerContextConfigs(target.WEB_PLAYER_CONTEXT_CONFIGS);
        try {
            Object.defineProperty(target, 'WEB_PLAYER_CONTEXT_CONFIGS', {
                get: function() { return current; },
                set: function(value) { current = makePlayerContextConfigs(value); },
                configurable: true
            });
        } catch (e) {
            target.WEB_PLAYER_CONTEXT_CONFIGS = current;
        }
    }
    installPlayerContextConfigProperty(globalThis.yt.config_);
    installPlayerContextConfigProperty(globalThis.ytcfg.data_);
    globalThis.ytcfg.get = globalThis.ytcfg.get || function(name) { return this.data_[name]; };
    function mergeYtConfig(target, values) {
        values = values || {};
        if (values.WEB_PLAYER_CONTEXT_CONFIGS) {
            values = Object.assign({}, values, {
                WEB_PLAYER_CONTEXT_CONFIGS: Object.assign(
                    {},
                    playerContextDefaults,
                    target.WEB_PLAYER_CONTEXT_CONFIGS || {},
                    values.WEB_PLAYER_CONTEXT_CONFIGS
                )
            });
        }
        Object.assign(target, values);
        target.WEB_PLAYER_CONTEXT_CONFIGS =
            Object.assign({}, playerContextDefaults, target.WEB_PLAYER_CONTEXT_CONFIGS || {});
    }
    globalThis.ytcfg.set = globalThis.ytcfg.set || function(values) {
        mergeYtConfig(this.data_, values);
        mergeYtConfig(globalThis.yt.config_, values);
    };

    globalThis.getComputedStyle = function(el) {
        var style = el && el.style;
        if (style && typeof style.getPropertyValue === 'function') return style;
        return {
            getPropertyValue: function() { return ''; },
            setProperty: function() {}, removeProperty: function() {}
        };
    };

    // Replace the networking-block URL stubs with a parser that handles the
    // relative URL and query-param shapes YouTube uses during bootstrap.
    function decodeUrlPart(value) {
        try { return decodeURIComponent(String(value).replace(/\+/g, ' ')); }
        catch (e) { return String(value); }
    }
    function encodeUrlPart(value) {
        return encodeURIComponent(String(value)).replace(/%20/g, '+');
    }
    function normalizeUrlPath(path) {
        var absolute = path.charAt(0) === '/';
        var trailing = path.length > 1 && path.charAt(path.length - 1) === '/';
        var parts = path.split('/');
        var out = [];
        for (var i = 0; i < parts.length; i++) {
            var part = parts[i];
            if (!part || part === '.') continue;
            if (part === '..') out.pop();
            else out.push(part);
        }
        return (absolute ? '/' : '') + out.join('/') + (trailing && out.length ? '/' : '');
    }
    function splitUrlQueryAndHash(input) {
        var rest = String(input);
        var hash = '';
        var hashIndex = rest.indexOf('#');
        if (hashIndex >= 0) {
            hash = rest.slice(hashIndex);
            rest = rest.slice(0, hashIndex);
        }
        var search = '';
        var queryIndex = rest.indexOf('?');
        if (queryIndex >= 0) {
            search = rest.slice(queryIndex);
            rest = rest.slice(0, queryIndex);
        }
        return { path: rest, search: search, hash: hash };
    }
    function parseAbsoluteUrl(input) {
        var match = /^([a-zA-Z][a-zA-Z0-9+.-]*:)(?:\/\/([^\/?#]*))?([^?#]*)(\?[^#]*)?(#.*)?$/.exec(input);
        if (!match) return null;
        var protocol = match[1].toLowerCase();
        var authority = match[2] || '';
        var pathname = match[3] || '';
        var search = match[4] || '';
        var hash = match[5] || '';
        var username = '';
        var password = '';
        var host = authority;
        var at = host.lastIndexOf('@');
        if (at >= 0) {
            var userinfo = host.slice(0, at);
            host = host.slice(at + 1);
            var colon = userinfo.indexOf(':');
            username = colon >= 0 ? userinfo.slice(0, colon) : userinfo;
            password = colon >= 0 ? userinfo.slice(colon + 1) : '';
        }
        var hostname = host;
        var port = '';
        if (hostname.charAt(0) !== '[') {
            var portIndex = hostname.lastIndexOf(':');
            if (portIndex >= 0) {
                port = hostname.slice(portIndex + 1);
                hostname = hostname.slice(0, portIndex);
            }
        }
        if (authority && !pathname) pathname = '/';
        return {
            protocol: protocol, username: username, password: password,
            host: host, hostname: hostname, port: port,
            pathname: pathname || '/', search: search, hash: hash
        };
    }
    function defaultUrlBase() {
        var loc = globalThis.location || {};
        return String(loc.href || 'https://youtube.com/');
    }
    function parseUrl(input, base) {
        var raw = String(input);
        var absolute = parseAbsoluteUrl(raw);
        if (absolute) return absolute;
        var baseParts = parseAbsoluteUrl(base ? String(base) : defaultUrlBase()) || parseAbsoluteUrl(defaultUrlBase());
        var split = splitUrlQueryAndHash(raw);
        if (raw.indexOf('//') === 0) {
            return parseAbsoluteUrl(baseParts.protocol + raw);
        }
        if (!split.path) {
            return Object.assign({}, baseParts, {
                search: split.search || baseParts.search,
                hash: split.hash
            });
        }
        var pathname = split.path.charAt(0) === '/'
            ? normalizeUrlPath(split.path)
            : normalizeUrlPath(baseParts.pathname.replace(/\/[^\/]*$/, '/') + split.path);
        return Object.assign({}, baseParts, {
            pathname: pathname || '/',
            search: split.search,
            hash: split.hash
        });
    }
    function serializeUrl(parts) {
        var auth = parts.host ? '//' + parts.host : '';
        return parts.protocol + auth + (parts.pathname || '/') + (parts.search || '') + (parts.hash || '');
    }

    globalThis.URLSearchParams = function URLSearchParams(init) {
        if (!(this instanceof URLSearchParams)) return new URLSearchParams(init);
        this._pairs = [];
        if (!init) return;
        if (typeof init === 'string') {
            var query = init.charAt(0) === '?' ? init.slice(1) : init;
            if (!query) return;
            var fields = query.split('&');
            for (var i = 0; i < fields.length; i++) {
                if (!fields[i]) continue;
                var eq = fields[i].indexOf('=');
                this.append(
                    decodeUrlPart(eq >= 0 ? fields[i].slice(0, eq) : fields[i]),
                    decodeUrlPart(eq >= 0 ? fields[i].slice(eq + 1) : '')
                );
            }
        } else if (Array.isArray(init)) {
            for (var j = 0; j < init.length; j++) this.append(init[j][0], init[j][1]);
        } else if (typeof init === 'object') {
            for (var key in init) if (Object.prototype.hasOwnProperty.call(init, key)) this.append(key, init[key]);
        }
    };
    URLSearchParams.prototype.append = function(name, value) {
        this._pairs.push([String(name), String(value)]);
    };
    URLSearchParams.prototype.delete = function(name) {
        name = String(name);
        this._pairs = this._pairs.filter(function(pair) { return pair[0] !== name; });
    };
    URLSearchParams.prototype.get = function(name) {
        name = String(name);
        for (var i = 0; i < this._pairs.length; i++) if (this._pairs[i][0] === name) return this._pairs[i][1];
        return null;
    };
    URLSearchParams.prototype.getAll = function(name) {
        name = String(name);
        return this._pairs.filter(function(pair) { return pair[0] === name; }).map(function(pair) { return pair[1]; });
    };
    URLSearchParams.prototype.has = function(name) {
        name = String(name);
        return this._pairs.some(function(pair) { return pair[0] === name; });
    };
    URLSearchParams.prototype.set = function(name, value) {
        name = String(name);
        value = String(value);
        var found = false;
        var next = [];
        for (var i = 0; i < this._pairs.length; i++) {
            if (this._pairs[i][0] === name) {
                if (!found) next.push([name, value]);
                found = true;
            } else {
                next.push(this._pairs[i]);
            }
        }
        if (!found) next.push([name, value]);
        this._pairs = next;
    };
    URLSearchParams.prototype.forEach = function(cb, thisArg) {
        for (var i = 0; i < this._pairs.length; i++) cb.call(thisArg, this._pairs[i][1], this._pairs[i][0], this);
    };
    URLSearchParams.prototype.toString = function() {
        return this._pairs.map(function(pair) { return encodeUrlPart(pair[0]) + '=' + encodeUrlPart(pair[1]); }).join('&');
    };
    URLSearchParams.prototype.entries = function() { return this._pairs.slice()[Symbol.iterator](); };
    URLSearchParams.prototype.keys = function() { return this._pairs.map(function(pair) { return pair[0]; })[Symbol.iterator](); };
    URLSearchParams.prototype.values = function() { return this._pairs.map(function(pair) { return pair[1]; })[Symbol.iterator](); };
    URLSearchParams.prototype[Symbol.iterator] = URLSearchParams.prototype.entries;

    globalThis.URL = function URL(input, base) {
        if (!(this instanceof URL)) return new URL(input, base);
        var parts = parseUrl(input, base);
        var self = this;
        function updateSearchParams() { self.searchParams = new URLSearchParams(parts.search); }
        function syncSearch() {
            var query = self.searchParams.toString();
            parts.search = query ? '?' + query : '';
        }
        Object.defineProperties(this, {
            href: { get: function() { syncSearch(); return serializeUrl(parts); }, set: function(v) { parts = parseUrl(v); updateSearchParams(); }, enumerable: true },
            origin: { get: function() { return parts.protocol + '//' + parts.host; }, enumerable: true },
            protocol: { get: function() { return parts.protocol; }, set: function(v) { parts.protocol = String(v).replace(/:*$/, ':').toLowerCase(); }, enumerable: true },
            username: { get: function() { return parts.username; }, set: function(v) { parts.username = String(v); }, enumerable: true },
            password: { get: function() { return parts.password; }, set: function(v) { parts.password = String(v); }, enumerable: true },
            host: { get: function() { return parts.host; }, set: function(v) { var p = parseAbsoluteUrl(parts.protocol + '//' + String(v) + '/'); if (p) { parts.host = p.host; parts.hostname = p.hostname; parts.port = p.port; } }, enumerable: true },
            hostname: { get: function() { return parts.hostname; }, set: function(v) { parts.hostname = String(v); parts.host = parts.hostname + (parts.port ? ':' + parts.port : ''); }, enumerable: true },
            port: { get: function() { return parts.port; }, set: function(v) { parts.port = String(v); parts.host = parts.hostname + (parts.port ? ':' + parts.port : ''); }, enumerable: true },
            pathname: { get: function() { return parts.pathname; }, set: function(v) { parts.pathname = normalizeUrlPath(String(v)); }, enumerable: true },
            search: { get: function() { syncSearch(); return parts.search; }, set: function(v) { parts.search = v ? (String(v).charAt(0) === '?' ? String(v) : '?' + String(v)) : ''; updateSearchParams(); }, enumerable: true },
            hash: { get: function() { return parts.hash; }, set: function(v) { parts.hash = v ? (String(v).charAt(0) === '#' ? String(v) : '#' + String(v)) : ''; }, enumerable: true }
        });
        updateSearchParams();
    };
    URL.prototype.toString = function() { return this.href; };
    URL.prototype.toJSON = function() { return this.href; };
    URL.createObjectURL = function() { return 'blob:aurora'; };
    URL.revokeObjectURL = function() {};

    globalThis.Blob = function Blob(parts, opts) {
        this.size = 0;
        this.type = (opts && opts.type) || '';
        this.text = function() { return Promise.resolve(''); };
        this.arrayBuffer = function() { return Promise.resolve(new ArrayBuffer(0)); };
        this.slice = function() { return this; };
    };
    globalThis.File = function File(parts, name, opts) {
        globalThis.Blob.call(this, parts, opts);
        this.name = name || '';
        this.lastModified = Date.now();
    };
    File.prototype = Object.create(Blob.prototype);

    globalThis.FormData = function FormData() {
        var m = {};
        this.append = function(k, v) { m[k] = v; };
        this.set = function(k, v) { m[k] = v; };
        this.get = function(k) { return k in m ? m[k] : null; };
        this.has = function(k) { return k in m; };
        this.delete = function(k) { delete m[k]; };
        this.forEach = function(fn) { for (var k in m) fn(m[k], k, this); };
    };

    globalThis.WebSocket = function WebSocket(url) {
        this.url = String(url);
        this.readyState = 3; // CLOSED — no socket backend
        this.bufferedAmount = 0;
        this.send = function() {};
        this.close = function() {};
        this.addEventListener = function() {};
        this.removeEventListener = function() {};
        this.onopen = null; this.onclose = null; this.onerror = null; this.onmessage = null;
    };
    WebSocket.CONNECTING = 0; WebSocket.OPEN = 1; WebSocket.CLOSING = 2; WebSocket.CLOSED = 3;

    globalThis.TextEncoder = globalThis.TextEncoder || function TextEncoder() {
        this.encode = function(s) {
            s = String(s === undefined ? '' : s);
            var out = [];
            for (var i = 0; i < s.length; i++) {
                var c = s.codePointAt(i);
                if (c > 0xffff) i++;
                if (c < 0x80) out.push(c);
                else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
                else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
                else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 0x3f), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
            }
            return new Uint8Array(out);
        };
    };

    // AbortController / AbortSignal. YouTube's kevlar_base bundle references
    // `AbortSignal` during init (fetch + scheduler plumbing); without it the
    // main module throws a ReferenceError before ytd-app boots. Built on the
    // real EventTarget above so 'abort' listeners actually fire. DOMException
    // is referenced lazily because it's a host-provided global that may not
    // exist; we fall back to a named Error when it's missing.
    if (typeof globalThis.AbortSignal !== 'function') {
        function makeAbortError(message, name) {
            message = message || 'signal is aborted without reason';
            name = name || 'AbortError';
            if (typeof globalThis.DOMException === 'function') {
                return new DOMException(message, name);
            }
            var err = new Error(message);
            err.name = name;
            return err;
        }

        function fireAbort(signal, reason) {
            if (signal.aborted) return;
            signal.aborted = true;
            signal.reason = reason === undefined ? makeAbortError() : reason;
            var event;
            try {
                event = typeof globalThis.Event === 'function'
                    ? new Event('abort')
                    : { type: 'abort' };
            } catch (e) {
                event = { type: 'abort' };
            }
            if (typeof signal.onabort === 'function') {
                try { signal.onabort.call(signal, event); } catch (e) {}
            }
            try { signal.dispatchEvent(event); } catch (e) {}
        }

        function AbortSignal() {
            EventTarget.call(this);
            this.aborted = false;
            this.reason = undefined;
            this.onabort = null;
        }
        AbortSignal.prototype = Object.create(EventTarget.prototype);
        AbortSignal.prototype.constructor = AbortSignal;
        AbortSignal.prototype.throwIfAborted = function() {
            if (this.aborted) throw this.reason;
        };

        AbortSignal.abort = function(reason) {
            var signal = new AbortSignal();
            signal.aborted = true;
            signal.reason = reason === undefined ? makeAbortError() : reason;
            return signal;
        };
        AbortSignal.timeout = function(ms) {
            var signal = new AbortSignal();
            setTimeout(function() {
                fireAbort(signal, makeAbortError('signal timed out', 'TimeoutError'));
            }, ms);
            return signal;
        };
        AbortSignal.any = function(signals) {
            var result = new AbortSignal();
            var list = Array.prototype.slice.call(signals || []);
            for (var i = 0; i < list.length; i++) {
                if (list[i] && list[i].aborted) {
                    fireAbort(result, list[i].reason);
                    return result;
                }
            }
            list.forEach(function(s) {
                if (s && typeof s.addEventListener === 'function') {
                    s.addEventListener('abort', function() { fireAbort(result, s.reason); });
                }
            });
            return result;
        };

        function AbortController() {
            this.signal = new AbortSignal();
        }
        AbortController.prototype.abort = function(reason) {
            fireAbort(this.signal, reason);
        };

        globalThis.AbortSignal = AbortSignal;
        globalThis.AbortController = AbortController;
    }
})();
