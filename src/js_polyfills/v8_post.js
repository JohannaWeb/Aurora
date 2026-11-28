// V8-only shims that depend on the shared polyfills (Event from
// event_constructors.js, the customElements registry) and on `document`
// already being installed. Runs last in the V8 bootstrap.
(function() {
    if (globalThis.DOMTokenList && typeof Symbol === 'function' && Symbol.iterator) {
        DOMTokenList.prototype[Symbol.iterator] = Array.prototype[Symbol.iterator];
    }
    // Event subtypes on top of the shared Event polyfill. Captured now, before
    // page scripts (ShadyDOM) can wrap the global; see event_constructors.js.
    var BaseEvent = globalThis.Event;
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
            BaseEvent.call(this, type, init);
            init = init || {};
            this.detail = init.detail !== undefined ? init.detail : 0;
            this.clientX = init.clientX || 0; this.clientY = init.clientY || 0;
            this.x = this.clientX; this.y = this.clientY;
            this.screenX = init.screenX !== undefined ? init.screenX : this.clientX;
            this.screenY = init.screenY !== undefined ? init.screenY : this.clientY;
            this.key = init.key || ''; this.keyCode = init.keyCode || 0;
            this.button = init.button || 0;
            this.buttons = init.buttons || 0;
            this.which = init.which !== undefined ? init.which : (this.button + 1);
            this.ctrlKey = !!init.ctrlKey; this.shiftKey = !!init.shiftKey;
            this.altKey = !!init.altKey; this.metaKey = !!init.metaKey;
            this.relatedTarget = init.relatedTarget || null;
        };
        Ctor.prototype = Object.create(globalThis.Event.prototype);
        Ctor.prototype.constructor = Ctor;
        if (name === 'MouseEvent' || name === 'PointerEvent' || name === 'KeyboardEvent') {
            Ctor.prototype.getModifierState = function(mod) {
                return (mod === 'Shift' && !!this.shiftKey)
                    || (mod === 'Control' && !!this.ctrlKey)
                    || (mod === 'Alt' && !!this.altKey)
                    || (mod === 'Meta' && !!this.metaKey);
            };
        }
        globalThis[name] = Ctor;
    });

    // Document factory and convenience shims over the native bridge.
    document.createElementNS = function(ns, tag) {
        var el = document.createElement(tag);
        var namespace = String(ns || '');
        if (/svg/i.test(namespace)) {
            try { el.namespaceURI = namespace; } catch (e) {}
            try { Object.setPrototypeOf(el, SVGElement.prototype); } catch (e) {}
        }
        return el;
    };
    // document.createComment is native (real Comment nodes, nodeType 8).
    document.createDocumentFragment = function() {
        var fragment = document.createElement('#document-fragment');
        if (typeof globalThis.__aurora_track_fragment__ === 'function') {
            try { globalThis.__aurora_track_fragment__(fragment); } catch (e) {}
        }
        return fragment;
    };
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

    // ShadyDOM renders a ShadyRoot's content as its host's *physical*
    // children: a logical insert into the root ends in a captured-native
    // `host.appendChild/insertBefore` (ShadyDOM copies Node.prototype's
    // methods, i.e. these forwarders, as `__shady_native_*`). Aurora adopts
    // ShadyDOM roots as real shadow roots
    // and renders those, so such content must go into the root; left in the
    // host's light DOM it is never painted (e.g. yt-formatted-string text set
    // via `root.textContent`). Existing light children are left alone: they
    // are distributed through slots.
    function adoptedShadyRootFor(host, node) {
        var root = null;
        try { root = host.shadowRoot; } catch (e) { return null; }
        if (!root || !root.__aurora_registered_shadow_root__ || root.host !== host) return null;
        try { if (node && node.parentNode === host) return null; } catch (e) { return null; }
        return root;
    }

    // The redirect is meant only for ShadyDOM's captured natives. Page code
    // calling `el.appendChild` must get the plain DOM method, as it did while
    // these were own methods of each wrapper, so element wrappers get own
    // aliases of the generated natives (see __aurora_decorate_element__).
    // Remove both with the redirect (prs.md P2.4).
    var unredirectedNodeMethods = {};

    function installPrototypeForward(proto, name) {
        if (!proto || !name) return;
        // Members already on generated bindings live on the prototype itself;
        // the forwarder wraps them instead of looking for an own method.
        var existing = Object.getOwnPropertyDescriptor(proto, name);
        var native = existing && typeof existing.value === 'function' ? existing.value : null;
        var redirectsToShadyRoot = name === 'appendChild' || name === 'insertBefore';
        if (native && (redirectsToShadyRoot || name === 'removeChild')) {
            unredirectedNodeMethods[name] = native;
        }
        var forward = function() {
            // The matching physical removal: ShadyDOM removes a root child
            // from the host, but Aurora keeps it in the adopted root.
            if (name === 'removeChild') {
                var removing = arguments[0];
                var shadyRoot = adoptedShadyRootFor(this, removing);
                if (shadyRoot && removing && removing.parentNode === shadyRoot) {
                    return shadyRoot.removeChild(removing);
                }
            }
            if (redirectsToShadyRoot) {
                var root = adoptedShadyRootFor(this, arguments[0]);
                if (root) {
                    var ref = name === 'insertBefore' ? arguments[1] : null;
                    if (ref && ref.parentNode === root) return root.insertBefore(arguments[0], ref);
                    return root.appendChild(arguments[0]);
                }
            }
            if (native) return native.apply(this, arguments);
            var own = Object.prototype.hasOwnProperty.call(this, name) ? this[name] : null;
            if (typeof own === 'function' && own !== forward) {
                return own.apply(this, arguments);
            }
            return undefined;
        };
        if (native) {
            try { Object.defineProperty(forward, 'length', { value: native.length }); } catch (e) {}
        }
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
        'appendChild', 'insertBefore', 'removeChild'
    ].forEach(function(name) {
        installPrototypeForward(globalThis.Node && Node.prototype, name);
    });
    [
        'insertAdjacentHTML', 'insertAdjacentElement', 'insertAdjacentText'
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

    function elementTarget(node) {
        var target = node;
        while (target && target.nodeType !== 1) {
            target = target.parentNode || (target.nodeType === 11 ? target.host : null);
        }
        return (target && typeof target.dispatchEvent === 'function') ? target : null;
    }

    function firePointerMouseEvent(target, Ctor, type, clientX, clientY, buttons, modifiers) {
        var scrollX = window.scrollX || 0, scrollY = window.scrollY || 0;
        var event = new Ctor(type, {
            bubbles: true, cancelable: true, composed: true, view: window,
            detail: 1, clientX: clientX, clientY: clientY, button: 0, buttons: buttons,
            shiftKey: !!(modifiers & 1), ctrlKey: !!(modifiers & 2),
            altKey: !!(modifiers & 4), metaKey: !!(modifiers & 8)
        });
        event.isTrusted = true;
        event.pageX = clientX + scrollX; event.pageY = clientY + scrollY;
        event.offsetX = 0; event.offsetY = 0;
        if (Ctor === globalThis.PointerEvent) {
            event.pointerId = 1; event.pointerType = 'mouse'; event.isPrimary = true;
            event.width = 1; event.height = 1; event.pressure = buttons ? 0.5 : 0;
        }
        return target.dispatchEvent(event);
    }

    function dispatchMouseDown(target, clientX, clientY, modifiers) {
        var Pointer = globalThis.PointerEvent || globalThis.MouseEvent;
        firePointerMouseEvent(target, Pointer, 'pointerdown', clientX, clientY, 1, modifiers);
        // mousedown's default action moves focus to the clicked element (or
        // its nearest focusable ancestor), or clears it.
        if (firePointerMouseEvent(target, MouseEvent, 'mousedown', clientX, clientY, 1, modifiers)
            && typeof globalThis.__aurora_focus_from_click__ === 'function') {
            globalThis.__aurora_focus_from_click__(target);
        }
    }

    function dispatchMouseUpAndClick(releaseTarget, clickTarget, clientX, clientY, modifiers) {
        var Pointer = globalThis.PointerEvent || globalThis.MouseEvent;
        if (releaseTarget) {
            firePointerMouseEvent(releaseTarget, Pointer, 'pointerup', clientX, clientY, 0, modifiers);
            firePointerMouseEvent(releaseTarget, MouseEvent, 'mouseup', clientX, clientY, 0, modifiers);
        }
        if (!clickTarget) return true;
        var allowed = firePointerMouseEvent(clickTarget, MouseEvent, 'click', clientX, clientY, 0, modifiers);
        if (allowed && typeof globalThis.__aurora_activate__ === 'function') {
            globalThis.__aurora_activate__(clickTarget);
        }
        return allowed;
    }

    globalThis.__aurora_dispatch_mouse_down__ = function(node, clientX, clientY, modifiers) {
        var target = elementTarget(node);
        if (!target) return;
        dispatchMouseDown(target, clientX, clientY, modifiers || 0);
    };

    globalThis.__aurora_dispatch_mouse_up__ = function(releaseNode, clickNode, clientX, clientY, modifiers) {
        var releaseTarget = elementTarget(releaseNode);
        var clickTarget = elementTarget(clickNode);
        dispatchMouseUpAndClick(releaseTarget, clickTarget, clientX, clientY, modifiers || 0);
    };

    // A trusted primary-button click from the window (V8Runtime::dispatch_click):
    // pointer and mouse down/up, then click, at the element that was hit. A
    // hit on a text node targets its parent element. Returns whether click's
    // default action is still allowed.
    globalThis.__aurora_dispatch_click__ = function(node, clientX, clientY) {
        var target = elementTarget(node);
        if (!target) return true;
        dispatchMouseDown(target, clientX, clientY, 0);
        return dispatchMouseUpAndClick(target, target, clientX, clientY, 0);
    };

    // A trusted key event from the window (V8Runtime::dispatch_key), sent to
    // the focused element or the body. A press fires keydown, then keypress for
    // keys that produce text; if neither was cancelled and the target is a
    // text control, the edit is applied natively and `input` fires. Returns
    // bit 1 when the default action was prevented, bit 2 when the target is a
    // text control (which owns the key).
    globalThis.__aurora_dispatch_key__ = function(key, code, keyCode, text, modifiers, released) {
        var target = document.activeElement || document.body;
        if (!target || typeof target.dispatchEvent !== 'function') return 0;
        var edit = globalThis.__aurora_edit_text_input__;
        var editable = typeof edit === 'function' && edit(target, 'check', '');
        function fire(type, charCode) {
            var event = new KeyboardEvent(type, {
                bubbles: true, cancelable: true, composed: true, view: window,
                key: key, keyCode: type === 'keypress' ? charCode : keyCode
            });
            event.isTrusted = true;
            event.code = code;
            event.which = type === 'keypress' ? charCode : keyCode;
            event.charCode = type === 'keypress' ? charCode : 0;
            event.location = 0; event.repeat = false; event.isComposing = false;
            event.shiftKey = !!(modifiers & 1); event.ctrlKey = !!(modifiers & 2);
            event.altKey = !!(modifiers & 4); event.metaKey = !!(modifiers & 8);
            event.getModifierState = function(name) {
                return (name === 'Shift' && event.shiftKey) || (name === 'Control' && event.ctrlKey)
                    || (name === 'Alt' && event.altKey) || (name === 'Meta' && event.metaKey);
            };
            return target.dispatchEvent(event);
        }
        function fireInput(inputType, data) {
            var event = new InputEvent('input', { bubbles: true, cancelable: false, composed: true });
            event.isTrusted = true; event.inputType = inputType; event.data = data; event.isComposing = false;
            target.dispatchEvent(event);
        }
        if (released) return fire('keyup', 0) ? (editable ? 2 : 0) : (1 | (editable ? 2 : 0));
        var allowed = fire('keydown', 0);
        var chord = (modifiers & 2) || (modifiers & 8);
        if (allowed && text && !chord) {
            allowed = fire('keypress', text.charCodeAt(0));
            var printable = text !== '\r' && text !== '\n' && text !== '\t' && text !== '\b';
            if (allowed && editable && printable && edit(target, 'insertText', text)) {
                fireInput('insertText', text);
            }
            // Enter in a text field submits its form (implicit submission).
            if (allowed && editable && key === 'Enter' && typeof globalThis.__aurora_implicit_submit__ === 'function') {
                globalThis.__aurora_implicit_submit__(target);
            }
        } else if (allowed && editable && key === 'Backspace') {
            if (edit(target, 'deleteContentBackward', '')) fireInput('deleteContentBackward', null);
        }
        return (allowed ? 0 : 1) | (editable ? 2 : 0);
    };

    // An image or script finished loading (`loaded` 1) or failed (0).
    globalThis.__aurora_fire_load_event__ = function(el, loaded) {
        var type = loaded ? 'load' : 'error';
        var event = new Event(type, { bubbles: false, cancelable: false });
        event.isTrusted = true;
        el.dispatchEvent(event);
    };

    globalThis.__aurora_set_current_script__ = function(script) {
        document.currentScript = script;
    };

    // Size and position getters backed by layout (`__aurora_metric__`).
    function installMetrics(el) {
        if (el.__aurora_metric_fallbacks__) return;
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
                        // Phase 8.2: prefer the real Blitz/Stylo rendered box
                        // size; fall back to any set value, then the heuristic
                        // (used only for unlaid-out / collapsed elements).
                        var native = 0;
                        try {
                            if (typeof el.__aurora_metric__ === 'function') {
                                native = Number(el.__aurora_metric__(name) || 0);
                            }
                        } catch (e) {}
                        return native || value || fallback();
                    },
                    set: function(next) {
                        value = Number(next || 0);
                    }
                });
            } catch (e) {}
        }
        var zeroFallback = function() { return 0; };
        metric('clientWidth', widthFallback);
        metric('offsetWidth', widthFallback);
        metric('scrollWidth', widthFallback);
        metric('clientHeight', heightFallback);
        metric('offsetHeight', heightFallback);
        metric('scrollHeight', heightFallback);
        // Position accessors read the Blitz/Stylo box origin (document-
        // relative) via the same native bridge; __aurora_metric__ maps
        // offsetTop→y and offsetLeft→x. scrollTop/scrollLeft stay plain
        // settable data properties (element scroll offset, 0 until scrolled).
        metric('offsetTop', zeroFallback);
        metric('offsetLeft', zeroFallback);
    }

    function performElementClick() {
        if (this.hasAttribute && this.hasAttribute('disabled')) return;
        var event = new MouseEvent('click', {
            bubbles: true, cancelable: true, composed: true, view: window, button: 0
        });
        if (typeof this.dispatchEvent === 'function'
            && this.dispatchEvent(event)
            && typeof globalThis.__aurora_activate__ === 'function') {
            globalThis.__aurora_activate__(this);
        }
    }
    if (globalThis.HTMLElement && HTMLElement.prototype) {
        HTMLElement.prototype.click = performElementClick;
    }

    if (typeof globalThis.KeyframeEffect !== 'function') {
        globalThis.KeyframeEffect = function KeyframeEffect(target, keyframes, options) {
            this.target = target || null;
            this.keyframes = keyframes || null;
            this.timing = options || {};
        };
    }
    function createAnimationStub(effect, timeline) {
        var anim = {
            playState: 'finished', currentTime: 0, startTime: 0,
            playbackRate: 1, effect: effect || null, timeline: timeline || null,
            onfinish: null, oncancel: null,
            play: function() {
                anim.playState = 'finished';
                queueMicrotask(function() {
                    if (typeof anim.onfinish === 'function') anim.onfinish({ target: anim });
                });
            },
            pause: function() {}, reverse: function() {},
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
    }
    if (typeof globalThis.Animation !== 'function') {
        globalThis.Animation = function Animation(effect, timeline) {
            return createAnimationStub(effect, timeline);
        };
    }
    if (document && !document.timeline) {
        document.timeline = { currentTime: 0 };
    }

    // Element decoration — called from the native bridge for every element
    // wrapper. Installs per-element JS APIs that are simpler here than native.
    globalThis.__aurora_decorate_element__ = function(el) {
        for (var methodName in unredirectedNodeMethods) {
            try {
                Object.defineProperty(el, methodName, {
                    value: unredirectedNodeMethods[methodName],
                    configurable: true,
                    writable: true
                });
            } catch (e) {}
        }
        // The native classList bridge returns an array-like snapshot; DOMTokenList
        // is iterable, and YouTube spreads it in its element-name diagnostics.
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
        installMetrics(el);
        el.animate = function(keyframes, options) {
            var effect = new KeyframeEffect(el, keyframes, options);
            return createAnimationStub(effect, document.timeline || null);
        };
        el.getAnimations = function() { return []; };
        el.scrollIntoView = function() {};
        el.scrollTo = function() {};
        el.scrollBy = function() {};
        if (typeof el.click !== 'function') {
            el.click = performElementClick;
        }
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
    // `document.elementFromPoint` is provided natively (hit-tests Blitz layout).
    // Only install a null stub if the native binding is missing.
    if (typeof document.elementFromPoint !== 'function') {
        document.elementFromPoint = function() { return null; };
    }
    // The document's own elements get their wrappers before
    // __aurora_decorate_element__ exists, so they need these separately.
    [document.documentElement, document.head, document.body].forEach(function(el) {
        if (el) installMetrics(el);
    });
    document.scrollingElement = document.documentElement || null;
    document.location = globalThis.location;

    // StyleSheetList and CSSStyleSheet wiring.
    (function() {
        var list = new globalThis.StyleSheetList();
        var sheetsMap = new WeakMap();

        function getOrCreateSheet(node) {
            var sheet = sheetsMap.get(node);
            if (!sheet) {
                sheet = new globalThis.CSSStyleSheet();
                sheet.ownerNode = node;
                if (node.localName === 'link') sheet.href = node.getAttribute('href');
                sheetsMap.set(node, sheet);
            }
            return sheet;
        }

        function sync() {
            if (!document.querySelectorAll) return;
            var nodes = document.querySelectorAll('style, link[rel="stylesheet"]');
            // Reset the list length and contents.
            for (var j = 0; j < list.length; j++) delete list[j];
            list.length = 0;

            for (var i = 0; i < nodes.length; i++) {
                var node = nodes[i];
                var sheet = getOrCreateSheet(node);
                list[i] = sheet;
                list.length++;
            }
        }

        Object.defineProperty(document, 'styleSheets', {
            get: function() { sync(); return list; },
            configurable: true
        });

        // Add .sheet property to <style> and <link> elements.
        try {
            Object.defineProperty(globalThis.HTMLStyleElement.prototype, 'sheet', {
                get: function() { return getOrCreateSheet(this); },
                configurable: true
            });
            Object.defineProperty(globalThis.HTMLLinkElement.prototype, 'sheet', {
                get: function() {
                    if (this.rel !== 'stylesheet') return null;
                    return getOrCreateSheet(this);
                },
                configurable: true
            });
        } catch (e) {}
    })();

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
    // The viewport's scroll position, shared with the window (CSSOM View).
    (function() {
        function position(index) {
            var v = typeof globalThis.__aurora_view__ === 'function' ? globalThis.__aurora_view__() : null;
            return v ? v[index] : 0;
        }
        [['scrollX', 2], ['pageXOffset', 2], ['scrollY', 3], ['pageYOffset', 3]].forEach(function(entry) {
            Object.defineProperty(globalThis, entry[0], {
                get: function() { return position(entry[1]); },
                set: function(value) {
                    // Replaceable: assigning shadows the accessor with the value.
                    Object.defineProperty(globalThis, entry[0], {
                        value: value, writable: true, enumerable: true, configurable: true
                    });
                },
                enumerable: true, configurable: true
            });
        });
        // scrollTo(x, y) or scrollTo({left, top, behavior}); scrollBy is relative.
        function target(args, relative) {
            var x = relative ? 0 : position(2), y = relative ? 0 : position(3);
            if (args.length >= 2) {
                x = Number(args[0]) || 0;
                y = Number(args[1]) || 0;
            } else if (args.length === 1 && args[0] !== null && typeof args[0] === 'object') {
                if (args[0].left !== undefined) x = Number(args[0].left) || 0;
                if (args[0].top !== undefined) y = Number(args[0].top) || 0;
            }
            return relative ? [position(2) + x, position(3) + y] : [x, y];
        }
        function scrollTo() {
            var to = target(arguments, false);
            globalThis.__aurora_scroll_to__(to[0], to[1]);
        }
        globalThis.scrollTo = scrollTo;
        globalThis.scroll = scrollTo;
        globalThis.scrollBy = function scrollBy() {
            var to = target(arguments, true);
            globalThis.__aurora_scroll_to__(to[0], to[1]);
        };
        globalThis.__aurora_fire_scroll_event__ = function() {
            var event = new Event('scroll', { bubbles: true, cancelable: false });
            event.isTrusted = true;
            document.dispatchEvent(event);
            // EventTarget here does not know about event handler properties.
            if (typeof globalThis.onscroll === 'function') globalThis.onscroll(event);
        };
    })();
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
        var current = u.href;
        // Assigning `href` navigates, except when only the fragment changes.
        Object.defineProperty(loc, 'href', {
            get: function() { return current; },
            set: function(value) {
                var next;
                try { next = new URL(String(value), current); } catch (e) { return; }
                var stripHash = function(s) { var i = s.indexOf('#'); return i < 0 ? s : s.slice(0, i); };
                if (stripHash(next.href) === stripHash(current) && next.hash) {
                    current = next.href; loc.hash = next.hash; return;
                }
                globalThis.__aurora_navigate__(next.href);
            },
            configurable: true, enumerable: true
        });
        loc.protocol = u.protocol;
        loc.host = u.host;
        loc.hostname = u.hostname;
        loc.port = u.port;
        loc.pathname = u.pathname;
        loc.search = u.search;
        loc.hash = u.hash;
        loc.origin = u.origin;
        loc.ancestorOrigins = [];
        loc.assign = function(url) { loc.href = url; };
        loc.replace = function(url) { loc.href = url; };
        loc.reload = function() { globalThis.__aurora_navigate__(current); };
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

    // CSSStyleSheet & document.styleSheets / adoptedStyleSheets polyfills
    (function() {
        function CSSStyleSheet(ownerNode) {
            this.ownerNode = ownerNode || null;
            this.cssRules = [];
            this.disabled = false;
            this.href = null;
            this.media = {
                mediaText: '',
                length: 0,
                item: function() { return null; },
                appendMedium: function() {},
                deleteMedium: function() {}
            };
            this.title = '';
            this.type = 'text/css';
        }
        CSSStyleSheet.prototype.insertRule = function(rule, index) {
            var rules = this.cssRules;
            index = index === undefined ? 0 : index;
            if (index < 0 || index > rules.length) {
                throw new DOMException('Index out of range', 'IndexSizeError');
            }
            var ruleObj = {
                cssText: rule,
                parentStyleSheet: this,
                parentRule: null,
                selectorText: '',
                style: {
                    cssText: '',
                    getPropertyValue: function() { return ''; },
                    setProperty: function() {},
                    removeProperty: function() {}
                }
            };
            var parts = rule.split('{');
            if (parts.length >= 2) {
                ruleObj.selectorText = parts[0].trim();
                ruleObj.style.cssText = parts[1].split('}')[0].trim();
            }
            rules.splice(index, 0, ruleObj);
            return index;
        };
        CSSStyleSheet.prototype.deleteRule = function(index) {
            var rules = this.cssRules;
            if (index < 0 || index >= rules.length) {
                throw new DOMException('Index out of range', 'IndexSizeError');
            }
            rules.splice(index, 1);
        };
        CSSStyleSheet.prototype.replace = function(text) {
            var self = this;
            return new Promise(function(resolve) {
                self.replaceSync(text);
                resolve(self);
            });
        };
        CSSStyleSheet.prototype.replaceSync = function(text) {
            this.cssRules = [];
            var self = this;
            var rules = text.split('}');
            rules.forEach(function(r) {
                r = r.trim();
                if (r) {
                    try {
                        self.insertRule(r + '}', self.cssRules.length);
                    } catch (e) {}
                }
            });
        };
        globalThis.CSSStyleSheet = CSSStyleSheet;

        var styleSheetsCache = new WeakMap();
        function getOrCreateSheet(el) {
            if (styleSheetsCache.has(el)) {
                return styleSheetsCache.get(el);
            }
            var sheet = new CSSStyleSheet(el);
            if (el.localName === 'style') {
                sheet.replaceSync(el.textContent || '');
            } else if (el.localName === 'link') {
                sheet.href = el.getAttribute('href');
            }
            styleSheetsCache.set(el, sheet);
            return sheet;
        }

        Object.defineProperty(globalThis.Element.prototype, 'sheet', {
            get: function() {
                var tag = this.localName || '';
                if (tag === 'style' || tag === 'link') {
                    return getOrCreateSheet(this);
                }
                return undefined;
            },
            configurable: true
        });

        Object.defineProperty(document, 'styleSheets', {
            get: function() {
                var list = [];
                var styles = document.querySelectorAll('style, link[rel="stylesheet"]');
                for (var i = 0; i < styles.length; i++) {
                    list.push(getOrCreateSheet(styles[i]));
                }
                list.item = function(index) { return this[index]; };
                return list;
            },
            configurable: true
        });

        var adoptedSheetsMap = new WeakMap();
        function getAdoptedStyleSheets(node) {
            if (!adoptedSheetsMap.has(node)) {
                adoptedSheetsMap.set(node, []);
            }
            return adoptedSheetsMap.get(node);
        }
        function setAdoptedStyleSheets(node, value) {
            adoptedSheetsMap.set(node, Array.from(value || []));
        }
        if (globalThis.Document) {
            Object.defineProperty(Document.prototype, 'adoptedStyleSheets', {
                get: function() { return getAdoptedStyleSheets(this); },
                set: function(v) { setAdoptedStyleSheets(this, v); },
                configurable: true
            });
        }
        var targetProto = globalThis.DocumentFragment
            ? DocumentFragment.prototype
            : (globalThis.ShadowRoot ? ShadowRoot.prototype : null);
        if (targetProto && typeof targetProto.Y !== 'function') {
            Object.defineProperty(targetProto, 'Y', {
                value: function() {},
                configurable: true,
                writable: true
            });
        }
        if (globalThis.ShadowRoot) {
            Object.defineProperty(ShadowRoot.prototype, 'adoptedStyleSheets', {
                get: function() { return getAdoptedStyleSheets(this); },
                set: function(v) { setAdoptedStyleSheets(this, v); },
                configurable: true
            });
        }
    })();
})();
