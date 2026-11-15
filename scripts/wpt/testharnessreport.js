// Served in place of WPT's resources/testharnessreport.js. Collects results
// into a global that scripts/wpt/dump.js prints once the page settles.
(function() {
    var state = globalThis.__aurora_wpt__ = { complete: false, harness: null, tests: [] };
    // Aurora's run ends when its event-loop budget does, so testharness's own
    // timeout never fires; unfinished tests are reported as a harness TIMEOUT.
    setup({ explicit_timeout: true, output: false });
    add_result_callback(function(test) {
        state.tests.push({ name: test.name, status: test.status, message: test.message });
    });
    add_completion_callback(function(tests, harness) {
        state.complete = true;
        state.harness = { status: harness.status, message: harness.message };
    });
})();
