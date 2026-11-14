// P2.0 SPIKE (not for merge). Loaded only with AURORA_SPIKE_PROTOS=1.
// Put Chrome-shaped, configurable accessors on the interface prototypes,
// forwarding to the wrapper's own native accessor. ShadyDOM keys its
// "native accessors available" path (`w.l`) off
// Object.getOwnPropertyDescriptor(Node.prototype, 'firstChild').
(function() {
    var INTERFACES = {
        Node: ['parentNode', 'parentElement', 'childNodes', 'firstChild', 'lastChild',
            'previousSibling', 'nextSibling', 'textContent', 'nodeValue',
            'ownerDocument', 'isConnected', 'baseURI'],
        Element: ['id', 'className', 'classList', 'attributes', 'innerHTML', 'outerHTML',
            'shadowRoot', 'slot', 'assignedSlot', 'children', 'firstElementChild',
            'lastElementChild', 'childElementCount', 'previousElementSibling',
            'nextElementSibling', 'tagName', 'localName', 'namespaceURI',
            'scrollTop', 'scrollLeft', 'scrollWidth', 'scrollHeight',
            'clientTop', 'clientLeft', 'clientWidth', 'clientHeight'],
        HTMLElement: ['hidden', 'innerText', 'style', 'dataset', 'title', 'lang', 'dir',
            'tabIndex', 'offsetParent', 'offsetTop', 'offsetLeft', 'offsetWidth',
            'offsetHeight'],
        CharacterData: ['data', 'length'],
        DocumentFragment: ['children', 'firstElementChild', 'lastElementChild',
            'childElementCount']
    };
    var installed = 0;
    function forwarder(name, proto) {
        function ownDescriptor(obj) {
            try { return Object.getOwnPropertyDescriptor(obj, name); } catch (e) { return null; }
        }
        return {
            get: function() {
                var d = ownDescriptor(this);
                if (!d) return undefined;
                return d.get ? d.get.call(this) : d.value;
            },
            set: function(v) {
                var d = ownDescriptor(this);
                if (!d) {
                    // Native wrapper setup writes data properties with
                    // [[Set]]; keep them own, as before this layer existed.
                    if (this !== proto) {
                        Object.defineProperty(this, name, {
                            value: v, writable: true, configurable: true, enumerable: true
                        });
                    }
                    return;
                }
                if (d.set) d.set.call(this, v);
                else if (d.writable) this[name] = v;
            },
            configurable: true,
            enumerable: true
        };
    }
    Object.keys(INTERFACES).forEach(function(iface) {
        var ctor = globalThis[iface];
        var proto = ctor && ctor.prototype;
        if (!proto) return;
        INTERFACES[iface].forEach(function(name) {
            for (var p = proto; p; p = Object.getPrototypeOf(p)) {
                if (Object.getOwnPropertyDescriptor(p, name)) return;
            }
            try { Object.defineProperty(proto, name, forwarder(name, proto)); installed++; } catch (e) {}
        });
        if (!Object.getOwnPropertyDescriptor(proto, Symbol.toStringTag)) {
            try {
                Object.defineProperty(proto, Symbol.toStringTag, { value: iface, configurable: true });
            } catch (e) {}
        }
    });
    if (globalThis.Element && !Object.getOwnPropertyDescriptor(Element.prototype, 'attachShadow')) {
        Object.defineProperty(Element.prototype, 'attachShadow', {
            value: function(init) {
                var d = Object.getOwnPropertyDescriptor(this, 'attachShadow');
                if (!d || typeof d.value !== 'function') throw new TypeError('Illegal invocation');
                return d.value.call(this, init);
            },
            configurable: true, writable: true
        });
    }
    console.log('[spike] prototype accessors installed: ' + installed);
})();
