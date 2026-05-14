//! Drawing command types and the RenderBackend trait.

/// An RGBA color, each channel 0–255.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

/// Axis-aligned rectangle in screen space (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Border edges, one width per side (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderEdge {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl BorderEdge {
    pub fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }
}

/// The backend-agnostic drawing trait.
/// Both VelloBackend and ImageBackend implement this.
pub trait RenderBackend {
    /// Fill a rectangle with a solid color at the given opacity.
    fn fill_rect(&mut self, bounds: Bounds, color: Rgba, opacity: f32);

    /// Stroke the edges of a rectangle.
    fn stroke_rect(&mut self, bounds: Bounds, border: BorderEdge, color: Rgba, opacity: f32);

    /// Draw a text string.
    fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Rgba,
        opacity: f32,
    );

    /// Draw a decoded image (RGBA pixels) into the given bounds.
    fn draw_image(&mut self, bounds: Bounds, pixels: &[u8], img_width: u32, img_height: u32, opacity: f32);

    /// Draw a placeholder (for images that haven't loaded).
    fn draw_image_placeholder(&mut self, bounds: Bounds, opacity: f32);

    /// Push a scroll clip so content outside bounds is not visible.
    fn push_clip(&mut self, bounds: Bounds);

    /// Pop the most recent clip.
    fn pop_clip(&mut self);
}

/// Convenience enum for when you need to store draw commands before executing them.
/// Not required for the live render path but useful for testing and debugging.
#[allow(dead_code)]
pub enum DrawCommand {
    FillRect {
        bounds: Bounds,
        color: Rgba,
        opacity: f32,
    },
    StrokeRect {
        bounds: Bounds,
        border: BorderEdge,
        color: Rgba,
        opacity: f32,
    },
    DrawText {
        text: String,
        x: f32,
        y: f32,
        font_size: f32,
        color: Rgba,
        opacity: f32,
    },
    DrawImage {
        bounds: Bounds,
        opacity: f32,
    },
}
