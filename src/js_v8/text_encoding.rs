//! Native `TextEncoder` and `TextDecoder` (WHATWG Encoding Standard).
//!
//! Replaces the partial UTF-8 JS `TextEncoder` polyfill in `v8_base.js` and
//! adds stateful streaming `TextDecoder` backed by Rust (UTF-8, UTF-16LE,
//! UTF-16BE, and Windows-1252 / Latin-1 labels).

use super::node_create::v8_str;
use super::registry::NodeRegistry;
use std::rc::Rc;

const WINDOWS_1252_C1: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncodingKind {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl EncodingKind {
    fn from_label(raw: &str) -> Option<Self> {
        let label = raw.trim().to_ascii_lowercase();
        match label.as_str() {
            "utf-8" | "utf8" | "unicode-1-1-utf-8" | "unicode11utf8" | "unicode20utf8"
            | "x-unicode20utf8" => Some(Self::Utf8),
            "utf-16" | "utf-16le" | "ucs-2" | "iso-10646-ucs-2" | "unicode" | "unicodefeff"
            | "csunicode" => Some(Self::Utf16Le),
            "utf-16be" | "unicodefffe" => Some(Self::Utf16Be),
            "windows-1252" | "latin1" | "iso-8859-1" | "iso_8859-1" | "iso8859-1"
            | "iso88591" | "iso_8859-1:1987" | "iso-ir-100" | "l1" | "csisolatin1"
            | "cp1252" | "x-cp1252" | "ascii" | "us-ascii" | "ansi_x3.4-1968"
            | "ansi_x3.4-1986" | "iso646-us" | "iso-ir-6" | "us" | "csascii"
            | "cp367" | "ibm367" | "cp819" | "ibm819" => Some(Self::Windows1252),
            _ => None,
        }
    }

    fn canonical_name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Windows1252 => "windows-1252",
        }
    }
}

pub(super) struct DecoderState {
    encoding: EncodingKind,
    fatal: bool,
    ignore_bom: bool,
    bom_seen: bool,
    pending: [u8; 4],
    pending_len: usize,
    utf8_needed: usize,
    utf8_lower_bound: u8,
    utf8_upper_bound: u8,
    utf16_lead_surrogate: Option<u16>,
}

impl DecoderState {
    fn new(encoding: EncodingKind, fatal: bool, ignore_bom: bool) -> Self {
        Self {
            encoding,
            fatal,
            ignore_bom,
            bom_seen: false,
            pending: [0; 4],
            pending_len: 0,
            utf8_needed: 0,
            utf8_lower_bound: 0x80,
            utf8_upper_bound: 0xBF,
            utf16_lead_surrogate: None,
        }
    }

    fn reset(&mut self) {
        self.bom_seen = false;
        self.pending_len = 0;
        self.utf8_needed = 0;
        self.utf8_lower_bound = 0x80;
        self.utf8_upper_bound = 0xBF;
        self.utf16_lead_surrogate = None;
    }

    fn push_pending(&mut self, byte: u8) {
        self.pending[self.pending_len] = byte;
        self.pending_len += 1;
    }

