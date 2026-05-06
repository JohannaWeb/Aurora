/// Simple guillotine packing algorithm for placing glyphs in atlas texture.
///
/// RUST FUNDAMENTAL: this uses a `Vec` of rows for dynamic growth with
/// amortized O(1) push.
pub struct AtlasPacker {
    width: u32,
    height: u32,
    rows: Vec<PackRow>,
}

struct PackRow {
    y: u32,
    height: u32,
    x_cursor: u32,
}

impl AtlasPacker {
    /// Create a new packer for an atlas of given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        AtlasPacker {
            width,
            height,
            rows: vec![],
        }
    }

    /// Pack a glyph of given size into atlas, returning position if it fits.
    pub fn pack(&mut self, glyph_width: u32, glyph_height: u32) -> Option<(u32, u32)> {
        for row in &mut self.rows {
            if row.x_cursor + glyph_width <= self.width && glyph_height <= row.height {
                let x = row.x_cursor;
                let y = row.y;
                row.x_cursor += glyph_width;
                return Some((x, y));
            }
        }

        let next_y = self.rows.iter().map(|r| r.y + r.height).max().unwrap_or(0);
        if next_y + glyph_height <= self.height {
            let x = 0;
            let y = next_y;
            self.rows.push(PackRow {
                y,
                height: glyph_height,
                x_cursor: glyph_width,
            });
            return Some((x, y));
        }

        None
    }
}
