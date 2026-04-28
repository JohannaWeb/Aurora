//! GPU glyph atlas for efficient text rendering
//!
//! Single packed texture containing all glyphs for a font.
//! Glyphs are pre-rasterized and packed using a simple guillotine algorithm.
//!
//! RUST FUNDAMENTAL: This module uses HashMap for O(1) glyph lookups by character.
//! The texture is stored as a flat Vec<u8> with RGBA8 format for GPU transfer.

// Import HashMap for character -> metrics mapping
// RUST FUNDAMENTAL: `HashMap` is the standard hash-table collection in Rust's standard library.
use std::collections::HashMap;

// Metrics describing where a glyph lives in the atlas and how to render it
/// Glyph metrics within the atlas texture
/// RUST FUNDAMENTAL: #[derive(Clone, Copy, Debug)] - Clone and Copy allow easy duplication
/// (small stack-allocated types); Debug enables {:?} formatting
#[derive(Clone, Copy, Debug)]
pub struct GlyphMetrics {
    /// X coordinate of glyph in atlas texture (pixels from left)
    pub x: u32,
    /// Y coordinate of glyph in atlas texture (pixels from top)
    pub y: u32,
    /// Glyph bitmap width in pixels
    pub width: u32,
    /// Glyph bitmap height in pixels
    pub height: u32,
    /// Horizontal offset when rendering (e.g., for accents)
    pub x_offset: i32,
    /// Vertical offset when rendering (e.g., for subscripts)
    pub y_offset: i32,
    /// How far to advance cursor for next glyph
    pub advance_width: f32,
    /// UV coordinates in normalized [0, 1] space for texture mapping
    pub uv_min: (f32, f32),
    /// UV coordinates for opposite corner
    pub uv_max: (f32, f32),
}

/// Pre-rasterized glyph atlas texture for GPU rendering
/// RUST FUNDAMENTAL: pub struct with private fields (glyphs) demonstrates encapsulation
pub struct GlyphAtlas {
    /// Atlas texture data in RGBA8 format (4 bytes per pixel)
    pub texture: Vec<u8>,
    /// Atlas texture width in pixels
    pub width: u32,
    /// Atlas texture height in pixels
    pub height: u32,
    /// HashMap mapping characters to their metrics (private for safety)
    /// RUST FUNDAMENTAL: HashMap provides O(1) lookup; private field forces use of methods
    glyphs: HashMap<char, GlyphMetrics>,
}

// Implementation of GlyphAtlas methods
impl GlyphAtlas {
    /// Create a new empty atlas with specified dimensions
    pub fn new(width: u32, height: u32) -> Self {
        // Initialize RGBA8 texture: 4 bytes per pixel, all zeros (transparent black)
        GlyphAtlas {
            // Vec preallocates all memory at once: width * height * 4 bytes
            // RUST FUNDAMENTAL: Vec<u8> is heap-allocated, owned by this struct
            texture: vec![0u8; (width * height * 4) as usize],
            // Store width and height for indexing
            width,
            height,
            // Empty map to be populated later
            glyphs: HashMap::new(),
        }
    }

    /// Register a pre-rasterized glyph in the atlas at a specific position
    /// RUST FUNDAMENTAL: &mut self takes mutable borrow - allows modification of texture/glyphs
    pub fn register_glyph(
        // Mutable reference to self (allows modification)
        &mut self,
        // Character being registered
        ch: char,
        // Bitmap data (grayscale alpha values, 0-255)
        bitmap: &[u8],
        // Bitmap width in pixels
        glyph_width: u32,
        // Bitmap height in pixels
        glyph_height: u32,
        // Baseline offset (left)
        x_offset: i32,
        // Baseline offset (up)
        y_offset: i32,
        // Advance to next glyph
        advance_width: f32,
        // X position in atlas
        atlas_x: u32,
        // Y position in atlas
        atlas_y: u32,
    ) {
        // Calculate stride (bytes per row) in atlas texture
        // RUST FUNDAMENTAL: "stride" means how many storage units you skip to move down one row in a linearized 2D buffer.
        let atlas_stride = self.width * 4;
        // Stride in bitmap (not multiplied by 4 - bitmap is grayscale)
        let bitmap_stride = glyph_width;

        // Copy bitmap rows into atlas texture
        for row in 0..glyph_height {
            // Source offset in bitmap (grayscale, 1 byte per pixel)
            let src_offset = (row * bitmap_stride) as usize;
            // Destination offset in RGBA texture (4 bytes per pixel)
            let dst_offset = ((atlas_y + row) * self.width + atlas_x) as usize * 4;

            // Copy each pixel in the row
            for col in 0..glyph_width {
                // Index into source bitmap
                let src = src_offset + col as usize;
                // Index into RGBA texture
                let dst = dst_offset + col as usize * 4;

                // Bounds check both buffers before write
                if src < bitmap.len() && dst + 3 < self.texture.len() {
                    // Read alpha value from bitmap
                    let alpha = bitmap[src];
                    // Write white text with varying alpha (standard glyph rendering)
                    self.texture[dst] = 255; // Red channel
                    self.texture[dst + 1] = 255; // Green channel
                    self.texture[dst + 2] = 255; // Blue channel
                    self.texture[dst + 3] = alpha; // Alpha channel (from bitmap)
                }
            }
        }

        // Calculate normalized UV coordinates for texture sampling [0, 1]
        let uv_min = (
            // Normalize atlas X position
            // RUST FUNDAMENTAL: Converting integer pixel coordinates to normalized UV coordinates
            // is common when data will be sampled by GPU texture hardware.
            atlas_x as f32 / self.width as f32,
            // Normalize atlas Y position
            atlas_y as f32 / self.height as f32,
        );
        // Calculate opposite corner UV coordinates
        let uv_max = (
            // Normalize right edge
            (atlas_x + glyph_width) as f32 / self.width as f32,
            // Normalize bottom edge
            (atlas_y + glyph_height) as f32 / self.height as f32,
        );

        // Create metrics struct with all information for rendering this glyph
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

        // Insert metrics into map with character as key
        // RUST FUNDAMENTAL: HashMap.insert() takes ownership of both key and value
        self.glyphs.insert(ch, metrics);
    }

