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

            globalThis.MessagePort = function MessagePort() {};
            globalThis.MessagePort.prototype.postMessage = function(data) {
                var target = this.__peer__;
                if (!target || target.__closed__) return;
                queueMicrotask(function() {
                    if (target.__closed__) return;
                    var event = new MessageEvent('message', { data: data, origin: '', source: null });
                    event.target = target;
                    event.currentTarget = target;
                    var handler = target.onmessage;
                    if (typeof handler === 'function') {
                        try { handler.call(target, event); } catch (e) { setTimeout(function(){ throw e; }, 0); }
                    }
                    var listeners = target.__listeners || [];
                    for (var i = 0; i < listeners.length; i++) {
                        try { listeners[i].call(target, event); } catch (e) { setTimeout(function(){ throw e; }, 0); }
                    }
                });
            };
            globalThis.MessagePort.prototype.start = function() {};
            globalThis.MessagePort.prototype.close = function() {
                this.__closed__ = true;
                this.onmessage = null;
                this.__listeners = [];
                this.__peer__ = null;
            };
            globalThis.MessagePort.prototype.addEventListener = function(type, listener) {
                if (type !== 'message' || typeof listener !== 'function') return;
                if (!this.__listeners) this.__listeners = [];
                this.__listeners.push(listener);
            };
            globalThis.MessagePort.prototype.removeEventListener = function(type, listener) {
                if (type !== 'message' || !this.__listeners) return;
                this.__listeners = this.__listeners.filter(function(fn) { return fn !== listener; });
            };
            globalThis.MessagePort.prototype.constructor = globalThis.MessagePort;

            globalThis.MessageChannel = function MessageChannel() {
                var port1 = Object.create(globalThis.MessagePort.prototype);
                var port2 = Object.create(globalThis.MessagePort.prototype);
                port1.__peer__ = port2;
                port2.__peer__ = port1;
                port1.onmessage = null;
                port2.onmessage = null;
                port1.__listeners = [];
                port2.__listeners = [];
                port1.__closed__ = false;
                port2.__closed__ = false;
                this.port1 = port1;
                this.port2 = port2;
            };
            globalThis.MessageChannel.prototype.constructor = globalThis.MessageChannel;

            globalThis.PromiseRejectionEvent = function PromiseRejectionEvent(type, init) {
                globalThis.Event.call(this, type, init);
                init = init || {};
                this.promise = init.promise || null; this.reason = init.reason;
            };
            globalThis.PromiseRejectionEvent.prototype = Object.create(globalThis.Event.prototype);
        })();
