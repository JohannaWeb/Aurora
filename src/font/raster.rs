use ab_glyph::{Font, PxScale};

use super::glyph::RasterGlyph;
use super::resources::get_ab_font;

pub fn rasterize_glyph(ch: char, font_size: f32) -> Option<RasterGlyph> {
    if ch == ' ' || ch == '\n' || ch == '\t' {
        return None;
    }

    let font = get_ab_font();
    let glyph_id = font.glyph_id(ch);
    let glyph = glyph_id.with_scale(PxScale::from(font_size));
    let outline = font.outline_glyph(glyph)?;
    let bounds = outline.px_bounds();
    let width = bounds.width().ceil().max(0.0) as u32;
    let height = bounds.height().ceil().max(0.0) as u32;
    if width == 0 || height == 0 {
        return None;
    }

    let mut bitmap = vec![0u8; (width * height) as usize];
    outline.draw(|x, y, coverage| {
        let idx = (y * width + x) as usize;
        if let Some(pixel) = bitmap.get_mut(idx) {
            *pixel = (coverage.clamp(0.0, 1.0) * 255.0) as u8;
        }
    });

    Some(RasterGlyph {
        width,
        height,
        x_offset: bounds.min.x.floor() as i32,
        y_offset: bounds.min.y.floor() as i32,
        bitmap,
    })
}
