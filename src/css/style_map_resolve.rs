use std::collections::BTreeMap;

use super::StyleMap;
use super::calc::{CalcContext, eval_calc};
use super::length::parse_length_value;

#[allow(dead_code)]
impl StyleMap {
    pub fn width_resolved(
        &self,
        available_width: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_width: f32,
    ) -> Option<f32> {
        let raw = self.get("width")?;
        if raw == "auto" {
            return None;
        }
        let ctx = CalcContext {
            available: available_width,
            font_size,
            root_font_size,
            viewport_width,
            viewport_height: 0.0,
        };
        resolve_length(raw, &ctx)
    }

    pub fn height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("height")?;
        if raw == "auto" {
            return None;
        }
        let ctx = CalcContext {
            available: available_height,
            font_size,
            root_font_size,
            viewport_width: 0.0,
            viewport_height,
        };
        resolve_length(raw, &ctx)
    }

    pub fn min_height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("min-height")?;
        if raw == "auto" {
            return None;
        }
        let ctx = CalcContext {
            available: available_height,
            font_size,
            root_font_size,
            viewport_width: 0.0,
            viewport_height,
        };
        resolve_length(raw, &ctx)
    }

    pub fn max_height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("max-height")?;
        if raw == "none" {
            return None;
        }
        let ctx = CalcContext {
            available: available_height,
            font_size,
            root_font_size,
            viewport_width: 0.0,
            viewport_height,
        };
        resolve_length(raw, &ctx)
    }

    pub fn font_size_resolved(&self, parent_font_size: f32, root_font_size: f32) -> Option<f32> {
        let raw = self.get("font-size")?;
        let ctx = CalcContext {
            available: parent_font_size,
            font_size: parent_font_size,
            root_font_size,
            viewport_width: 0.0,
            viewport_height: 0.0,
        };
        resolve_length(raw, &ctx)
    }

    #[allow(dead_code)]
    pub fn font_weight(&self) -> &str {
        self.get("font-weight").unwrap_or("normal")
    }

    pub fn is_bold(&self) -> bool {
        matches!(self.font_weight(), "bold" | "700" | "bolder")
    }

    pub fn font_style(&self) -> &str {
        self.get("font-style").unwrap_or("normal")
    }

    pub fn is_italic(&self) -> bool {
        matches!(self.font_style(), "italic" | "oblique")
    }

    pub fn line_height_px(&self) -> Option<f32> {
        self.get("line-height")
            .and_then(super::length::parse_length_px)
    }

    pub fn text_decoration(&self) -> Option<&str> {
        self.get("text-decoration")
    }

    pub fn opacity(&self) -> f32 {
        self.get("opacity")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn visibility(&self) -> &str {
        self.get("visibility").unwrap_or("visible")
    }

    pub fn resolve_vars(&mut self, ancestors: &[&StyleMap]) {
        let mut resolved = BTreeMap::new();
        for (name, value) in &self.0 {
            if value.contains("var(") {
                if let Some(new_value) = self.resolve_single_value(value, ancestors, 0) {
                    resolved.insert(name.clone(), new_value);
                }
            }
        }
        for (name, value) in resolved {
            self.0.insert(name, value);
        }
    }

    fn resolve_single_value(
        &self,
        value: &str,
        ancestors: &[&StyleMap],
        depth: u8,
    ) -> Option<String> {
        // Guard against circular CSS variable references (e.g. --a: var(--b); --b: var(--a)).
        if depth > 32 {
            return None;
        }
        if !value.contains("var(") {
            return Some(value.to_string());
        }

        let mut result = String::new();
        let mut i = 0;

        while i < value.len() {
            if value[i..].starts_with("var(") {
                let content_start = i + 4;
                // Find matching `)` at depth 0.
                let (end, inner) = find_matching_close(value, content_start);
                // Split name and fallback at the first `,` at depth 0 in inner.
                let (var_name, fallback) = split_var_args(inner);
                let var_name = var_name.trim();

                if let Some(val) = self.lookup_variable(var_name, ancestors) {
                    // Resolved — recursively handle any var() inside the resolved value.
                    if val.contains("var(") {
                        if let Some(resolved) =
                            self.resolve_single_value(&val, ancestors, depth + 1)
                        {
                            result.push_str(&resolved);
                        } else {
                            result.push_str(&val);
                        }
                    } else {
                        result.push_str(&val);
                    }
                } else if let Some(fb) = fallback {
                    // Variable not found — use fallback, which may itself contain var().
                    if let Some(resolved) =
                        self.resolve_single_value(fb.trim(), ancestors, depth + 1)
                    {
                        result.push_str(&resolved);
                    } else {
                        result.push_str(fb.trim());
                    }
                } else {
                    // No fallback — leave the var() as-is.
                    result.push_str(&value[i..=end]);
                }
                i = end + 1;
            } else {
                let ch = value[i..].chars().next().unwrap_or('\0');
                result.push(ch);
                i += ch.len_utf8();
            }
        }
        Some(result)
    }

    fn lookup_variable(&self, name: &str, ancestors: &[&StyleMap]) -> Option<String> {
        if let Some(val) = self.0.get(name) {
            return Some(val.clone());
        }
        for ancestor in ancestors.iter().rev() {
            if let Some(val) = ancestor.0.get(name) {
                return Some(val.clone());
            }
        }
        None
    }
}

/// Find the closing `)` for a block starting at `start` (after the opening `(` was consumed).
/// Returns `(byte_index_of_close, &str_of_inner)`.
fn find_matching_close(s: &str, start: usize) -> (usize, &str) {
    let mut depth = 1usize;
    let mut i = start;
    for ch in s[start..].chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (i, &s[start..i]);
                }
            }
            _ => {}
        }
        i += ch.len_utf8();
    }
    (s.len().saturating_sub(1), &s[start..])
}

/// Split `--var-name, fallback` at the first `,` at paren depth 0.
fn split_var_args(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

/// Resolve a raw CSS length string to px, handling `calc()`, `min()`, `max()`, `clamp()`.
fn resolve_length(raw: &str, ctx: &CalcContext) -> Option<f32> {
    let raw = raw.trim();
    if raw.starts_with("calc(")
        || raw.starts_with("min(")
        || raw.starts_with("max(")
        || raw.starts_with("clamp(")
    {
        // eval_calc handles all math functions via eval_factor
        return eval_calc(raw, ctx);
    }
    parse_length_value(raw).map(|lv| {
        lv.to_px(
            ctx.available,
            ctx.font_size,
            ctx.root_font_size,
            ctx.viewport_width,
            ctx.viewport_height,
        )
    })
}
