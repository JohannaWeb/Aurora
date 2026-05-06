use super::*;

pub(super) fn log_native() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let msg = args
            .iter()
            .map(|v| v.display().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("JS Console: {}", msg);
        Ok(JsValue::undefined())
    })
}

pub(super) fn noop_native() -> NativeFunction {
    NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()))
}

pub(super) fn return_bool(v: bool) -> NativeFunction {
    if v {
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(true)))
    } else {
        NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(false)))
    }
}

pub(super) fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for ch in s.chars() {
        if ch == '-' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// Base64 — minimal self-contained implementation for atob/btoa parity.
pub(super) fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

pub(super) fn base64_decode(input: &str) -> Option<String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&c| c != b'\n' && c != b'\r' && c != b' ')
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = bytes[i + 2];
        let d = bytes[i + 3];
        let n = ((a as u32) << 18) | ((b as u32) << 12);
        out.push(((n >> 16) & 0xff) as u8);
        if c != b'=' {
            let cv = val(c)?;
            let n = n | ((cv as u32) << 6);
            out.push(((n >> 8) & 0xff) as u8);
            if d != b'=' {
                let dv = val(d)?;
                let n = n | (dv as u32);
                out.push((n & 0xff) as u8);
            }
        }
        i += 4;
    }
    String::from_utf8(out).ok()
}

// Unused but kept for API symmetry; silences warnings.
#[allow(dead_code)]
pub(super) fn _keep_types_alive(_: ElementNode) {}