    fn fail_or_replace(&self, out: &mut String) -> Result<(), &'static str> {
        if self.fatal {
            Err("The encoded data was not valid.")
        } else {
            out.push('\u{FFFD}');
            Ok(())
        }
    }

    fn decode(&mut self, input: &[u8], stream: bool) -> Result<String, &'static str> {
        let mut out = String::new();
        match self.encoding {
            EncodingKind::Windows1252 => {
                for &b in input {
                    if b < 0x80 {
                        out.push(b as char);
                    } else if b < 0xA0 {
                        out.push(WINDOWS_1252_C1[(b - 0x80) as usize]);
                    } else {
                        out.push(char::from(b));
                    }
                }
            }
            EncodingKind::Utf8 => self.decode_utf8(input, stream, &mut out)?,
            EncodingKind::Utf16Le => self.decode_utf16(input, stream, true, &mut out)?,
            EncodingKind::Utf16Be => self.decode_utf16(input, stream, false, &mut out)?,
        }
        if !stream {
            self.reset();
        }
        Ok(out)
    }

    fn decode_utf8(&mut self, input: &[u8], stream: bool, out: &mut String) -> Result<(), &'static str> {
        for &b in input {
            self.push_utf8_byte(b, out)?;
        }
        if !stream && self.utf8_needed != 0 {
            self.pending_len = 0;
            self.utf8_needed = 0;
            self.utf8_lower_bound = 0x80;
            self.utf8_upper_bound = 0xBF;
            self.fail_or_replace(out)?;
        }
        if !self.ignore_bom && !self.bom_seen && !out.is_empty() {
            if let Some(rest) = out.strip_prefix('\u{FEFF}') {
                *out = rest.to_owned();
            }
            self.bom_seen = true;
        }
        Ok(())
    }

    fn push_utf8_byte(&mut self, b: u8, out: &mut String) -> Result<(), &'static str> {
        if self.utf8_needed == 0 {
            match b {
                0x00..=0x7F => {
                    out.push(b as char);
                }
                0xC2..=0xDF => {
                    self.utf8_needed = 1;
                    self.push_pending(b);
                }
                0xE0..=0xEF => {
                    self.utf8_needed = 2;
                    self.utf8_lower_bound = if b == 0xE0 { 0xA0 } else { 0x80 };
                    self.utf8_upper_bound = if b == 0xED { 0x9F } else { 0xBF };
                    self.push_pending(b);
                }
                0xF0..=0xF4 => {
                    self.utf8_needed = 3;
                    self.utf8_lower_bound = if b == 0xF0 { 0x90 } else { 0x80 };
                    self.utf8_upper_bound = if b == 0xF4 { 0x8F } else { 0xBF };
                    self.push_pending(b);
                }
                _ => self.fail_or_replace(out)?,
            }
            return Ok(());
        }

        if b < self.utf8_lower_bound || b > self.utf8_upper_bound {
            self.pending_len = 0;
            self.utf8_needed = 0;
            self.utf8_lower_bound = 0x80;
            self.utf8_upper_bound = 0xBF;
            self.fail_or_replace(out)?;
            // Reprocess the current byte as a fresh lead byte (maximal subpart).
            return self.push_utf8_byte(b, out);
        }

        self.utf8_lower_bound = 0x80;
        self.utf8_upper_bound = 0xBF;
        self.push_pending(b);
        if self.pending_len <= self.utf8_needed {
            return Ok(());
        }

        let s = std::str::from_utf8(&self.pending[..self.pending_len])
            .map_err(|_| "The encoded data was not valid.")?;
        out.push_str(s);
        self.pending_len = 0;
        self.utf8_needed = 0;
        Ok(())
    }

    fn decode_utf16(
        &mut self,
        input: &[u8],
        stream: bool,
        little_endian: bool,
        out: &mut String,
    ) -> Result<(), &'static str> {
        for &b in input {
            self.push_pending(b);
            if self.pending_len < 2 {
                continue;
            }
            let unit = if little_endian {
                u16::from_le_bytes([self.pending[0], self.pending[1]])
            } else {
                u16::from_be_bytes([self.pending[0], self.pending[1]])
            };
            self.pending_len = 0;

            if let Some(lead) = self.utf16_lead_surrogate.take() {
                if (0xDC00..=0xDFFF).contains(&unit) {
                    let cp = 0x10000 + (((lead as u32 - 0xD800) << 10) | (unit as u32 - 0xDC00));
                    if let Some(ch) = char::from_u32(cp) {
                        self.push_utf16_char(ch, out);
                    }
                    continue;
                }
                self.fail_or_replace(out)?;
            }

            if (0xD800..=0xDBFF).contains(&unit) {
                self.utf16_lead_surrogate = Some(unit);
            } else if (0xDC00..=0xDFFF).contains(&unit) {
                self.fail_or_replace(out)?;
            } else if let Some(ch) = char::from_u32(unit as u32) {
                self.push_utf16_char(ch, out);
            }
        }

        if !stream {
            if self.pending_len > 0 {
                self.pending_len = 0;
                self.fail_or_replace(out)?;
            }
            if self.utf16_lead_surrogate.take().is_some() {
                self.fail_or_replace(out)?;
            }
        }
        Ok(())
    }

    fn push_utf16_char(&mut self, ch: char, out: &mut String) {
        if !self.bom_seen {
            self.bom_seen = true;
            if !self.ignore_bom && ch == '\u{FEFF}' {
                return;
            }
        }
        out.push(ch);
    }
}

fn encoder_key<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Private> {
    let name = v8_str(scope, "aurora.TextEncoder");
    v8::Private::for_api(scope, Some(name))
}

fn decoder_id_key<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Private> {
    let name = v8_str(scope, "aurora.TextDecoder.id");
    v8::Private::for_api(scope, Some(name))
}

