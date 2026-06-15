// V8-only shims that depend on the shared polyfills (Event from
// event_constructors.js, the customElements registry) and on `document`
// already being installed. Runs last in the V8 bootstrap.
(function() {
    // Event subtypes on top of the shared Event polyfill.
    globalThis.Event.prototype.initEvent = function(type, bubbles, cancelable) {
        this.type = type || '';
        this.bubbles = !!bubbles;
        this.cancelable = !!cancelable;
    };
    globalThis.CustomEvent.prototype.initCustomEvent = function(type, bubbles, cancelable, detail) {
        this.initEvent(type, bubbles, cancelable);
        this.detail = detail === undefined ? null : detail;
    };
    [
        'MouseEvent','KeyboardEvent','FocusEvent','InputEvent','UIEvent',
        'TouchEvent','WheelEvent','PointerEvent','CompositionEvent',
        'AnimationEvent','TransitionEvent','ProgressEvent','PopStateEvent',
        'HashChangeEvent','BeforeUnloadEvent','PageTransitionEvent',
        'StorageEvent','DragEvent','ClipboardEvent','SubmitEvent'
    ].forEach(function(name) {
        var Ctor = function(type, init) {
            globalThis.Event.call(this, type, init);
            init = init || {};
            this.detail = init.detail !== undefined ? init.detail : 0;
            this.clientX = init.clientX || 0; this.clientY = init.clientY || 0;
            this.key = init.key || ''; this.keyCode = init.keyCode || 0;
            this.button = init.button || 0;
            this.relatedTarget = init.relatedTarget || null;
        };
        Ctor.prototype = Object.create(globalThis.Event.prototype);
        Ctor.prototype.constructor = Ctor;
        globalThis[name] = Ctor;
    });

    // Document factory and convenience shims over the native bridge.
    document.createElementNS = function(ns, tag) { return document.createElement(tag); };
    // Mirrors js_sm: comments become text nodes (no Comment node type in the DOM core).
    document.createComment = function(text) { return document.createTextNode(text); };
    document.createDocumentFragment = function() { return document.createElement('#document-fragment'); };
    document.importNode = function(node, deep) {
        return node && typeof node.cloneNode === 'function' ? node.cloneNode(deep) : null;
    };
    document.adoptNode = function(node) { return node; };
    document.createEvent = function(kind) {
        var Ctor = globalThis[kind] || globalThis.Event;
        return new Ctor('');
    };
    document.createRange = function() {
        return {
            selectNode: function() {}, selectNodeContents: function() {},
            setStart: function() {}, setEnd: function() {},
            collapse: function() {}, deleteContents: function() {},
            insertNode: function() {}, detach: function() {},
            cloneContents: function() { return document.createDocumentFragment(); },
            createContextualFragment: function(html) {
                var t = document.createElement('template');
                t.innerHTML = String(html);
                return t.content;
            },
            getBoundingClientRect: function() {
                return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, x: 0, y: 0 };
            }
        };
    };
    globalThis.NodeFilter = {
        FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3,
        SHOW_ALL: 0xFFFFFFFF, SHOW_ELEMENT: 1, SHOW_ATTRIBUTE: 2,
        SHOW_TEXT: 4, SHOW_CDATA_SECTION: 8, SHOW_PROCESSING_INSTRUCTION: 64,
        SHOW_COMMENT: 128, SHOW_DOCUMENT: 256, SHOW_DOCUMENT_TYPE: 512,
        SHOW_DOCUMENT_FRAGMENT: 1024
    };
    document.createTreeWalker = function(root, whatToShow, filter) {
        whatToShow = whatToShow === undefined ? 0xFFFFFFFF : whatToShow;
        function accepts(n) {
            if (!n || !n.nodeType) return false;
            if (!(whatToShow & (1 << (n.nodeType - 1)))) return false;
            if (filter) {
                var f = typeof filter === 'function' ? filter : filter.acceptNode;
                if (f && f.call(filter, n) !== 1) return false;
            }
            return true;
        }
        function nextInTree(n) {
            if (n.firstChild) return n.firstChild;
            while (n && n !== root) {
                if (n.nextSibling) return n.nextSibling;
                n = n.parentNode;
            }
            return null;
        }
        return {
            root: root, currentNode: root, whatToShow: whatToShow, filter: filter || null,
            nextNode: function() {
                var n = this.currentNode;
                while (n) {
                    n = nextInTree(n);
                    if (n && accepts(n)) { this.currentNode = n; return n; }
                    if (!n) break;
                }
                return null;
            },
            firstChild: function() {
                var c = this.currentNode.firstChild;
                while (c && !accepts(c)) c = c.nextSibling;
                if (c) this.currentNode = c;
                return c || null;
            },
            lastChild: function() { return null; },
            nextSibling: function() {
                var c = this.currentNode.nextSibling;
                while (c && !accepts(c)) c = c.nextSibling;
                if (c) this.currentNode = c;
                return c || null;
            },
            previousSibling: function() { return null; },
            previousNode: function() { return null; },
            parentNode: function() {
                var p = this.currentNode.parentNode;
                if (p) this.currentNode = p;
                return p || null;
            }
        };
    };
    document.createNodeIterator = function(root, whatToShow, filter) {
        var walker = document.createTreeWalker(root, whatToShow, filter);
        return { root: root, nextNode: function() { return walker.nextNode(); }, previousNode: function() { return null; }, detach: function() {} };
    };

    function installPrototypeForward(proto, name) {
        if (!proto || !name) return;
        var forward = function() {
            var own = Object.prototype.hasOwnProperty.call(this, name) ? this[name] : null;
            if (typeof own === 'function' && own !== forward) {
                return own.apply(this, arguments);
            }
            return undefined;
        };
        try {
            Object.defineProperty(proto, name, {
                value: forward,
                configurable: true,
                writable: true
            });
        } catch (e) {
            proto[name] = forward;
        }
    }

    [
        'appendChild', 'insertBefore', 'removeChild', 'replaceChild',
        'cloneNode', 'contains', 'hasChildNodes', 'getRootNode',
        'append', 'prepend', 'before', 'after', 'remove',
        'replaceChildren', 'replaceWith'
    ].forEach(function(name) {
        installPrototypeForward(globalThis.Node && Node.prototype, name);
    });
    [
        'querySelector', 'querySelectorAll', 'getElementsByTagName',
        'getElementsByClassName', 'matches', 'closest',
        'getAttribute', 'setAttribute', 'removeAttribute', 'hasAttribute',
        'toggleAttribute',
        'insertAdjacentHTML', 'insertAdjacentElement', 'insertAdjacentText',
        'normalize'
    ].forEach(function(name) {
        installPrototypeForward(globalThis.Element && Element.prototype, name);
        installPrototypeForward(globalThis.HTMLElement && HTMLElement.prototype, name);
        installPrototypeForward(globalThis.DocumentFragment && DocumentFragment.prototype, name);
        installPrototypeForward(globalThis.Document && Document.prototype, name);
    });
    [
        ['__shady_setAttribute', 'setAttribute'],
        ['__shady_removeAttribute', 'removeAttribute'],
        ['__shady_getAttribute', 'getAttribute'],
        ['__shady_hasAttribute', 'hasAttribute'],
        ['__shady_getRootNode', 'getRootNode'],
        ['__shady_appendChild', 'appendChild'],
        ['__shady_insertBefore', 'insertBefore'],
        ['__shady_removeChild', 'removeChild'],
        ['__shady_replaceChild', 'replaceChild'],
        ['__shady_addEventListener', 'addEventListener'],
        ['__shady_removeEventListener', 'removeEventListener'],
        ['__shady_dispatchEvent', 'dispatchEvent']
    ].forEach(function(pair) {
        var shadyName = pair[0];
        var nativeName = pair[1];
        try {
            Object.defineProperty(Object.prototype, shadyName, {
                value: function() {
                    var fn = this && this[nativeName];
                    if (typeof fn === 'function') return fn.apply(this, arguments);
                    return nativeName === 'dispatchEvent' ? true : undefined;
                },
                configurable: true,
                writable: true
            });
        } catch (e) {}
    });
    [
        ['__shady_native_contains', 'contains'],
        ['__shady_native_getRootNode', 'getRootNode'],
        ['__shady_native_querySelector', 'querySelector'],
        ['__shady_native_querySelectorAll', 'querySelectorAll'],
        ['__shady_native_appendChild', 'appendChild'],
        ['__shady_native_insertBefore', 'insertBefore'],
        ['__shady_native_removeChild', 'removeChild'],
        ['__shady_native_replaceChild', 'replaceChild'],
        ['__shady_native_setAttribute', 'setAttribute'],
        ['__shady_native_removeAttribute', 'removeAttribute'],
        ['__shady_native_addEventListener', 'addEventListener'],
        ['__shady_native_removeEventListener', 'removeEventListener']
    ].forEach(function(pair) {
        var shadyName = pair[0];
        var nativeName = pair[1];
        if (Object.prototype[shadyName]) return;
        try {
            Object.defineProperty(Object.prototype, shadyName, {
                value: function() {
                    var fn = this && this[nativeName];
                    if (typeof fn === 'function') return fn.apply(this, arguments);
                    return undefined;
                },
                configurable: true,
                writable: true
            });
        } catch (e) {}
    });

    // Element decoration — called from the native bridge for every element
    // wrapper. Installs per-element JS APIs that are simpler here than native.
    globalThis.__aurora_decorate_element__ = function(el) {
        // Most wrappers inherit these through HTMLElement -> Node -> EventTarget,
        // but template/shadow stamping can surface older wrappers whose prototype
        // chain was established before the JS skeletons existed. Keep event
        // delivery available on the instance so Polymer-ready code can safely
        // attach listeners to stamped ids.
        if (typeof el.addEventListener !== 'function' && globalThis.EventTarget) {
            el.addEventListener = EventTarget.prototype.addEventListener;
            el.removeEventListener = EventTarget.prototype.removeEventListener;
            el.dispatchEvent = EventTarget.prototype.dispatchEvent;
        }
        if (!el.__aurora_metric_fallbacks__) {
            try {
                Object.defineProperty(el, '__aurora_metric_fallbacks__', {
                    value: true,
                    configurable: true
                });
            } catch (e) {
                el.__aurora_metric_fallbacks__ = true;
            }
            var isCustomElement = function() {
                try {
                    var name = el.localName || '';
                    return name.indexOf('-') >= 0;
                } catch (e) {
                    return false;
                }
            };
            var connected = function() {
                try { return el.isConnected === true; } catch (e) { return false; }
            };
            var widthFallback = function() {
                if (!isCustomElement() || !connected()) return 0;
                var parent = null;
                try { parent = el.parentElement || el.parentNode || null; } catch (e) {}
                if (parent && parent !== el) {
                    try {
                        var parentWidth = Number(parent.clientWidth || parent.offsetWidth || 0);
                        if (parentWidth > 0) return parentWidth;
                    } catch (e2) {}
                }
                return Number(globalThis.innerWidth || 0);
            };
            var heightFallback = function() {
                if (!isCustomElement() || !connected()) return 0;
                return Number(globalThis.innerHeight || 0);
            };
            function metric(name, fallback) {
                var value = 0;
                try { value = Number(el[name] || 0); } catch (e) {}
                try {
                    Object.defineProperty(el, name, {
                        configurable: true,
                        enumerable: true,
                        get: function() {
                            return value || fallback();
                        },
                        set: function(next) {
                            value = Number(next || 0);
                        }
                    });
                } catch (e) {}
            }
            metric('clientWidth', widthFallback);
            metric('offsetWidth', widthFallback);
            metric('scrollWidth', widthFallback);
            metric('clientHeight', heightFallback);
            metric('offsetHeight', heightFallback);
            metric('scrollHeight', heightFallback);
        }
        el.animate = function() {
            var anim = {
                playState: 'finished', currentTime: 0, startTime: 0,
                playbackRate: 1, effect: null, timeline: null,
                onfinish: null, oncancel: null,
                play: function() {}, pause: function() {}, reverse: function() {},
                updatePlaybackRate: function() {},
                cancel: function() {
                    if (typeof anim.oncancel === 'function') anim.oncancel({ target: anim });
                },
                finish: function() {
                    if (typeof anim.onfinish === 'function') anim.onfinish({ target: anim });
                },
                addEventListener: function() {}, removeEventListener: function() {},
                finished: Promise.resolve()
            };
            queueMicrotask(function() {
                if (typeof anim.onfinish === 'function') anim.onfinish({ target: anim });
            });
            return anim;
        };
        el.getAnimations = function() { return []; };
        el.scrollIntoView = function() {};
        el.scrollTo = function() {};
        el.scrollBy = function() {};
        el.focus = function() {};
        el.blur = function() {};
        el.click = function() {};
        el.releasePointerCapture = function() {};
        el.setPointerCapture = function() {};
        el.hasPointerCapture = function() { return false; };
    };

    // Canvas decoration — called from the native bridge for every <canvas>
    // wrapper. Stub 2D/WebGL contexts so probing code proceeds (no real pixels).
    globalThis.__aurora_install_canvas__ = function(el) {
        var noop = function() {};
        var ctx2d = {
            canvas: el, fillStyle: '#000', strokeStyle: '#000', font: '10px sans-serif',
            globalAlpha: 1, lineWidth: 1, textAlign: 'start', textBaseline: 'alphabetic',
            save: noop, restore: noop, scale: noop, rotate: noop, translate: noop,
            transform: noop, setTransform: noop, resetTransform: noop,
            clearRect: noop, fillRect: noop, strokeRect: noop,
            beginPath: noop, closePath: noop, moveTo: noop, lineTo: noop,
            bezierCurveTo: noop, quadraticCurveTo: noop, arc: noop, arcTo: noop,
            ellipse: noop, rect: noop, fill: noop, stroke: noop, clip: noop,
            isPointInPath: function() { return false; },
            drawImage: noop, createLinearGradient: function() { return { addColorStop: noop }; },
            createRadialGradient: function() { return { addColorStop: noop }; },
            createPattern: function() { return null; },
            fillText: noop, strokeText: noop,
            measureText: function(s) { return { width: String(s).length * 6 }; },
            getImageData: function(x, y, w, h) {
                return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) };
            },
            putImageData: noop,
            createImageData: function(w, h) {
                return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) };
            },
            getLineDash: function() { return []; }, setLineDash: noop
        };
        el.width = parseFloat(el.getAttribute('width')) || 300;
        el.height = parseFloat(el.getAttribute('height')) || 150;
        el.getContext = function(kind) {
            return (kind === '2d') ? ctx2d : null;
        };
        el.toDataURL = function() { return 'data:,'; };
        el.toBlob = function(cb) { if (typeof cb === 'function') queueMicrotask(function() { cb(null); }); };
        el.transferControlToOffscreen = function() { return el; };
    };

    // Media decoration — enough HTMLMediaElement shape for YouTube/player
    // bootstrap code to probe capabilities, attach listeners and call play().
    if (typeof globalThis.__aurora_TimeRanges__ !== 'function') {
        globalThis.__aurora_TimeRanges__ = function TimeRanges(ranges) {
            this._ranges = ranges || [];
        };
        Object.defineProperty(globalThis.__aurora_TimeRanges__.prototype, 'length', {
            get: function() { return this._ranges.length; }
        });
        globalThis.__aurora_TimeRanges__.prototype.start = function(i) {
            if (!this._ranges[i]) throw new DOMException('Index out of range', 'IndexSizeError');
            return this._ranges[i][0];
        };
        globalThis.__aurora_TimeRanges__.prototype.end = function(i) {
            if (!this._ranges[i]) throw new DOMException('Index out of range', 'IndexSizeError');
            return this._ranges[i][1];
        };
    }
    globalThis.__aurora_install_media_element__ = function(el) {
        if (!el || el.__media_installed__) return;
        el.__media_installed__ = true;
        var fire = function(type) {
            var ev = new Event(type);
            try { el.dispatchEvent(ev); } catch (e) {}
            var handler = el['on' + type];
            if (typeof handler === 'function') {
                try { handler.call(el, ev); } catch (e) {}
            }
        };
        var state = {
            currentTime: 0, duration: NaN, paused: true, ended: false,
            seeking: false, readyState: 0, networkState: 0,
            volume: 1, muted: false, defaultMuted: false,
            playbackRate: 1, defaultPlaybackRate: 1,
            autoplay: false, loop: false, controls: false, preload: 'metadata',
            crossOrigin: null, currentSrc: el.getAttribute('src') || '',
            error: null, srcObject: null,
            videoWidth: el.localName === 'video' ? 640 : 0,
            videoHeight: el.localName === 'video' ? 360 : 0,
            textTracks: []
        };
        [
            'currentTime','duration','paused','ended','seeking','readyState',
            'networkState','volume','muted','defaultMuted','playbackRate',
            'defaultPlaybackRate','autoplay','loop','controls','preload',
            'crossOrigin','currentSrc','error','srcObject','videoWidth',
            'videoHeight','textTracks'
        ].forEach(function(key) {
            Object.defineProperty(el, key, {
                get: function() { return state[key]; },
                set: function(v) { state[key] = v; },
                configurable: true,
                enumerable: true
            });
        });
        el.HAVE_NOTHING = 0; el.HAVE_METADATA = 1; el.HAVE_CURRENT_DATA = 2;
        el.HAVE_FUTURE_DATA = 3; el.HAVE_ENOUGH_DATA = 4;
        el.NETWORK_EMPTY = 0; el.NETWORK_IDLE = 1; el.NETWORK_LOADING = 2; el.NETWORK_NO_SOURCE = 3;
        Object.defineProperty(el, 'buffered', { get: function() { return new globalThis.__aurora_TimeRanges__([]); } });
        Object.defineProperty(el, 'played', { get: function() { return new globalThis.__aurora_TimeRanges__(state.currentTime > 0 ? [[0, state.currentTime]] : []); } });
        Object.defineProperty(el, 'seekable', { get: function() { return new globalThis.__aurora_TimeRanges__(isFinite(state.duration) ? [[0, state.duration]] : []); } });
        Object.defineProperty(el, 'src', {
            get: function() { return state.currentSrc; },
            set: function(v) {
                state.currentSrc = String(v || '');
                state.networkState = state.currentSrc ? 2 : 0;
                fire('loadstart');
                queueMicrotask(function() {
                    state.readyState = 4;
                    state.networkState = 1;
                    if (isNaN(state.duration)) state.duration = 0;
                    fire('loadedmetadata'); fire('loadeddata'); fire('canplay'); fire('canplaythrough');
                    if (state.autoplay) el.play();
                });
            },
            configurable: true,
            enumerable: true
        });
        el.load = function() {
            state.readyState = 0;
            state.networkState = state.currentSrc ? 2 : 0;
            fire('loadstart');
        };
        el.canPlayType = function(type) {
            return (typeof type === 'string' && /^(video|audio)\/(mp4|webm|ogg)/i.test(type)) ? 'probably' : '';
        };
        el.play = function() {
            state.paused = false;
            state.ended = false;
            state.readyState = Math.max(state.readyState, 4);
            fire('play'); fire('playing');
            return Promise.resolve();
        };
        el.pause = function() {
            if (state.paused) return;
            state.paused = true;
            fire('pause');
        };
        el.fastSeek = function(t) { state.currentTime = Number(t) || 0; fire('seeked'); };
        el.addTextTrack = function(kind, label, lang) {
            var track = {
                kind: kind || 'subtitles', label: label || '', language: lang || '',
                mode: 'disabled', cues: [], activeCues: [],
                addEventListener: function(){}, removeEventListener: function(){},
                addCue: function(){}, removeCue: function(){}
            };
            state.textTracks.push(track);
            return track;
        };
        el.captureStream = function() {
            return { getTracks: function(){ return []; }, getAudioTracks: function(){ return []; },
                getVideoTracks: function(){ return []; }, addTrack: function(){}, removeTrack: function(){} };
        };
        if (state.currentSrc) el.src = state.currentSrc;
    };

    document.getElementsByClassName = function(cls) {
        var sel = '.' + String(cls).trim().split(/\s+/).join('.');
        return document.querySelectorAll(sel);
    };
    document.getElementsByName = function(name) {
        return document.querySelectorAll('[name="' + String(name) + '"]');
    };
    // Route window/document events through the real JS EventTarget so listeners
    // added via addEventListener actually receive dispatched events (and
    // delegated/bubbling events from elements reach document/window).
    document.addEventListener = EventTarget.prototype.addEventListener;
    document.removeEventListener = EventTarget.prototype.removeEventListener;
    document.dispatchEvent = EventTarget.prototype.dispatchEvent;
    document.readyState = 'loading';
    document.hidden = false;
    document.visibilityState = 'visible';
    document.cookie = '';
    document.currentScript = null;
    document.compatMode = 'CSS1Compat';
    document.characterSet = 'UTF-8';
    document.referrer = '';
    document.domain = '';
    document.contentType = 'text/html';
    document.implementation = {
        hasFeature: function() { return true; },
        createHTMLDocument: function() { return document; },
        createDocumentType: function() { return null; }
    };
    document.write = function() {};
    document.writeln = function() {};
    document.open = function() {};
    document.close = function() {};
    document.execCommand = function() { return false; };
    document.hasFocus = function() { return false; };
    document.getSelection = function() { return null; };
    document.elementFromPoint = function() { return null; };
    document.activeElement = document.body || null;
    document.scrollingElement = document.documentElement || null;
    document.location = globalThis.location;
    document.fonts = {
        ready: Promise.resolve(),
        load: function() { return Promise.resolve([]); },
        check: function() { return true; },
        addEventListener: function() {}, removeEventListener: function() {}
    };

    globalThis.addEventListener = EventTarget.prototype.addEventListener;
    globalThis.removeEventListener = EventTarget.prototype.removeEventListener;
    globalThis.dispatchEvent = EventTarget.prototype.dispatchEvent;

    // ShadyDOM saves "native" copies as __shady_native_* before patching; our
    // flat global has no EventTarget/Window prototype chain for it to harvest
    // from, so it skips the save and later calls land on undefined. Predefine.
    globalThis.__shady_native_addEventListener = globalThis.addEventListener;
    globalThis.__shady_native_removeEventListener = globalThis.removeEventListener;
    globalThis.__shady_native_dispatchEvent = globalThis.dispatchEvent;
    document.__shady_native_addEventListener = document.addEventListener;
    document.__shady_native_removeEventListener = document.removeEventListener;
    document.__shady_native_dispatchEvent = document.dispatchEvent;
    document.__shady_native_createElement = document.createElement;
    document.__shady_native_createTextNode = document.createTextNode;
    document.__shady_native_importNode = document.importNode;
    globalThis.scrollX = 0; globalThis.scrollY = 0;
    globalThis.pageXOffset = 0; globalThis.pageYOffset = 0;
    globalThis.outerWidth = globalThis.innerWidth; globalThis.outerHeight = globalThis.innerHeight;
    globalThis.frames = globalThis;
    globalThis.parent = globalThis;
    globalThis.top = globalThis;
    globalThis.open = function() { return null; };
    globalThis.close = function() {};
    globalThis.focus = function() {};
    globalThis.blur = function() {};
    globalThis.confirm = function() { return false; };
    globalThis.prompt = function() { return null; };
    globalThis.postMessage = function() {};
    globalThis.getSelection = function() { return null; };
    globalThis.reportError = function() {};

    // Navigator extras (the native bridge only sets userAgent).
    navigator.language = 'en-US';
    navigator.languages = ['en-US', 'en'];
    navigator.platform = 'Linux x86_64';
    navigator.vendor = 'Google Inc.';
    navigator.userAgentData = {
        brands: [
            { brand: 'Chromium', version: '137' },
            { brand: 'Google Chrome', version: '137' },
            { brand: 'Not/A)Brand', version: '24' }
        ],
        mobile: false,
        platform: 'Linux',
        getHighEntropyValues: function() {
            return Promise.resolve({
                architecture: 'x86', bitness: '64', model: '',
                platformVersion: '6.0.0', uaFullVersion: '137.0.0.0', fullVersionList: []
            });
        }
    };
    navigator.appName = 'Netscape';
    navigator.appVersion = '5.0';
    navigator.product = 'Gecko';
    navigator.cookieEnabled = true;
    navigator.onLine = true;
    navigator.doNotTrack = null;
    navigator.hardwareConcurrency = 8;
    navigator.maxTouchPoints = 0;
    navigator.webdriver = false;
    navigator.deviceMemory = 8;
    navigator.sendBeacon = function() { return true; };
    navigator.plugins = []; navigator.mimeTypes = [];
    navigator.connection = { effectiveType: '4g', downlink: 10, rtt: 50, saveData: false, addEventListener: function() {}, removeEventListener: function() {} };
    navigator.mediaCapabilities = {
        decodingInfo: function() {
            return Promise.resolve({ supported: false, smooth: false, powerEfficient: false });
        }
    };
    navigator.clipboard = {
        writeText: function() { return Promise.resolve(); },
        readText: function() { return Promise.resolve(''); }
    };
    navigator.serviceWorker = {
        register: function() { return Promise.reject(new Error('no service worker support')); },
        getRegistration: function() { return Promise.resolve(undefined); },
        getRegistrations: function() { return Promise.resolve([]); },
        addEventListener: function() {}, removeEventListener: function() {},
        controller: null,
        ready: new Promise(function() {})
    };

    // Called by the runner once the page URL is known.
    globalThis.__aurora_set_location__ = function(href) {
        var u = new URL(href);
        var loc = globalThis.location;
        loc.href = u.href;
        loc.protocol = u.protocol;
        loc.host = u.host;
        loc.hostname = u.hostname;
        loc.port = u.port;
        loc.pathname = u.pathname;
        loc.search = u.search;
        loc.hash = u.hash;
        loc.origin = u.origin;
        loc.ancestorOrigins = [];
        loc.assign = function() {};
        loc.replace = function() {};
        loc.reload = function() {};
        loc.toString = function() { return loc.href; };
        document.URL = u.href;
        document.documentURI = u.href;
        document.domain = u.hostname;
        try { globalThis.origin = u.origin; } catch (e) {}
    };

    // Wire the customElements registry (custom_elements.js) to the document:
    // patch createElement for upgrades and prime elements already in the tree.
    if (typeof globalThis.__aurora_init_custom_elements__ === 'function') {
        globalThis.__aurora_init_custom_elements__();
    }
    if (globalThis.customElements && document.documentElement) {
        customElements.upgrade(document.documentElement);
    }
})();
