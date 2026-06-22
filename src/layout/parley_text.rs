use std::cell::RefCell;

use parley::{Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, StyleProperty};

use crate::css::{EdgeSizes, Margin, WhiteSpace};

use super::{LayoutBox, LayoutKind, Rect};

thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new(FontContext::new());
    static LAYOUT_CX: RefCell<LayoutContext<()>> = RefCell::new(LayoutContext::new());
}

/// Measure the intrinsic (unwrapped) width of `text` at `font_size`.
pub(super) fn measure(text: &str, font_size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    layout_lines(text, font_size, None).0
}

/// Lay out `text` with optional line-wrapping and return `(width, height)`.
/// Pass `None` for `max_width` to get intrinsic single-line dimensions.
pub(super) fn layout_lines(text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0);
    }
    FONT_CX.with(|fc| {
        LAYOUT_CX.with(|lc| {
            let mut fc = fc.borrow_mut();
            let mut lc = lc.borrow_mut();
            let mut builder = lc.ranged_builder(&mut fc, text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(font_size));
            let mut layout: Layout<()> = builder.build(text);
            layout.break_all_lines(max_width);
            layout.align(max_width, Alignment::Start, AlignmentOptions::default());
            (layout.width(), layout.height())
        })
    })
}

/// Lay out a text node using Parley, respecting white-space and available width.
/// Returns one `LayoutBox` per visual line.
#[allow(dead_code)]
pub fn layout_text_with_parley(
    node: Option<crate::dom::NodePtr>,
    text: &str,
    styles: &crate::css::StyleMap,
    x: f32,
    y: f32,
    available_width: f32,
) -> Vec<LayoutBox> {
    if text.trim().is_empty() {
        return vec![];
    }

    let font_size = styles.font_size_px().unwrap_or(16.0);
    let white_space = styles.white_space();

    let (normalized, max_width) = if white_space == WhiteSpace::NoWrap {
        (text.split_whitespace().collect::<Vec<_>>().join(" "), None)
    } else {
        (text.to_string(), Some(available_width))
    };

    if normalized.is_empty() {
        return vec![];
    }

    FONT_CX.with(|fc| {
        LAYOUT_CX.with(|lc| {
            let mut fc = fc.borrow_mut();
            let mut lc = lc.borrow_mut();
            let mut builder = lc.ranged_builder(&mut fc, &normalized, 1.0, true);
            builder.push_default(StyleProperty::FontSize(font_size));
            let mut layout: Layout<()> = builder.build(&normalized);
            layout.break_all_lines(max_width);
            layout.align(max_width, Alignment::Start, AlignmentOptions::default());

            let line_height = font_size * 1.2;
            let mut boxes = Vec::new();
            let mut line_y = y;

            for line in layout.lines() {
                let range = line.text_range();
                let line_text = normalized[range].trim_end_matches('\n').to_string();
                if line_text.trim().is_empty() {
                    line_y += line_height;
                    continue;
                }
                let line_width = line.metrics().advance;
                boxes.push(LayoutBox {
                    node: node.clone(),
                    kind: LayoutKind::Text { text: line_text },
                    rect: Rect {
                        x,
                        y: line_y,
                        width: line_width,
                        height: line_height,
                    },
                    styles: styles.clone(),
                    margin: Margin::zero(),
                    border: EdgeSizes::zero(),
                    padding: EdgeSizes::zero(),
                    children: Vec::new(),
                });
                line_y += line_height;
            }

            boxes
        })
    })
}
