use crate::css::{EdgeSizes, Margin, StyleMap, WhiteSpace};

use super::text_metrics::{line_height_from_styles, measure_text_width};
use super::{LayoutBox, LayoutKind, Rect};

impl LayoutBox {
    pub(in crate::layout) fn layout_text(text: &str, styles: StyleMap, x: f32, y: f32) -> Self {
        let line_height = line_height_from_styles(&styles);

        Self {
            kind: LayoutKind::Text {
                text: text.to_string(),
            },
            rect: Rect {
                x,
                y,
                width: measure_text_width(text, &styles),
                height: line_height,
            },
            styles,
            margin: Margin::zero(),
            border: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            children: Vec::new(),
        }
    }

    fn decode_entities(text: &str) -> String {
        text.replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&apos;", "'")
            .replace("&copy;", "©")
            .replace("&reg;", "®")
            .replace("&trade;", "™")
            .replace("&bull;", "•")
            .replace("&middot;", "·")
            .replace("&ndash;", "–")
            .replace("&mdash;", "—")
    }

    pub(in crate::layout) fn layout_text_fragments(
        text: &str,
        styles: StyleMap,
        x: f32,
        available_width: f32,
        line_x: &mut f32,
        line_y: &mut f32,
        line_height: &mut f32,
        max_line_width: &mut f32,
    ) -> Vec<Self> {
        let mut fragments = Vec::new();
        let text = Self::decode_entities(text);
        let base_line_height = line_height_from_styles(&styles);

        if styles.white_space() == WhiteSpace::NoWrap {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                return fragments;
            }

            let fragment = Self::layout_text(&text, styles, *line_x, *line_y);
            *line_x += fragment.rect.width;
            *line_height = (*line_height).max(base_line_height);
            *max_line_width = max_line_width.max(*line_x - x);
            fragments.push(fragment);
            return fragments;
        }

        let words = text
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();

        if words.is_empty() {
            return fragments;
        }

        let mut current_line = String::new();

        for word in words {
            let candidate = if current_line.is_empty() {
                word.clone()
            } else {
                format!("{} {}", current_line, word)
            };
            let candidate_width = measure_text_width(&candidate, &styles);
            let used_width = *line_x - x;
            let remaining_width = (available_width - used_width).max(1.0);

            if !current_line.is_empty() && candidate_width > remaining_width {
                if !current_line.is_empty() {
                    // and it keeps each laid-out text fragment self-contained.
                    let fragment =
                        Self::layout_text(&current_line, styles.clone(), *line_x, *line_y);
                    *line_x += fragment.rect.width;
                    *max_line_width = max_line_width.max(*line_x - x);
                    fragments.push(fragment);
                }

                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
                current_line = word;
            } else {
                current_line = candidate;
            }
        }

        if !current_line.is_empty() {
            let last_width = measure_text_width(&current_line, &styles);
            if *line_x - x + last_width > available_width && *line_x > x {
                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
            }
            let fragment = Self::layout_text(&current_line, styles.clone(), *line_x, *line_y);
            *line_x += fragment.rect.width;
            *line_height = (*line_height).max(base_line_height);
            *max_line_width = max_line_width.max(*line_x - x);
            fragments.push(fragment);
        }

        fragments
    }
}
