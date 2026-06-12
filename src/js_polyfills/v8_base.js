// V8-only environment shims, run before the shared polyfills
// (event_constructors.js / custom_elements.js need HTMLElement,
// queueMicrotask, etc. to exist). The SpiderMonkey engine installs
// equivalents natively in js_sm/globals/browser_api.rs.
(function() {
    if (typeof globalThis.queueMicrotask !== 'function') {
        globalThis.queueMicrotask = function(fn) { Promise.resolve().then(fn); };
    }

    // DOM constructor skeletons. Aurora's V8 node wrappers are plain objects,
    // so these exist for prototype-chain patching (Polymer's ES5 adapter wraps
    // HTMLElement; webcomponents probes Element.prototype) rather than as the
    // wrappers' actual prototypes.
    globalThis.EventTarget = function EventTarget() {};
    EventTarget.prototype.addEventListener = function(){};
    EventTarget.prototype.removeEventListener = function(){};
    EventTarget.prototype.dispatchEvent = function(){ return true; };

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
    globalThis.CSSStyleSheet = function CSSStyleSheet() {};
    globalThis.Worker = function Worker() {
        this.postMessage = function(){};
        this.terminate = function(){};
        this.addEventListener = function(){};
        this.removeEventListener = function(){};
        this.onmessage = null; this.onerror = null;
    };
    globalThis.SharedWorker = globalThis.Worker;

    globalThis.DOMException = function DOMException(message, name) {
        this.message = message || '';
        this.name = name || 'Error';
        this.code = 0;
    };
    DOMException.prototype = Object.create(Error.prototype);

    // AbortSignal / AbortController (replaces the minimal stub installed with
    // the networking polyfills — fetch-polyfill.js references the constructor).
    globalThis.AbortSignal = function AbortSignal() {
        this.aborted = false;
        this.reason = undefined;
        this.onabort = null;
    };
    AbortSignal.prototype = Object.create(EventTarget.prototype);
    AbortSignal.prototype.constructor = AbortSignal;
    AbortSignal.prototype.throwIfAborted = function() {
        if (this.aborted) throw this.reason;
    };
    AbortSignal.abort = function(reason) {
        var s = new AbortSignal();
        s.aborted = true;
        s.reason = reason;
        return s;
    };
    AbortSignal.timeout = function() { return new AbortSignal(); };
    AbortSignal.any = function() { return new AbortSignal(); };

    globalThis.AbortController = function AbortController() {
        this.signal = new AbortSignal();
    };
    AbortController.prototype.abort = function(reason) {
        this.signal.aborted = true;
        this.signal.reason = reason;
        if (typeof this.signal.onabort === 'function') {
            try { this.signal.onabort({ type: 'abort', target: this.signal }); } catch (e) {}
        }
    };

    // Performance
    var timeOrigin = Date.now();
    globalThis.performance = {
        timeOrigin: timeOrigin,
        now: function() { return Date.now() - timeOrigin; },
        mark: function() {}, measure: function() {},
        clearMarks: function() {}, clearMeasures: function() {},
        clearResourceTimings: function() {}, setResourceTimingBufferSize: function() {},
        getEntries: function() { return []; },
        getEntriesByName: function() { return []; },
        getEntriesByType: function() { return []; },
        navigation: { type: 0, redirectCount: 0 },
        timing: {
            navigationStart: timeOrigin, fetchStart: timeOrigin,
            domainLookupStart: timeOrigin, domainLookupEnd: timeOrigin,
            connectStart: timeOrigin, connectEnd: timeOrigin,
            requestStart: timeOrigin, responseStart: timeOrigin,
            responseEnd: timeOrigin, domLoading: timeOrigin,
            domInteractive: 0, domContentLoadedEventStart: 0,
            domContentLoadedEventEnd: 0, domComplete: 0,
            loadEventStart: 0, loadEventEnd: 0, unloadEventStart: 0,
            unloadEventEnd: 0, redirectStart: 0, redirectEnd: 0,
            secureConnectionStart: 0
        }
    };

    // Observers — stubs that never fire. (js_sm has a real MutationObserver;
    // V8 parity is future work. The webcomponents/intersection-observer
    // bundles skip their own polyfills when these exist.)
    globalThis.MutationObserver = function MutationObserver(cb) { this._cb = cb; };
    MutationObserver.prototype.observe = function() {};
    MutationObserver.prototype.disconnect = function() {};
    MutationObserver.prototype.takeRecords = function() { return []; };

    globalThis.IntersectionObserver = function IntersectionObserver(cb) { this._cb = cb; };
    IntersectionObserver.prototype.observe = function() {};
    IntersectionObserver.prototype.unobserve = function() {};
    IntersectionObserver.prototype.disconnect = function() {};
    IntersectionObserver.prototype.takeRecords = function() { return []; };
    globalThis.IntersectionObserverEntry = function IntersectionObserverEntry() {};

    globalThis.ResizeObserver = function ResizeObserver(cb) { this._cb = cb; };
    ResizeObserver.prototype.observe = function() {};
    ResizeObserver.prototype.unobserve = function() {};
    ResizeObserver.prototype.disconnect = function() {};

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

    globalThis.history = {
        length: 1, state: null, scrollRestoration: 'auto',
        pushState: function(state) { this.state = state; },
        replaceState: function(state) { this.state = state; },
        back: function() {}, forward: function() {}, go: function() {}
    };

    globalThis.getComputedStyle = function(el) {
        var style = el && el.style;
        if (style && typeof style.getPropertyValue === 'function') return style;
        return {
            getPropertyValue: function() { return ''; },
            setProperty: function() {}, removeProperty: function() {}
        };
    };

    // Replace the networking-block URL stub with a real-enough parser.
    globalThis.URL = function URL(url, base) {
        url = String(url);
        if (base && url.indexOf('://') < 0) {
            base = String(base);
            var root = base.match(/^[a-z][a-z0-9+.-]*:\/\/[^\/]*/i);
            if (url.charAt(0) === '/' && url.charAt(1) === '/') {
                url = (base.match(/^[a-z][a-z0-9+.-]*:/i) || ['https:'])[0] + url;
            } else if (url.charAt(0) === '/') {
                url = (root ? root[0] : base) + url;
            } else {
                url = base.replace(/[^\/]*$/, '') + url;
            }
        }
        var m = url.match(/^([a-z][a-z0-9+.-]*:)\/\/([^\/?#:]*)(?::(\d+))?([^?#]*)(\?[^#]*)?(#.*)?$/i);
        this.href = url;
        this.protocol = m ? m[1] : '';
        this.hostname = m ? m[2] : '';
        this.port = m && m[3] ? m[3] : '';
        this.host = this.hostname + (this.port ? ':' + this.port : '');
        this.origin = m ? this.protocol + '//' + this.host : '';
        this.pathname = m ? (m[4] || '/') : '';
        this.search = m && m[5] ? m[5] : '';
        this.hash = m && m[6] ? m[6] : '';
        this.username = ''; this.password = '';
        this.searchParams = new URLSearchParams(this.search);
    };
    URL.prototype.toString = function() { return this.href; };
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
})();
