use ab_glyph::{Font, PxScale};

use crate::atlas::{AtlasPacker, GlyphAtlas};

use super::resources::{get_ab_font, ATLAS_BASE_SIZE};

pub(super) struct AtlasBuilder;

impl AtlasBuilder {
    pub(super) fn build() -> GlyphAtlas {
        let font = get_ab_font();
        let scale = PxScale::from(ATLAS_BASE_SIZE);
        let atlas_width = 1024;
        let atlas_height = 1024;
        let mut atlas = GlyphAtlas::new(atlas_width, atlas_height);
        let mut packer = AtlasPacker::new(atlas_width, atlas_height);

        for code in 0u32..256 {
            if let Some(ch) = char::from_u32(code) {
                register_character(ch, &font, scale, &mut atlas, &mut packer);
            }
        }

        atlas
    }
}

fn register_character(
    ch: char,
    font: &ab_glyph::FontRef<'static>,
    scale: PxScale,
    atlas: &mut GlyphAtlas,
    packer: &mut AtlasPacker,
) {
    let glyph_id = font.glyph_id(ch);
    let glyph = glyph_id.with_scale(scale);

    if let Some(outline) = font.outline_glyph(glyph) {
        let bounds = outline.px_bounds();
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        if width == 0 || height == 0 {
            return;
        }

        let mut bitmap = vec![0u8; (width * height) as usize];
        outline.draw(|x, y, v| {
            let idx = (y * width + x) as usize;
            if idx < bitmap.len() {
                bitmap[idx] = (v * 255.0) as u8;
            }
        });

        if let Some((atlas_x, atlas_y)) = packer.pack(width, height) {
            atlas.register_glyph(
                ch,
                &bitmap,
                width,
                height,
                bounds.min.x as i32,
                bounds.min.y as i32,
                atlas_x,
                atlas_y,
            );
        }
    } else if ch == ' ' {
        atlas.register_glyph(ch, &[], 0, 0, 0, 0, 0, 0);
    }
}
