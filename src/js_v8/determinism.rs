//! Make a replayed page load reproducible from the JS side.
//!
//! Under `AURORA_REPLAY`, `Math.random` uses a fixed V8 seed and `Date`
//! starts at the recording's wall-clock time instead of now, then advances
//! with the virtual event-loop clock (`crate::clock`).

/// The V8 `--random-seed` used under replay.
pub(super) const REPLAY_RANDOM_SEED: i32 = 1;

/// V8 flags for this process, set once before V8 is initialized.
pub(super) fn v8_flags() -> Option<String> {
    crate::fetch::replay::start_time_ms()
        .map(|_| format!("--random-seed={REPLAY_RANDOM_SEED}"))
}

/// Native `__aurora_clock_ms()`: ms the event-loop clock has run. The pin
/// script captures it and deletes the global.
pub(super) fn clock_ms(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    retval.set_double(crate::clock::script_elapsed_ms());
}

/// Script that makes `Date` read `start_ms` plus the event-loop clock.
/// Without a `__aurora_clock_ms` global (unit tests), it shifts the native
/// clock to start at `start_ms` instead.
///
/// `Date.now()`, `new Date()` and `Date()` read the shifted clock; every
/// other form (`new Date(ms)`, `Date.UTC`, `Date.parse`, the prototype) is
/// the native one, so `instanceof Date` and date arithmetic are unchanged.
pub(super) fn pin_date_script(start_ms: f64) -> String {
    format!(
        r#"(function(start) {{
    var NativeDate = Date;
    var nativeNow = NativeDate.now;
    var elapsed = globalThis.__aurora_clock_ms;
    delete globalThis.__aurora_clock_ms;
    var offset = start - nativeNow();
    var now = typeof elapsed === 'function'
        ? function() {{ return Math.floor(start + elapsed()); }}
        : function() {{ return nativeNow() + offset; }};
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
