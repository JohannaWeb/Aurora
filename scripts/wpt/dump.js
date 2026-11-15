// AURORA_PROBE_JS hook: print the results collected by testharnessreport.js.
(function() {
    var state = globalThis.__aurora_wpt__;
    var report = state
        ? { complete: state.complete, harness: state.harness, tests: state.tests }
        : { complete: false, harness: null, tests: [], missing: true };
    console.log('[wpt] ' + JSON.stringify(report));
})();
