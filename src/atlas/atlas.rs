use std::collections::HashMap;

use super::GlyphMetrics;

/// Pre-rasterized glyph atlas texture for GPU rendering.
///
/// RUST FUNDAMENTAL: public fields expose texture data and dimensions while
/// the glyph map stays private, so callers use methods for safe lookup.
pub struct GlyphAtlas {
    /// Atlas texture data in RGBA8 format (4 bytes per pixel).
    pub texture: Vec<u8>,
    /// Atlas texture width in pixels.
    pub width: u32,
    /// Atlas texture height in pixels.
    pub height: u32,
    glyphs: HashMap<char, GlyphMetrics>,
}

impl GlyphAtlas {
    /// Create a new empty atlas with specified dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        GlyphAtlas {
            texture: vec![0u8; (width * height * 4) as usize],
            width,
            height,
            glyphs: HashMap::new(),
        }
    }

    /// Register a pre-rasterized glyph in the atlas at a specific position.
    #[allow(clippy::too_many_arguments)]
    pub fn register_glyph(
        &mut self,
        ch: char,
        bitmap: &[u8],
        glyph_width: u32,
        glyph_height: u32,
        x_offset: i32,
        y_offset: i32,
        advance_width: f32,
        atlas_x: u32,
        atlas_y: u32,
    ) {
        self.copy_bitmap(bitmap, glyph_width, glyph_height, atlas_x, atlas_y);

        let uv_min = (
            atlas_x as f32 / self.width as f32,
            atlas_y as f32 / self.height as f32,
        );
        let uv_max = (
            (atlas_x + glyph_width) as f32 / self.width as f32,
            (atlas_y + glyph_height) as f32 / self.height as f32,
        );

        let metrics = GlyphMetrics {
            x: atlas_x,
            y: atlas_y,
            width: glyph_width,
            height: glyph_height,
            x_offset,
            y_offset,
            advance_width,
            uv_min,
            uv_max,
        };

        self.glyphs.insert(ch, metrics);
    }

    /// Get metrics for a character.
    pub fn get_glyph(&self, ch: char) -> Option<GlyphMetrics> {
        self.glyphs.get(&ch).copied()
    }

    /// Get reference to all glyphs.
    pub fn glyphs(&self) -> &HashMap<char, GlyphMetrics> {
        &self.glyphs
    }

    fn copy_bitmap(
        &mut self,
        bitmap: &[u8],
        glyph_width: u32,
        glyph_height: u32,
        atlas_x: u32,
        atlas_y: u32,
    ) {
        let bitmap_stride = glyph_width;

        for row in 0..glyph_height {
            let src_offset = (row * bitmap_stride) as usize;
            let dst_offset = ((atlas_y + row) * self.width + atlas_x) as usize * 4;

            for col in 0..glyph_width {
                let src = src_offset + col as usize;
                let dst = dst_offset + col as usize * 4;

                if src < bitmap.len() && dst + 3 < self.texture.len() {
                    let alpha = bitmap[src];
                    self.texture[dst] = 255;
                    self.texture[dst + 1] = 255;
                    self.texture[dst + 2] = 255;
                    self.texture[dst + 3] = alpha;
                }
            }
        }
    }
}