    /// Get metrics for a character, returning Option
    /// RUST FUNDAMENTAL: &self is immutable borrow - doesn't modify atlas
    /// Option<T> is enum: Some(T) or None - safe null handling without null pointers
    pub fn get_glyph(&self, ch: char) -> Option<GlyphMetrics> {
        // Look up character in map, then copy the metrics (they're Copy type)
        self.glyphs.get(&ch).copied()
    }

    /// Get reference to all glyphs (private field exposed as read-only)
    pub fn glyphs(&self) -> &HashMap<char, GlyphMetrics> {
        // Return reference to glyphs map (doesn't give ownership, just borrow)
        &self.glyphs
    }
}

/// Simple guillotine packing algorithm for placing glyphs in atlas texture
/// RUST FUNDAMENTAL: This uses a Vec of rows - dynamic growth with amortized O(1) push
pub struct AtlasPacker {
    // Maximum width of atlas
    width: u32,
    // Maximum height of atlas
    height: u32,
    // Rows of glyphs (each row has fixed height, variable width usage)
    rows: Vec<PackRow>,
}

/// Single row in guillotine packing layout
struct PackRow {
    // Y position of this row
    y: u32,
    // Height of this row (all glyphs in row use this height)
    height: u32,
    // How far across the row we've used
    x_cursor: u32,
}

// AtlasPacker implementation
impl AtlasPacker {
    /// Create a new packer for an atlas of given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        AtlasPacker {
            width,
            height,
            // Start with no rows - create them as needed
            rows: vec![],
        }
    }

    /// Pack a glyph of given size into atlas, returning position or None if full
    /// RUST FUNDAMENTAL: &mut self takes mutable borrow - allows modifying rows and cursors
    pub fn pack(&mut self, glyph_width: u32, glyph_height: u32) -> Option<(u32, u32)> {
        // Try to fit glyph in an existing row
        // RUST FUNDAMENTAL: &mut self.rows gives mutable iterator
        for row in &mut self.rows {
            // Check if glyph fits: enough width left and height matches
            // RUST FUNDAMENTAL: This is a greedy packing heuristic, not an optimal search.
            // It takes the first row that fits rather than trying every possible arrangement.
            if row.x_cursor + glyph_width <= self.width && glyph_height <= row.height {
                // Yes, it fits - record position
                let x = row.x_cursor;
                let y = row.y;
                // Advance cursor for next glyph in this row
                row.x_cursor += glyph_width;
                // Return position
                return Some((x, y));
            }
        }

        // Glyph didn't fit in existing rows, try to create new row
        // Calculate where next row would start (below all existing rows)
        let next_y = self.rows.iter().map(|r| r.y + r.height).max().unwrap_or(0);
        // RUST FUNDAMENTAL: `.max().unwrap_or(0)` is a common pattern when an iterator may be empty
        // and you want a neutral fallback value.
        // Check if new row would fit vertically
        if next_y + glyph_height <= self.height {
            // Yes, create new row
            let x = 0;
            let y = next_y;
            // RUST FUNDAMENTAL: Vec.push() - amortized O(1) growth
            self.rows.push(PackRow {
                y,
                height: glyph_height,
                x_cursor: glyph_width,
            });
            // Return position
            return Some((x, y));
        }

        // RUST FUNDAMENTAL: Returning `None` signals that packing failed without needing exceptions or sentinel coordinates.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packer() {
        let mut packer = AtlasPacker::new(256, 256);

        // Pack several glyphs
        let pos1 = packer.pack(16, 16);
        assert!(pos1.is_some());

        let pos2 = packer.pack(16, 16);
        assert!(pos2.is_some());

        // They should be adjacent in the same row
        assert_eq!(pos1.unwrap().1, pos2.unwrap().1);
    }

    #[test]
    fn test_atlas_registration() {
        let mut atlas = GlyphAtlas::new(512, 512);

        // Create a simple glyph bitmap
        let bitmap = vec![255; 16 * 16];

        atlas.register_glyph('A', &bitmap, 16, 16, 0, 0, 10.0, 0, 0);

        let metrics = atlas.get_glyph('A');
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m.width, 16);
        assert_eq!(m.height, 16);
        assert_eq!(m.advance_width, 10.0);
    }
}