fn throw_type_error(scope: &mut v8::PinScope<'_, '_>, msg: &str) {
    let msg = v8_str(scope, msg);
    let err = v8::Exception::type_error(scope, msg);
    scope.throw_exception(err);
}

fn throw_range_error(scope: &mut v8::PinScope<'_, '_>, msg: &str) {
    let msg = v8_str(scope, msg);
    let err = v8::Exception::range_error(scope, msg);
    scope.throw_exception(err);
}

fn install_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    proto: v8::Local<'s, v8::ObjectTemplate>,
    name: &str,
    getter_fn: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let getter = v8::FunctionTemplate::builder(getter_fn)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    getter.set_class_name(v8_str(scope, &format!("get {name}")));
    getter.remove_prototype();
    proto.set_accessor_property(
        v8_str(scope, name).into(),
        Some(getter),
        None,
        v8::PropertyAttribute::NONE,
    );
}

/// Install `TextEncoder` and `TextDecoder` constructors on `globalThis`.
pub(super) fn install(scope: &mut v8::PinScope<'_, '_>) {
    install_encoder(scope);
    install_decoder(scope);
}

fn install_encoder(scope: &mut v8::PinScope<'_, '_>) {
    let template = v8::FunctionTemplate::new(scope, construct_encoder);
    let name = v8_str(scope, "TextEncoder");
    template.set_class_name(name);
    template.read_only_prototype();
    let proto = template.prototype_template(scope);
    install_accessor(scope, proto, "encoding", encoder_encoding);
    let encode_fn = v8::FunctionTemplate::builder(encoder_encode)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "encode").into(), encode_fn.into());
    let encode_into_fn = v8::FunctionTemplate::builder(encoder_encode_into)
        .length(2)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "encodeInto").into(), encode_into_fn.into());
    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        name.into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );

    let ctor = template.get_function(scope).expect("TextEncoder constructor");
    scope
        .get_current_context()
        .global(scope)
        .define_own_property(scope, name.into(), ctor.into(), v8::PropertyAttribute::DONT_ENUM);
}

fn install_decoder(scope: &mut v8::PinScope<'_, '_>) {
    let template = v8::FunctionTemplate::new(scope, construct_decoder);
    let name = v8_str(scope, "TextDecoder");
    template.set_class_name(name);
    template.read_only_prototype();
    let proto = template.prototype_template(scope);
    install_accessor(scope, proto, "encoding", decoder_encoding);
    install_accessor(scope, proto, "fatal", decoder_fatal);
    install_accessor(scope, proto, "ignoreBOM", decoder_ignore_bom);
    let decode_fn = v8::FunctionTemplate::builder(decoder_decode)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "decode").into(), decode_fn.into());
    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        name.into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );

    let ctor = template.get_function(scope).expect("TextDecoder constructor");
    scope
        .get_current_context()
        .global(scope)
        .define_own_property(scope, name.into(), ctor.into(), v8::PropertyAttribute::DONT_ENUM);
}

fn construct_encoder(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !args.is_construct_call() {
        throw_type_error(scope, "Failed to construct 'TextEncoder': Please use the 'new' operator.");
        return;
    }
    let this = args.this();
    let key = encoder_key(scope);
    let flag = v8::Boolean::new(scope, true);
    this.set_private(scope, key, flag.into());
    rv.set(this.into());
}

fn require_encoder(scope: &mut v8::PinScope<'_, '_>, this: v8::Local<v8::Object>) -> bool {
    let key = encoder_key(scope);
    let ok = this
        .get_private(scope, key)
        .is_some_and(|v| v.boolean_value(scope));
    if !ok {
        throw_type_error(scope, "Illegal invocation");
    }
    ok
}

fn encoder_encoding(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !require_encoder(scope, args.this()) {
        return;
    }
    rv.set(v8_str(scope, "utf-8").into());
}

fn encoder_encode(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !require_encoder(scope, args.this()) {
        return;
    }
    let text = if args.get(0).is_undefined() {
        String::new()
    } else {
        let Some(s) = args.get(0).to_string(scope) else {
            return;
        };
        s.to_rust_string_lossy(scope)
    };
    let bytes = text.as_bytes();
    let ab = v8::ArrayBuffer::new(scope, bytes.len());
    if !bytes.is_empty()
        && let Some(ptr) = ab.data()
    {
        // SAFETY: `ab` was just created with exact capacity `bytes.len()`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr().cast::<u8>(), bytes.len()) };
    }
    if let Some(arr) = v8::Uint8Array::new(scope, ab, 0, bytes.len()) {
        rv.set(arr.into());
    }
}

