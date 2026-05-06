#[derive(Debug, Clone)]
pub struct RasterGlyph {
    pub width: u32,
    pub height: u32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub bitmap: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    pub ch: char,
    pub x: f32,
    pub y_offset: f32,
}

#[derive(Debug, Clone)]
pub struct TextRun {
    pub glyphs: Vec<PositionedGlyph>,
    pub width: f32,
}
