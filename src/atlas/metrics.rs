/// Glyph metrics within the atlas texture.
///
/// RUST FUNDAMENTAL: `Clone` and `Copy` allow easy duplication of this small
/// stack-allocated type. `Debug` enables `{:?}` formatting.
#[derive(Clone, Copy, Debug)]
pub struct GlyphMetrics {
    /// X coordinate of glyph in atlas texture (pixels from left).
    pub x: u32,
    /// Y coordinate of glyph in atlas texture (pixels from top).
    pub y: u32,
    /// Glyph bitmap width in pixels.
    pub width: u32,
    /// Glyph bitmap height in pixels.
    pub height: u32,
    /// Horizontal offset when rendering, for example for accents.
    pub x_offset: i32,
    /// Vertical offset when rendering, for example for subscripts.
    pub y_offset: i32,
    /// How far to advance cursor for next glyph.
    pub advance_width: f32,
    /// UV coordinates in normalized [0, 1] space for texture mapping.
    pub uv_min: (f32, f32),
    /// UV coordinates for opposite corner.
    pub uv_max: (f32, f32),
}