fn encoder_encode_into(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !require_encoder(scope, args.this()) {
        return;
    }
    if args.length() < 2 {
        throw_type_error(scope, "TextEncoder.encodeInto: 2 arguments required.");
        return;
    }
    if args.get(0).is_undefined() {
        encode_into_units(scope, &[], args.get(1), &mut rv);
        return;
    }
    let Some(s) = args.get(0).to_string(scope) else {
        return;
    };
    let mut units = vec![0u16; s.length()];
    if !units.is_empty() {
        s.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
    }
    encode_into_units(scope, &units, args.get(1), &mut rv);
}

fn encode_into_units(
    scope: &mut v8::PinScope<'_, '_>,
    units: &[u16],
    dest_val: v8::Local<v8::Value>,
    rv: &mut v8::ReturnValue,
) {
    let Ok(dest) = v8::Local::<v8::Uint8Array>::try_from(dest_val) else {
        throw_type_error(scope, "TextEncoder.encodeInto: destination must be a Uint8Array.");
        return;
    };
    let cap = dest.byte_length();
    let detached = dest.has_buffer() && dest.buffer(scope).is_some_and(|b| b.was_detached());
    let dst = if cap > 0 && !detached && !dest.data().is_null() {
        // SAFETY: `dest` is an attached Uint8Array whose backing store has at least `cap` bytes.
        unsafe { std::slice::from_raw_parts_mut(dest.data().cast::<u8>(), cap) }
    } else {
        &mut []
    };

    let mut read = 0usize;
    let mut written = 0usize;
    let mut tmp = [0u8; 4];
    for decoded in char::decode_utf16(units.iter().copied()) {
        let (ch, utf16_len) = match decoded {
            Ok(ch) => (ch, ch.len_utf16()),
            Err(_) => ('\u{FFFD}', 1),
        };
        let encoded = ch.encode_utf8(&mut tmp);
        if written + encoded.len() > dst.len() {
            break;
        }
        dst[written..written + encoded.len()].copy_from_slice(encoded.as_bytes());
        written += encoded.len();
        read += utf16_len;
    }

    let result = v8::Object::new(scope);
    result.set(scope, v8_str(scope, "read").into(), v8::Number::new(scope, read as f64).into());
    result.set(
        scope,
        v8_str(scope, "written").into(),
        v8::Number::new(scope, written as f64).into(),
    );
    rv.set(result.into());
}

fn construct_decoder(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !args.is_construct_call() {
        throw_type_error(scope, "Failed to construct 'TextDecoder': Please use the 'new' operator.");
        return;
    }
    let Some(registry) = scope.get_current_context().get_slot::<NodeRegistry>() else {
        return;
    };
    let label = if args.get(0).is_undefined() {
        "utf-8".to_string()
    } else {
        let Some(s) = args.get(0).to_string(scope) else {
            return;
        };
        s.to_rust_string_lossy(scope)
    };
    let Some(encoding) = EncodingKind::from_label(&label) else {
        throw_range_error(
            scope,
            &format!("Failed to construct 'TextDecoder': The encoding label provided ('{label}') is invalid."),
        );
        return;
    };

    let mut fatal = false;
    let mut ignore_bom = false;
    let opts_val = args.get(1);
    if !opts_val.is_undefined() && !opts_val.is_null() {
        if !opts_val.is_object() {
            throw_type_error(scope, "Failed to construct 'TextDecoder': parameter 2 is not of type 'TextDecoderOptions'.");
            return;
        }
        let Some(opts) = opts_val.to_object(scope) else {
            return;
        };
        if let Some(v) = opts.get(scope, v8_str(scope, "fatal").into()) {
            fatal = v.boolean_value(scope);
        }
        if let Some(v) = opts.get(scope, v8_str(scope, "ignoreBOM").into()) {
            ignore_bom = v.boolean_value(scope);
        }
    }

    let id = {
        let mut decoders = registry.text_decoders.borrow_mut();
        let id = decoders.len();
        decoders.push(DecoderState::new(encoding, fatal, ignore_bom));
        id
    };
    let this = args.this();
    let key = decoder_id_key(scope);
    let id_val = v8::Integer::new_from_unsigned(scope, id as u32);
    this.set_private(scope, key, id_val.into());
    rv.set(this.into());
}

