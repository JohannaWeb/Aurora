//! Make a replayed page load reproducible from the JS side.
//!
//! Under `AURORA_REPLAY`, `Math.random` uses a fixed V8 seed and `Date`
//! starts at the recording's wall-clock time instead of now. Elapsed time
//! still follows the real clock; a virtual clock is a separate step.

/// The V8 `--random-seed` used under replay.
pub(super) const REPLAY_RANDOM_SEED: i32 = 1;

/// V8 flags for this process, set once before V8 is initialized.
pub(super) fn v8_flags() -> Option<String> {
    crate::fetch::replay::start_time_ms()
        .map(|_| format!("--random-seed={REPLAY_RANDOM_SEED}"))
}

/// Script that shifts `Date` so the page's clock starts at `start_ms`.
///
/// `Date.now()`, `new Date()` and `Date()` read the shifted clock; every
/// other form (`new Date(ms)`, `Date.UTC`, `Date.parse`, the prototype) is
/// the native one, so `instanceof Date` and date arithmetic are unchanged.
pub(super) fn pin_date_script(start_ms: f64) -> String {
    format!(
        r#"(function(start) {{
    var NativeDate = Date;
    var nativeNow = NativeDate.now;
    var offset = start - nativeNow();
    function now() {{ return nativeNow() + offset; }}
    function PinnedDate(a, b, c, d, e, f, g) {{
        if (!new.target) return new NativeDate(now()).toString();
        switch (arguments.length) {{
            case 0: return new NativeDate(now());
            case 1: return new NativeDate(a);
            default: return new NativeDate(a, b, c === undefined ? 1 : c, d || 0, e || 0, f || 0, g || 0);
        }}
    }}
    PinnedDate.prototype = NativeDate.prototype;
    PinnedDate.now = now;
    PinnedDate.UTC = NativeDate.UTC;
    PinnedDate.parse = NativeDate.parse;
    Object.defineProperty(NativeDate.prototype, 'constructor', {{
        value: PinnedDate, writable: true, configurable: true
    }});
    globalThis.Date = PinnedDate;
}})({start_ms});"#
    )
}
