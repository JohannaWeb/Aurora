use parley::style::{FontStack, StyleProperty};
use parley::{FontContext, LayoutContext};
use std::cell::RefCell;

use super::glyph::{PositionedGlyph, TextRun};

thread_local! {
    static FONT_CTX: RefCell<FontContext> = RefCell::new(FontContext::default());
}

/// Shape a text string using Parley (which uses HarfBuzz/rustybuzz internally).
/// Returns a TextRun with per-glyph positions for the renderer.
pub fn layout_text_run(text: &str, font_size: f32) -> TextRun {
    if text.is_empty() {
        return TextRun {
            glyphs: Vec::new(),
            width: 0.0,
        };
    }

    FONT_CTX.with(|font_cx| {
        let mut font_cx = font_cx.borrow_mut();
        let mut layout_cx: LayoutContext<peniko::Color> = LayoutContext::new();
        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0);
        builder.push_default(&StyleProperty::FontSize(font_size));
        builder.push_default(&StyleProperty::FontStack(FontStack::Source("system-ui")));

        let mut layout = builder.build(text);
        layout.break_all_lines(None);

        let mut glyphs = Vec::new();
        let mut total_width = 0.0_f32;
        let text_chars: Vec<char> = text.chars().collect();

        for line in layout.lines() {
            for item in line.items() {
                use parley::layout::PositionedLayoutItem;
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let mut run_x = run.offset();
                    for glyph in run.glyphs() {
                        let ch = text_chars
                            .get(glyph.text_index as usize)
                            .copied()
                            .unwrap_or(' ');

                        glyphs.push(PositionedGlyph {
                            ch,
                            x: run_x,
                            y_offset: 0.0,
                        });

                        run_x += glyph.advance;
                        total_width = total_width.max(run_x);
                    }
                }
            }
        }

        TextRun {
            glyphs,
            width: total_width,
        }
    })
}