fn decoder_id(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &NodeRegistry,
    this: v8::Local<v8::Object>,
) -> Option<usize> {
    let key = decoder_id_key(scope);
    let idx = this
        .get_private(scope, key)
        .and_then(|v| v.uint32_value(scope))
        .map(|id| id as usize)
        .filter(|&id| id < registry.text_decoders.borrow().len());
    if idx.is_none() {
        throw_type_error(scope, "Illegal invocation");
    }
    idx
}

fn with_decoder<R>(
    scope: &mut v8::PinScope<'_, '_>,
    this: v8::Local<v8::Object>,
    f: impl FnOnce(&Rc<NodeRegistry>, &mut DecoderState) -> R,
) -> Option<R> {
    let registry = scope.get_current_context().get_slot::<NodeRegistry>()?;
    let id = decoder_id(scope, &registry, this)?;
    let mut decoders = registry.text_decoders.borrow_mut();
    Some(f(&registry, &mut decoders[id]))
}

fn decoder_encoding(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if let Some(name) = with_decoder(scope, args.this(), |_, d| d.encoding.canonical_name()) {
        rv.set(v8_str(scope, name).into());
    }
}

fn decoder_fatal(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if let Some(fatal) = with_decoder(scope, args.this(), |_, d| d.fatal) {
        rv.set(v8::Boolean::new(scope, fatal).into());
    }
}

fn decoder_ignore_bom(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if let Some(ignore) = with_decoder(scope, args.this(), |_, d| d.ignore_bom) {
        rv.set(v8::Boolean::new(scope, ignore).into());
    }
}

fn extract_input_bytes(
    scope: &mut v8::PinScope<'_, '_>,
    val: v8::Local<v8::Value>,
) -> Result<Vec<u8>, ()> {
    if val.is_undefined() {
        return Ok(Vec::new());
    }
    if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(val) {
        let len = view.byte_length();
        let mut bytes = vec![0u8; len];
        if len > 0 {
            let copied = view.copy_contents(&mut bytes);
            bytes.truncate(copied);
        }
        return Ok(bytes);
    }
    if let Ok(ab) = v8::Local::<v8::ArrayBuffer>::try_from(val) {
        let len = ab.byte_length();
        if len == 0 || ab.was_detached() {
            return Ok(Vec::new());
        }
        let Some(ptr) = ab.data() else {
            return Ok(Vec::new());
        };
        let mut bytes = vec![0u8; len];
        // SAFETY: `ab` is live and non-detached with `len` bytes at `ptr`.
        unsafe { std::ptr::copy_nonoverlapping(ptr.as_ptr().cast::<u8>(), bytes.as_mut_ptr(), len) };
        return Ok(bytes);
    }
    if let Ok(sab) = v8::Local::<v8::SharedArrayBuffer>::try_from(val) {
        let len = sab.byte_length();
        let store = sab.get_backing_store();
        let Some(ptr) = store.data() else {
            return Ok(Vec::new());
        };
        let mut bytes = vec![0u8; len];
        // SAFETY: `sab` backing store is live with `len` bytes at `ptr`.
        unsafe { std::ptr::copy_nonoverlapping(ptr.as_ptr().cast::<u8>(), bytes.as_mut_ptr(), len) };
        return Ok(bytes);
    }
    throw_type_error(
        scope,
        "Failed to execute 'decode' on 'TextDecoder': The provided value is not of type '(ArrayBuffer or ArrayBufferView)'.",
    );
    Err(())
}

fn decoder_decode(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(registry) = scope.get_current_context().get_slot::<NodeRegistry>() else {
        return;
    };
    let Some(id) = decoder_id(scope, &registry, args.this()) else {
        return;
    };

    let mut stream = false;
    let opts_val = args.get(1);
    if !opts_val.is_undefined() && !opts_val.is_null() {
        if !opts_val.is_object() {
            throw_type_error(scope, "Failed to execute 'decode' on 'TextDecoder': parameter 2 is not of type 'TextDecodeOptions'.");
            return;
        }
        let Some(opts) = opts_val.to_object(scope) else {
            return;
        };
        if let Some(v) = opts.get(scope, v8_str(scope, "stream").into()) {
            stream = v.boolean_value(scope);
        }
    }

    let Ok(bytes) = extract_input_bytes(scope, args.get(0)) else {
        return;
    };
    let result = registry.text_decoders.borrow_mut()[id].decode(&bytes, stream);
    match result {
        Ok(decoded) => rv.set(v8_str(scope, &decoded).into()),
        Err(msg) => throw_type_error(scope, msg),
    }
}
