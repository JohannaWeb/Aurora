use super::resources::get_glyph_atlas;

pub fn get_glyph_metrics(ch: char) -> Option<crate::atlas::GlyphMetrics> {
    get_glyph_atlas().get_glyph(ch)
}

pub fn measure_text(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size
}

pub fn get_atlas_texture() -> (&'static [u8], u32, u32) {
    let atlas = get_glyph_atlas();
    (&atlas.texture, atlas.width, atlas.height)
}

pub fn get_glyph(_ch: char) -> &'static [u8; 8] {
    &[0; 8]
}
