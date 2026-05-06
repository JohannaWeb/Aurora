use rustybuzz::{shape, UnicodeBuffer};

use super::glyph::{PositionedGlyph, TextRun};
use super::resources::get_font_face;

pub fn layout_text_run(text: &str, font_size: f32) -> TextRun {
    let face = get_font_face();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);

    let glyph_buffer = shape(face, &[], buffer);
    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();
    let mut glyphs = Vec::new();
    let mut cursor_x = 0.0;
    let scale = font_size / face.units_per_em() as f32;
    let text_chars: Vec<char> = text.chars().collect();

    for (i, (_info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
        let ch = text_chars.get(i).copied().unwrap_or(' ');
        glyphs.push(PositionedGlyph {
            ch,
            x: cursor_x + (pos.x_offset as f32 * scale),
            y_offset: pos.y_offset as f32 * scale,
        });
        cursor_x += pos.x_advance as f32 * scale;
    }

    TextRun {
        glyphs,
        width: cursor_x,
    }
}
