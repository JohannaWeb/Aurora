//! Basic CSS `calc()` evaluator.
//!
//! Handles expressions of the form:
//!   `calc(100% - 16px)`, `calc(2 * 1.5em)`, `calc((100% - 8px) / 2)`
//!
//! Operator precedence: `*` `/` bind tighter than `+` `-`.
//! All lengths are resolved to px at evaluation time.

use super::length::{parse_length_value, LengthValue};

pub struct CalcContext {
    pub available: f32,
    pub font_size: f32,
    pub root_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl CalcContext {
    fn resolve(&self, lv: LengthValue) -> f32 {
        lv.to_px(
            self.available,
            self.font_size,
            self.root_font_size,
            self.viewport_width,
            self.viewport_height,
        )
    }
}

/// Evaluate a CSS math function string — `calc(...)`, `min(...)`, `max(...)`, `clamp(...)`.
/// Pass the full token including the function name.
pub fn eval_calc(expr: &str, ctx: &CalcContext) -> Option<f32> {
    eval_factor(expr.trim(), ctx)
}

// ─── Recursive descent ────────────────────────────────────────────────────────

/// expr = term (('+' | '-') term)*
fn eval_expr(s: &str, ctx: &CalcContext) -> Option<f32> {
    // Find the rightmost `+` or `-` at paren depth 0, not at position 0
    // (right-most gives us left-to-right evaluation after recursion).
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut split: Option<(usize, u8)> = None;

    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                // Exclude unary minus: the char before must be a digit, %, or closing paren.
                let prev = s[..i].trim_end();
                if !prev.is_empty()
                    && !prev.ends_with(['*', '/', '+', '-', '('])
                {
                    split = Some((i, bytes[i]));
                    break; // rightmost split found
                }
            }
            _ => {}
        }
    }

    if let Some((pos, op)) = split {
        let left = eval_expr(s[..pos].trim(), ctx)?;
        let right = eval_term(s[pos + 1..].trim(), ctx)?;
        return Some(if op == b'+' { left + right } else { left - right });
    }

    eval_term(s, ctx)
}

/// term = factor (('*' | '/') factor)*
fn eval_term(s: &str, ctx: &CalcContext) -> Option<f32> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut split: Option<(usize, u8)> = None;

    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => depth -= 1,
            b'*' | b'/' if depth == 0 => {
                split = Some((i, bytes[i]));
                break;
            }
            _ => {}
        }
    }

    if let Some((pos, op)) = split {
        let left = eval_term(s[..pos].trim(), ctx)?;
        let right = eval_factor(s[pos + 1..].trim(), ctx)?;
        return Some(if op == b'*' { left * right } else {
            if right == 0.0 { return None; }
            left / right
        });
    }

    eval_factor(s, ctx)
}

/// factor = '(' expr ')' | min/max/clamp/calc | length | number
fn eval_factor(s: &str, ctx: &CalcContext) -> Option<f32> {
    let s = s.trim();

    // Parenthesised sub-expression.
    if s.starts_with('(') && s.ends_with(')') {
        return eval_expr(&s[1..s.len() - 1], ctx);
    }

    // min(a, b, …)
    if let Some(args_str) = strip_fn(s, "min") {
        let vals: Vec<f32> = split_args(args_str).iter().filter_map(|a| eval_expr(a, ctx)).collect();
        return vals.into_iter().reduce(f32::min);
    }

    // max(a, b, …)
    if let Some(args_str) = strip_fn(s, "max") {
        let vals: Vec<f32> = split_args(args_str).iter().filter_map(|a| eval_expr(a, ctx)).collect();
        return vals.into_iter().reduce(f32::max);
    }

    // clamp(min, val, max)
    if let Some(args_str) = strip_fn(s, "clamp") {
        let parts: Vec<f32> = split_args(args_str).iter().filter_map(|a| eval_expr(a, ctx)).collect();
        if parts.len() == 3 {
            return Some(parts[1].clamp(parts[0], parts[2]));
        }
        return None;
    }

    // Nested calc().
    if let Some(inner) = strip_fn(s, "calc") {
        return eval_expr(inner, ctx);
    }

    // Length value (px, %, em, rem, vw, vh, …).
    if let Some(lv) = parse_length_value(s) {
        return Some(ctx.resolve(lv));
    }

    // Bare number (used in `2 * 1.5em` etc.).
    s.parse::<f32>().ok()
}

/// Strip `fn_name(` prefix and `)` suffix, returning the inner args string.
fn strip_fn<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}(");
    s.strip_prefix(prefix.as_str())?.strip_suffix(')')
}

/// Split a comma-separated argument list respecting paren depth.
fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].trim());
    args
}
