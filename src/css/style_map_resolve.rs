use std::collections::BTreeMap;

use super::length::parse_length_value;
use super::StyleMap;

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
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_width,
                font_size,
                root_font_size,
                viewport_width,
                0.0,
            )
        })
    }

    pub fn height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("height")?;
        if raw == "auto" || raw.contains("calc(") {
            return None;
        }
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_height,
                font_size,
                root_font_size,
                0.0,
                viewport_height,
            )
        })
    }

    pub fn min_height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("min-height")?;
        if raw == "auto" || raw.contains("calc(") {
            return None;
        }
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_height,
                font_size,
                root_font_size,
                0.0,
                viewport_height,
            )
        })
    }

    pub fn max_height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("max-height")?;
        if raw == "none" || raw.contains("calc(") {
            return None;
        }
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_height,
                font_size,
                root_font_size,
                0.0,
                viewport_height,
            )
        })
    }

    pub fn font_size_resolved(&self, parent_font_size: f32, root_font_size: f32) -> Option<f32> {
        let raw = self.get("font-size")?;
        parse_length_value(raw)
            .map(|lv| lv.to_px(parent_font_size, parent_font_size, root_font_size, 0.0, 0.0))
    }

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
                if let Some(new_value) = self.resolve_single_value(value, ancestors) {
                    resolved.insert(name.clone(), new_value);
                }
            }
        }
        for (name, value) in resolved {
            self.0.insert(name, value);
        }
    }

    fn resolve_single_value(&self, value: &str, ancestors: &[&StyleMap]) -> Option<String> {
        if !value.contains("var(") {
            return Some(value.to_string());
        }

        let mut result = String::new();
        let mut last_end = 0;
        while let Some(start) = value[last_end..].find("var(") {
            let start = last_end + start;
            result.push_str(&value[last_end..start]);
            let content_start = start + 4;
            let Some(end_offset) = value[content_start..].find(')') else {
                break;
            };
            let end = content_start + end_offset;
            let var_expr = value[content_start..end].trim();
            let var_name = var_expr
                .split_once(',')
                .map(|(name, _)| name.trim())
                .unwrap_or(var_expr);
            if let Some(val) = self.lookup_variable(var_name, ancestors) {
                result.push_str(&val);
            } else {
                result.push_str(&value[start..end + 1]);
            }
            last_end = end + 1;
        }
        result.push_str(&value[last_end..]);
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
