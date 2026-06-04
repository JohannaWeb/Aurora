use std::cell::RefCell;

use parley::{Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, StyleProperty};

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
