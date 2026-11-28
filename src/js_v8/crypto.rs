//! Native `Crypto` interface (`globalThis.crypto`).
//!
//! Provides Web Crypto §10.1 `getRandomValues(typedArray)` and
//! `randomUUID()` backed by `rustls`'s `ring` OS CSPRNG (or a deterministic
//! per-context SplitMix64 generator under `AURORA_REPLAY` so replayed runs stay
//! bit-identical).

use std::cell::Cell;
use std::rc::Rc;

use super::dom_exception::throw_dom_exception;
use super::node_create::v8_str;

/// Maximum byte length allowed per `crypto.getRandomValues` call (W3C Web Crypto §10.1).
const MAX_RANDOM_BYTES: usize = 65_536;

/// Seed used when `AURORA_REPLAY` is active so `getRandomValues` stays reproducible
/// across runs of the same recording while producing distinct outputs per call.
const REPLAY_CRYPTO_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

struct IntrinsicCrypto {
    instance: v8::Global<v8::Object>,
    replay_state: Cell<Option<u64>>,
}

/// Install `Crypto` and `globalThis.crypto` in the current V8 context.
pub(super) fn install<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
) {
    let template = v8::FunctionTemplate::new(scope, illegal_constructor);
    let class_name = v8_str(scope, "Crypto");
    template.set_class_name(class_name);
    template.read_only_prototype();

    let proto = template.prototype_template(scope);
    let get_random_values_fn = v8::FunctionTemplate::builder(get_random_values)
        .length(1)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(
        v8_str(scope, "getRandomValues").into(),
        get_random_values_fn.into(),
    );

    let random_uuid_fn = v8::FunctionTemplate::builder(random_uuid)
        .length(0)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "randomUUID").into(), random_uuid_fn.into());

    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        class_name.into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );

    let ctor_fn = template.get_function(scope).expect("Crypto constructor");
    global.set(scope, class_name.into(), ctor_fn.into());

    let inst = template
        .instance_template(scope)
        .new_instance(scope)
        .expect("crypto instance");
    if let Some(proto_val) = ctor_fn.get(scope, v8_str(scope, "prototype").into()) {
        let _ = inst.set_prototype(scope, proto_val);
    }
    global.set(scope, v8_str(scope, "crypto").into(), inst.into());

    let replay_seed = crate::fetch::replay::start_time_ms().map(|_| REPLAY_CRYPTO_SEED);
    scope.get_current_context().set_slot(Rc::new(IntrinsicCrypto {
        instance: v8::Global::new(scope, inst),
        replay_state: Cell::new(replay_seed),
    }));
}

fn illegal_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let msg = v8_str(scope, "Illegal constructor");
    let err = v8::Exception::type_error(scope, msg);
    scope.throw_exception(err);
}

fn require_crypto(scope: &mut v8::PinScope<'_, '_>, this: v8::Local<v8::Object>) -> bool {
    let context = scope.get_current_context();
    let is_valid = context
        .get_slot::<IntrinsicCrypto>()
        .is_some_and(|slot| v8::Local::new(scope, &slot.instance) == this);
    if !is_valid {
        let msg = v8_str(scope, "Illegal invocation");
        let err = v8::Exception::type_error(scope, msg);
        scope.throw_exception(err);
    }
    is_valid
}

fn is_integer_typed_array(value: v8::Local<v8::Value>) -> bool {
    value.is_typed_array()
        && !value.is_float16_array()
        && !value.is_float32_array()
        && !value.is_float64_array()
}

fn next_splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn fill_random_bytes(scope: &mut v8::PinScope<'_, '_>, buf: &mut [u8]) -> bool {
    if let Some(slot) = scope.get_current_context().get_slot::<IntrinsicCrypto>()
        && let Some(mut state) = slot.replay_state.get()
    {
        for chunk in buf.chunks_mut(8) {
            let word = next_splitmix64(&mut state).to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        slot.replay_state.set(Some(state));
        return true;
    }

    if rustls::crypto::ring::default_provider()
        .secure_random
        .fill(buf)
        .is_err()
    {
        throw_dom_exception(
            scope,
            "OperationError",
            "Failed to generate random bytes",
        );
        return false;
    }
    true
}

fn get_random_values(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if !require_crypto(scope, args.this()) {
        return;
    }
    if args.length() < 1 {
        let msg = v8_str(
            scope,
            "Failed to execute 'getRandomValues' on 'Crypto': 1 argument required, but only 0 present.",
        );
        let err = v8::Exception::type_error(scope, msg);
        scope.throw_exception(err);
        return;
    }

    let arg = args.get(0);
    if !is_integer_typed_array(arg) {
        throw_dom_exception(
            scope,
            "TypeMismatchError",
            "Failed to execute 'getRandomValues' on 'Crypto': The provided value is not an integer TypedArray.",
        );
        return;
    }

    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(arg) else {
        throw_dom_exception(
            scope,
            "TypeMismatchError",
            "Failed to execute 'getRandomValues' on 'Crypto': The provided value is not an ArrayBufferView.",
        );
        return;
    };

    let byte_len = view.byte_length();
    if byte_len > MAX_RANDOM_BYTES {
        throw_dom_exception(
            scope,
            "QuotaExceededError",
            "Failed to execute 'getRandomValues' on 'Crypto': The ArrayBufferView's byte length exceeds 65536.",
        );
        return;
    }

    if byte_len > 0 {
        let ptr = view.data();
        if !ptr.is_null() {
            // SAFETY: `byte_len > 0`, `ptr` is non-null, and `view.data()` already
            // accounts for `byte_offset()`, pointing at `byte_len` valid writable bytes.
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr.cast::<u8>(), byte_len) };
            if !fill_random_bytes(scope, slice) {
                return;
            }
        }
    }

    retval.set(arg);
}

