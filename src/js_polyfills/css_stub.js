        (function() {
            globalThis.CSS = {
                supports: function() { return false; },
                escape: function(s) { return String(s); }
            };
        })();
