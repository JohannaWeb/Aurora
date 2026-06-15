        (function() {
            function makeTrusted(val) { return { toString: function(){ return val; } }; }
            globalThis.trustedTypes = {
                createPolicy: function(name, rules) {
                    return {
                        name: name,
                        createHTML: function(s) { return makeTrusted(rules && rules.createHTML ? rules.createHTML(s) : s); },
                        createScript: function(s) { return makeTrusted(rules && rules.createScript ? rules.createScript(s) : s); },
                        createScriptURL: function(s) { return makeTrusted(rules && rules.createScriptURL ? rules.createScriptURL(s) : s); }
                    };
                },
                getAttributeType: function() { return null; },
                getPropertyType: function() { return null; },
                isHTML: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScript: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                isScriptURL: function(v) { return v && typeof v === 'object' && 'toString' in v; },
                emptyHTML: makeTrusted(''),
                emptyScript: makeTrusted(''),
                defaultPolicy: null
            };
        })();
