use parley::FontContext;

use crate::css::StyleMap;

use super::{LayoutBox, LayoutKind, Rect};
use crate::css::{EdgeSizes, Margin, WhiteSpace};

/// Create a FontContext for text layout.
/// In Phase 5 (reflow), this should be stored on the document and reused.
/// For now, create per-call (not optimal, but functional).
pub fn create_font_context() -> FontContext {
    FontContext::default()
}

/// Layout text using Parley, with support for word wrapping and line breaking.
/// Replaces the custom text wrapping in inline_text.rs.
pub fn layout_text_with_parley(
    node: Option<crate::dom::NodePtr>,
    text: &str,
    styles: &StyleMap,
    x: f32,
    y: f32,
    available_width: f32,
) -> Vec<LayoutBox> {
    // Handle empty text.
    if text.trim().is_empty() {
        return vec![];
    }

    // Get font properties from styles.
    let font_size = styles.font_size_px().unwrap_or(16.0);
    let white_space = styles.white_space();

    // Handle white-space: nowrap (no line breaking).
    if white_space == WhiteSpace::NoWrap {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return vec![];
        }
        let width = estimate_text_width(&text, font_size);
        return vec![LayoutBox {
            node,
            kind: LayoutKind::Text { text },
            rect: Rect {
                x,
                y,
                width,
                height: font_size * 1.2,
            },
            styles: styles.clone(),
            margin: Margin::zero(),
            border: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            children: Vec::new(),
        }];
    }

    // Normal text wrapping with line breaking.
    // For now, use a simple word-wrap algorithm.
    // TODO: Replace with full Parley line-breaking once validated.
    layout_text_with_word_wrap(
        node,
        text,
        styles,
        x,
        y,
        available_width,
        font_size,
    )
}

/// Simple word-wrap implementation (fallback until full Parley integration).
/// Breaks text into lines based on available width.
fn layout_text_with_word_wrap(
    node: Option<crate::dom::NodePtr>,
    text: &str,
    styles: &StyleMap,
    x: f32,
    y: f32,
    available_width: f32,
    font_size: f32,
) -> Vec<LayoutBox> {
    let mut boxes = Vec::new();
    let line_height = font_size * 1.2;
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return boxes;
    }

    let mut current_line = String::new();
    let mut line_y = y;

    for (i, word) in words.iter().enumerate() {
        let candidate = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current_line, word)
        };

        let candidate_width = estimate_text_width(&candidate, font_size);

        // If line would exceed available width and we have content, wrap.
        if !current_line.is_empty() && candidate_width > available_width {
            // Commit the current line.
            let line_width = estimate_text_width(&current_line, font_size);
            boxes.push(LayoutBox {
                node: node.clone(),
                kind: LayoutKind::Text {
                    text: current_line.clone(),
                },
                rect: Rect {
                    x,
                    y: line_y,
                    width: line_width.min(available_width),
                    height: line_height,
                },
                styles: styles.clone(),
                margin: Margin::zero(),
                border: EdgeSizes::zero(),
                padding: EdgeSizes::zero(),
                children: Vec::new(),
            });

            current_line = word.to_string();
            line_y += line_height;
        } else {
            current_line = candidate;
        }

        // On the last word, commit what we have.
        if i == words.len() - 1 && !current_line.is_empty() {
            let line_width = estimate_text_width(&current_line, font_size);
            boxes.push(LayoutBox {
                node: node.clone(),
                kind: LayoutKind::Text {
                    text: current_line.clone(),
                },
                rect: Rect {
                    x,
                    y: line_y,
                    width: line_width.min(available_width),
                    height: line_height,
                },
                styles: styles.clone(),
                margin: Margin::zero(),
                border: EdgeSizes::zero(),
                padding: EdgeSizes::zero(),
                children: Vec::new(),
            });
        }
    }

    boxes
}

/// Estimate text width using a simple heuristic.
/// TODO: Replace with Parley's actual glyph measurement once integrated.
fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    // Rough: average character width is ~0.5 * font_size for monospace-ish fonts.
    // For more accuracy, integrate Parley's glyph measurement.
    (text.len() as f32) * (font_size * 0.5)
}

// TODO: Full Parley integration for Phase 4:
//
// 1. **FontContext lifecycle**: Move from per-call to per-document (Phase 5)
// 2. **Parley Layout API**: Replace word-wrap with actual line-breaking via Parley
//    - Use `parley::layout::Layout::builder()`
//    - Set font properties: family, size, weight, style
//    - Set line width constraint
//    - Call `layout.layout()` to get positioned runs
// 3. **Glyph measurement**: Use Parley's glyph measurement instead of `estimate_text_width`
// 4. **Text shaping**: Parley uses rustybuzz under the hood (already integrated)
// 5. **Multi-script support**: Parley handles BiDi, CJK, combining marks automatically
// 6. **Line breaking**: Support hanging punctuation, hyphenation, discretionary breaks
// 7. **Text styling**: Apply color, weight, decoration from StyleMap
// 8. **Validation**: Test with fixtures (CJK, RTL Arabic, Devanagari, etc.)
//
// Current implementation uses a simple word-wrap fallback that works but lacks:
// - Proper glyph-level metrics
// - BiDi text support
// - Hyphenation
// - OpenType features
//
// The skeleton above shows where Parley calls will go. Fill them in by:
// - Reading Parley's docs: https://github.com/linebender/parley
// - Reference: Blitz's inline.rs integration
// - Validate each text feature with a test case
