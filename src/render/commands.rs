//! Drawing command types and the RenderBackend trait.

/// A newtype for pixel values to prevent unit confusion and layout drift.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Px(pub f32);

impl From<f32> for Px {
    fn from(f: f32) -> Self {
        Px(f)
    }
}
impl From<Px> for f32 {
    fn from(p: Px) -> Self {
        p.0
    }
}

/// An RGBA color, each channel 0–255.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

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
    pub x: Px,
    pub y: Px,
    pub width: Px,
    pub height: Px,
}

impl Bounds {
    pub fn new(x: Px, y: Px, width: Px, height: Px) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> Px {
        Px(self.x.0 + self.width.0)
    }

    pub fn bottom(&self) -> Px {
        Px(self.y.0 + self.height.0)
    }
}

/// Border edges, one width per side (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderEdge {
    pub top: Px,
    pub right: Px,
    pub bottom: Px,
    pub left: Px,
}

impl BorderEdge {
    pub fn zero() -> Self {
        Self {
            top: Px(0.0),
            right: Px(0.0),
            bottom: Px(0.0),
            left: Px(0.0),
        }
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
    fn draw_text(&mut self, text: &str, x: Px, y: Px, font_size: Px, color: Rgba, opacity: f32);

    /// Draw a decoded image (RGBA pixels) into the given bounds.
    fn draw_image(
        &mut self,
        bounds: Bounds,
        pixels: &[u8],
        img_width: u32,
        img_height: u32,
        opacity: f32,
    );

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
        x: Px,
        y: Px,
        font_size: Px,
        color: Rgba,
        opacity: f32,
    },
    DrawImage {
        bounds: Bounds,
        opacity: f32,
    },
}

/// Represents what needs to be recomputed in the next frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvalidationLevel {
    None,
    Paint,  // Only repaint (e.g. opacity, color change)
    Layout, // Recompute positions (e.g. width, height, margin)
    Style,  // Recompute CSS cascade (e.g. class change, new styles)
}
