use boa_engine::Context;

pub(super) fn install_xhr_and_fetch(context: &mut Context) {
    // ... existing code ...
    context.eval(r#"
        globalThis.URL = function(u, base) {
            var resolved = u;
            if (base && u && !u.match(/^[a-z][a-z0-9+\-.]*:/i)) {
                if (u.startsWith('//')) {
                    var m = base.match(/^([a-z][a-z0-9+\-.]*:)/i);
                    resolved = (m ? m[1] : 'https:') + u;
                } else if (u.startsWith('/')) {
                    var m = base.match(/^([a-z][a-z0-9+\-.]*:\/\/[^/]+)/i);
                    resolved = (m ? m[1] : '') + u;
                } else {
                    resolved = base.replace(/\/[^/]*$/, '/') + u;
                }
            }
            this.href = resolved || '';
            var m = this.href.match(/^([a-z][a-z0-9+\-.]*:)\/\/([^/:?#]*)(?::(\d+))?(\/[^?#]*)?([^#]*)?(#.*)?$/i);
            if (m) {
                this.protocol = (m[1] || '').toLowerCase();
                this.hostname  = m[2] || '';
                this.port      = m[3] || '';
                this.host      = this.hostname + (this.port ? ':' + this.port : '');
                this.pathname  = m[4] || '/';
                this.search    = m[5] || '';
                this.hash      = m[6] || '';
                this.origin    = this.protocol + '//' + this.host;
            } else {
                this.protocol = ''; this.hostname = ''; this.port = '';
                this.host = ''; this.pathname = ''; this.search = '';
                this.hash = ''; this.origin = '';
            }
            this.searchParams = new URLSearchParams(this.search);
            this.toString = function() { return this.href; };
        };
        globalThis.URL.createObjectURL = function() { return ""; };
    "#).ok();
}