fn random_uuid(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if !require_crypto(scope, args.this()) {
        return;
    }

    let mut b = [0u8; 16];
    if !fill_random_bytes(scope, &mut b) {
        return;
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let uuid = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15]
    );
    retval.set(v8_str(scope, &uuid).into());
}

#[cfg(test)]
mod tests {
    use super::super::V8Runtime;
    use crate::html::Parser;

    fn eval(source: &str) -> String {
        let mut runtime = V8Runtime::new(Parser::new("<html><body></body></html>").parse_document());
        runtime.eval_to_string(source).unwrap()
    }

    #[test]
    fn crypto_interface_and_brand_checks() {
        assert_eq!(
            eval(
                r#"(() => {
                    const checks = [
                        crypto instanceof Crypto,
                        Object.getPrototypeOf(crypto) === Crypto.prototype,
                        Object.prototype.toString.call(crypto) === '[object Crypto]',
                        window.crypto === crypto,
                        self.crypto === crypto,
                        crypto.getRandomValues.length,
                        crypto.randomUUID.length,
                    ];
                    const errs = [
                        () => new Crypto(),
                        () => crypto.getRandomValues.call({}, new Uint8Array(4)),
                        () => crypto.randomUUID.call({}),
                        () => crypto.getRandomValues(),
                    ].map(fn => {
                        try { fn(); return 'ok'; } catch (e) { return e.constructor.name; }
                    });
                    return checks.concat(errs).join('|');
                })()"#
            ),
            "true|true|true|true|true|1|0|TypeError|TypeError|TypeError|TypeError"
        );
    }

    #[test]
    fn get_random_values_fills_integer_typed_arrays_and_subarrays() {
        assert_eq!(
            eval(
                r#"(() => {
                    const arrays = [
                        new Int8Array(32),
                        new Uint8Array(32),
                        new Uint8ClampedArray(32),
                        new Int16Array(16),
                        new Uint16Array(16),
                        new Int32Array(8),
                        new Uint32Array(8),
                        new BigInt64Array(4),
                        new BigUint64Array(4),
                    ];
                    const same = arrays.every(a => crypto.getRandomValues(a) === a);
                    const nonZero = arrays.every(a => {
                        const bytes = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
                        return bytes.some(b => b !== 0);
                    });

                    // Subarray view only touches [byteOffset .. byteOffset + byteLength].
                    const backing = new Uint8Array(24);
                    const sub = new Uint8Array(backing.buffer, 4, 16);
                    const ret = crypto.getRandomValues(sub);
                    const prefixZero = backing.slice(0, 4).every(b => b === 0);
                    const suffixZero = backing.slice(20).every(b => b === 0);
                    const midChanged = backing.slice(4, 20).some(b => b !== 0);

                    // Zero-length and exact 65536-byte limit succeed.
                    const empty = new Uint8Array(0);
                    const max = new Uint8Array(65536);
                    const emptyOk = crypto.getRandomValues(empty) === empty;
                    const maxOk = crypto.getRandomValues(max) === max && max.some(b => b !== 0);

                    return [same, nonZero, ret === sub, prefixZero, suffixZero, midChanged, emptyOk, maxOk].join('|');
                })()"#
            ),
            "true|true|true|true|true|true|true|true"
        );
    }

    #[test]
    fn get_random_values_rejects_invalid_views_and_oversized_arrays() {
        assert_eq!(
            eval(
                r#"(() => {
                    const invalid = [
                        null,
                        undefined,
                        42,
                        {},
                        [],
                        new ArrayBuffer(16),
                        new DataView(new ArrayBuffer(16)),
                        new Float32Array(4),
                        new Float64Array(4),
                    ].map(v => {
                        try {
                            crypto.getRandomValues(v);
                            return 'ok';
                        } catch (e) {
                            return `${e instanceof DOMException}:${e.name}:${e.code}`;
                        }
                    });
                    let quota = 'ok';
                    try {
                        crypto.getRandomValues(new Uint8Array(65537));
                    } catch (e) {
                        quota = `${e instanceof DOMException}:${e.name}:${e.code}`;
                    }
                    return invalid.concat(quota).join('|');
                })()"#
            ),
            "true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:TypeMismatchError:17|true:QuotaExceededError:22"
        );
    }

    #[test]
    fn random_uuid_matches_rfc4122_v4_and_is_unique() {
        assert_eq!(
            eval(
                r#"(() => {
                    const re = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
                    const a = crypto.randomUUID();
                    const b = crypto.randomUUID();
                    return [re.test(a), re.test(b), a !== b, a.length === 36].join('|');
                })()"#
            ),
            "true|true|true|true"
        );
    }
}
