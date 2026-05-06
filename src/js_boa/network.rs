use super::*;

pub(super) fn install_xhr_and_fetch(context: &mut Context) {
    let xhr_polyfill = r#"
        globalThis.XMLHttpRequest = function() {
            this.readyState = 0;
            this.status = 0;
            this.responseText = "";
            this.response = null;
            this.responseType = "";
            this.onreadystatechange = null;
            this.onload = null;
            this.onerror = null;
        };
        globalThis.XMLHttpRequest.prototype.open = function(method, url) {
            this._method = method;
            this._url = url;
            this.readyState = 1;
        };
        globalThis.XMLHttpRequest.prototype.send = function() {
            this.readyState = 4;
            this.status = 0;
            this.responseText = "";
            if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
            if (typeof this.onerror === 'function') this.onerror();
        };
        globalThis.XMLHttpRequest.prototype.setRequestHeader = function() {};
        globalThis.XMLHttpRequest.prototype.getResponseHeader = function() { return null; };
        globalThis.XMLHttpRequest.prototype.getAllResponseHeaders = function() { return ""; };
        globalThis.XMLHttpRequest.prototype.abort = function() {};
        globalThis.XMLHttpRequest.prototype.addEventListener = function() {};
        globalThis.XMLHttpRequest.prototype.removeEventListener = function() {};
        globalThis.XMLHttpRequest.UNSENT = 0;
        globalThis.XMLHttpRequest.OPENED = 1;
        globalThis.XMLHttpRequest.HEADERS_RECEIVED = 2;
        globalThis.XMLHttpRequest.LOADING = 3;
        globalThis.XMLHttpRequest.DONE = 4;

        // fetch returns a Promise that rejects — callers using .catch survive.
        globalThis.fetch = function(url) {
            return Promise.reject(new Error("Aurora: network fetch disabled in JS runtime"));
        };

        // Headers, Request, Response, URL(SearchParams), Blob, FormData, File, FileReader — minimal stubs.
        globalThis.Headers = function(init) {
            var m = {};
            if (init) for (var k in init) m[k.toLowerCase()] = init[k];
            this._m = m;
            this.get = function(k) { return m[(''+k).toLowerCase()] || null; };
            this.set = function(k, v) { m[(''+k).toLowerCase()] = ''+v; };
            this.has = function(k) { return (''+k).toLowerCase() in m; };
            this.append = this.set;
            this.delete = function(k) { delete m[(''+k).toLowerCase()]; };
            this.forEach = function(fn) { for (var k in m) fn(m[k], k, this); };
        };
        globalThis.Request = function(url, init) { this.url = url; this.method = (init && init.method) || 'GET'; };
        globalThis.Response = function(body, init) {
            this.body = body; this.status = (init && init.status) || 200; this.ok = this.status >= 200 && this.status < 300;
            this.text = function() { return Promise.resolve(String(body)); };
            this.json = function() { try { return Promise.resolve(JSON.parse(String(body))); } catch (e) { return Promise.reject(e); } };
            this.arrayBuffer = function() { return Promise.resolve(new ArrayBuffer(0)); };
            this.blob = function() { return Promise.resolve({}); };
        };
        globalThis.URL = function(u, base) {
            this.href = u; this.origin = ''; this.protocol = ''; this.host = ''; this.hostname = '';
            this.port = ''; this.pathname = ''; this.search = ''; this.hash = '';
            this.toString = function() { return this.href; };
        };
        globalThis.URL.createObjectURL = function() { return ""; };
        globalThis.URL.revokeObjectURL = function() {};
        globalThis.URLSearchParams = function(init) {
            var m = {}; if (typeof init === 'string') {
                init.replace(/^\?/, '').split('&').forEach(function(p){ if (!p) return; var i = p.indexOf('='); if (i<0) m[p]=''; else m[p.slice(0,i)] = decodeURIComponent(p.slice(i+1)); });
            }
            this._m = m;
            this.get = function(k){ return k in m ? m[k] : null; };
            this.set = function(k,v){ m[k] = ''+v; };
            this.has = function(k){ return k in m; };
            this.append = this.set;
            this.delete = function(k){ delete m[k]; };
            this.toString = function(){ var o=[]; for (var k in m) o.push(encodeURIComponent(k)+'='+encodeURIComponent(m[k])); return o.join('&'); };
            this.forEach = function(fn){ for (var k in m) fn(m[k], k, this); };
        };
        globalThis.Blob = function(parts, opts) { this.size = 0; this.type = (opts && opts.type) || ''; };
        globalThis.File = function(parts, name, opts) { globalThis.Blob.call(this, parts, opts); this.name = name; };
        globalThis.FormData = function() {
            var m = {};
            this.append = function(k,v){ m[k] = v; };
            this.get = function(k){ return k in m ? m[k] : null; };
            this.has = function(k){ return k in m; };
            this.delete = function(k){ delete m[k]; };
        };
        globalThis.FileReader = function() {
            this.readAsText = function(){};
            this.readAsDataURL = function(){};
            this.readAsArrayBuffer = function(){};
            this.onload = null;
            this.onerror = null;
        };
        globalThis.DOMParser = function() {
            this.parseFromString = function(str, type) {
                return { documentElement: null, body: null, head: null, querySelector: function(){return null;}, querySelectorAll: function(){return [];} };
            };
        };
        globalThis.AbortController = function() {
            this.signal = { aborted: false, addEventListener: function(){}, removeEventListener: function(){} };
            this.abort = function(){ this.signal.aborted = true; };
        };
        globalThis.WebSocket = function() { throw new Error("Aurora: WebSocket not supported"); };
        globalThis.Worker = function() { throw new Error("Aurora: Worker not supported"); };
        globalThis.SharedWorker = function() { throw new Error("Aurora: SharedWorker not supported"); };
    "#;
    let _ = context.eval(Source::from_bytes(xhr_polyfill.as_bytes()));
}
