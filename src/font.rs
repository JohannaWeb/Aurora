//! True font support with pre-baked glyph atlas and shaping
//!
//! This module provides TrueType font loading, shaping via rustybuzz,
//! and a unified glyph atlas for efficient GPU rendering.

// Import OnceLock for lazy static initialization
// RUST FUNDAMENTAL: `OnceLock<T>` is a standard-library primitive for one-time initialization.
// The first caller computes the value, and every later caller gets a shared reference to the same cached result.
// It is a good fit for expensive resources such as parsed fonts or prebuilt atlases.
use std::sync::OnceLock;

// Import glyph atlas and packer
// RUST FUNDAMENTAL: `crate::...` starts from this crate's root module.
// That makes the import path explicit and stable even if the current file's local module nesting changes.
use crate::atlas::{AtlasPacker, GlyphAtlas};

// Import font and scaling types from ab_glyph
// RUST FUNDAMENTAL: Traits let Rust code work with shared behavior across different concrete types.
// Here the `Font` trait gives a common interface for font operations, while `FontRef` is one concrete implementation.
use ab_glyph::{Font, FontRef, PxScale};

// Import font shaping from rustybuzz
// RUST FUNDAMENTAL: Text shaping is the step between Unicode characters and actual rendered glyph placement.
// A shaped result may not be one glyph per character, because ligatures, combining marks, and script-specific rules can change the mapping.
use rustybuzz::{shape, Face, UnicodeBuffer};

// Global glyph atlas (lazily initialized on first access)
// RUST FUNDAMENTAL: `static` items live for the entire duration of the program.
static GLYPH_ATLAS: OnceLock<GlyphAtlas> = OnceLock::new();
// Embedded TTF font file (binary data)
// RUST FUNDAMENTAL: `include_bytes!(...)` embeds a file's raw bytes into the binary at compile time as a `&'static [u8; N]`.
static FONT_DATA: &[u8] = include_bytes!("../fonts/default.ttf");
// Cached font face for shaping (rustybuzz)
static FONT_FACE: OnceLock<Face<'static>> = OnceLock::new();

// Font size in pixels for pre-baking glyph atlas
const ATLAS_BASE_SIZE: f32 = 32.0;

// Get or initialize font face for text shaping (uses rustybuzz)
fn get_font_face() -> &'static Face<'static> {
    // Initialize on first call, reuse thereafter
    // RUST FUNDAMENTAL: Returning `&'static T` here is valid because the value is stored in a `static` cache.
    FONT_FACE.get_or_init(|| {
        // Parse font data as TrueType font
        Face::from_slice(FONT_DATA, 0).expect("Failed to parse font for shaping")
    })
}

// Get font reference for glyph rendering (uses ab_glyph)
fn get_ab_font() -> FontRef<'static> {
    // Parse embedded font data for rendering
    // RUST FUNDAMENTAL: This function returns an owned lightweight font handle rather than a borrowed temporary,
    // so callers can use it freely within the current scope.
    FontRef::try_from_slice(FONT_DATA).expect("Failed to parse font for rasterization")
}

// Builder for creating pre-baked glyph atlas
pub struct AtlasBuilder;

// Implementation of atlas builder
impl AtlasBuilder {
    // Build a pre-rasterized glyph atlas for common characters (0-255)
    pub fn build() -> GlyphAtlas {
        // Use base size for atlas glyphs
        let base_size = ATLAS_BASE_SIZE;
        // Load font for rasterization
        let font = get_ab_font();
        // Create scale for font rendering
        let scale = PxScale::from(base_size);

        // Atlas dimensions (1024x1024)
        // RUST FUNDAMENTAL: Unsuffixed integer literals are inferred from usage, here as `u32` because the constructor expects `u32`.
        let atlas_width = 1024;
        let atlas_height = 1024;
        // Create new empty atlas
        let mut atlas = GlyphAtlas::new(atlas_width, atlas_height);
        // Create packer for placing glyphs in atlas
        let mut packer = AtlasPacker::new(atlas_width, atlas_height);

        // Pre-rasterize all ASCII and extended ASCII characters
        for code in 0u32..256 {
            // Convert code point to character
            // RUST FUNDAMENTAL: `char::from_u32(...)` returns `Option<char>` because not every integer is a valid Unicode scalar value.
            if let Some(ch) = char::from_u32(code) {
                // Get glyph ID for this character
                let glyph_id = font.glyph_id(ch);
                // Get scaled glyph
                let glyph = glyph_id.with_scale(scale);

                // Try to get glyph outline
                if let Some(outline) = font.outline_glyph(glyph) {
                    // Get glyph bounding box
                    let bounds = outline.px_bounds();
                    // Calculate bitmap width
                    let width = bounds.width() as u32;
                    // Calculate bitmap height
                    let height = bounds.height() as u32;

                    // Only process glyphs with non-zero dimensions
                    if width > 0 && height > 0 {
                        // Create bitmap buffer for glyph
                        // RUST FUNDAMENTAL: `vec![value; len]` allocates and fills a vector with repeated copies of the initial value.
                        let mut bitmap = vec![0u8; (width * height) as usize];
                        // Rasterize glyph outline to bitmap
                        // RUST FUNDAMENTAL: The closure passed to `draw(...)` captures `bitmap` mutably from the surrounding scope.
                        outline.draw(|x, y, v| {
                            // Calculate pixel index
                            let idx = (y * width + x) as usize;
                            // Set alpha value (0-255)
                            if idx < bitmap.len() {
                                // RUST FUNDAMENTAL: Numeric casts with `as` are explicit in Rust even for float-to-int conversions.
                                bitmap[idx] = (v * 255.0) as u8;
                            }
                        });

                        // Try to pack glyph in atlas
                        if let Some((atlas_x, atlas_y)) = packer.pack(width, height) {
                            // Get font's units per em for scaling
                            // RUST FUNDAMENTAL: `unwrap_or(...)` is often used when a library API exposes an optional metric
                            // but the caller wants a reasonable default fallback.
                            let upem = font.units_per_em().unwrap_or(1000.0);
                            // Calculate character advance width
                            let advance = font.h_advance_unscaled(glyph_id) * (base_size / upem);

                            // Register glyph in atlas with its metrics
                            atlas.register_glyph(
                                ch,
                                &bitmap,
                                width,
                                height,
                                bounds.min.x as i32,
                                bounds.min.y as i32,
                                advance,
                                atlas_x,
                                atlas_y,
                            );
                        }
                    }
                } else if ch == ' ' {
                    // Space character has no outline but has advance width
                    // Get font's units per em
                    let upem = font.units_per_em().unwrap_or(1000.0);
                    // Calculate space advance
                    let advance = font.h_advance_unscaled(glyph_id) * (base_size / upem);
                    // Register space with zero bitmap
                    // RUST FUNDAMENTAL: A glyph can still participate in layout even if it has no pixels to draw,
                    // because advance width matters for text spacing.
                    atlas.register_glyph(ch, &[], 0, 0, 0, 0, advance, 0, 0);
                }
            }
        }

        // Return fully populated atlas
        atlas
    }
}

// Get or initialize global glyph atlas
fn get_glyph_atlas() -> &'static GlyphAtlas {
    // Initialize atlas on first call, reuse thereafter
    // RUST FUNDAMENTAL: Passing a function item like `AtlasBuilder::build` uses it as a zero-argument callback.
    GLYPH_ATLAS.get_or_init(AtlasBuilder::build)
}

// Get glyph metrics from atlas for a character
pub fn get_glyph_metrics(ch: char) -> Option<crate::atlas::GlyphMetrics> {
    // Get the global atlas
    let atlas = get_glyph_atlas();
    // Look up metrics for character
    atlas.get_glyph(ch)
}

// Measure text width at given font size
pub fn measure_text(text: &str, font_size: f32) -> f32 {
    // Calculate width as character count times font size
    // Note: uses simple character count, not shaped glyph advances
    // RUST FUNDAMENTAL: This is a deliberately approximate helper; not every function in a codebase needs to model full correctness if its purpose is heuristic sizing.
    text.chars().count() as f32 * font_size
}

// Rasterized glyph data
#[derive(Debug, Clone)]
pub struct RasterGlyph {
    // Glyph bitmap width
    pub width: u32,
    // Glyph bitmap height
    pub height: u32,
    // Horizontal offset from baseline
    pub x_offset: i32,
    // Vertical offset from baseline
    pub y_offset: i32,
    // RGBA8 bitmap data
    pub bitmap: Vec<u8>,
}

// Single glyph positioned in a text run
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    // Character being rendered
    pub ch: char,
    // X position in text run
    pub x: f32,
    // Y offset from baseline
    pub y_offset: f32,
}

// Laid-out sequence of glyphs
#[derive(Debug, Clone)]
pub struct TextRun {
    // Vector of positioned glyphs
    pub glyphs: Vec<PositionedGlyph>,
    // Total width of text run
    pub width: f32,
}

// Layout text using font shaping with proper glyph positioning
pub fn layout_text_run(text: &str, font_size: f32) -> TextRun {
    // Get font face for shaping
    let face = get_font_face();
    // Create Unicode buffer for shaping input
    // RUST FUNDAMENTAL: Builder-style buffer APIs are common when constructing structured input for another library.
    let mut buffer = UnicodeBuffer::new();
    // Add text to buffer
    buffer.push_str(text);

    // Perform text shaping (converts characters to glyphs with positioning)
    let glyph_buffer = shape(face, &[], buffer);
    // Get shaped glyph info
    let infos = glyph_buffer.glyph_infos();
    // Get glyph positions (x_advance, x_offset, y_offset)
    let positions = glyph_buffer.glyph_positions();

    // Initialize result glyph vector
    let mut glyphs = Vec::new();
    // Track cursor position as we layout glyphs
    let mut cursor_x = 0.0;

    // Calculate scale factor from font units to pixels
    let upem = face.units_per_em() as f32;
    let scale = font_size / upem;

    // Collect characters for lookup
    // RUST FUNDAMENTAL: Collecting the text into `Vec<char>` gives indexed access later in the shaping loop.
    let text_chars: Vec<char> = text.chars().collect();

    // Process each shaped glyph
    // RUST FUNDAMENTAL: Zipping two iterators together is a convenient way to walk paired data in lockstep.
    for (i, (_info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
        // Get character for this glyph
        // RUST FUNDAMENTAL: `.copied()` converts `Option<&char>` into `Option<char>` because `char` implements `Copy`.
        let ch = text_chars.get(i).copied().unwrap_or(' ');

        // Create positioned glyph with offsets
        glyphs.push(PositionedGlyph {
            ch,
            // X position is cursor plus x offset
            x: cursor_x + (pos.x_offset as f32 * scale),
            // Y offset for positioning (subscripts, etc.)
            y_offset: pos.y_offset as f32 * scale,
        });

        // Advance cursor by glyph advance
        cursor_x += pos.x_advance as f32 * scale;
    }

    // Return text run with glyphs and total width
    TextRun {
        glyphs,
        width: cursor_x,
    }
}

pub fn get_atlas_texture() -> (&'static [u8], u32, u32) {
    // RUST FUNDAMENTAL: Returning a tuple is a lightweight way to bundle a few related values without creating a dedicated struct.
    let atlas = get_glyph_atlas();
    (&atlas.texture, atlas.width, atlas.height)
}

pub fn rasterize_glyph(ch: char, font_size: f32) -> Option<RasterGlyph> {
    // RUST FUNDAMENTAL: Guard clauses at the top of a function are a common Rust style for filtering out simple cases early.
    if ch == ' ' || ch == '\n' || ch == '\t' {
        return None;
    }

    let font = get_ab_font();
    let glyph_id = font.glyph_id(ch);
    let glyph = glyph_id.with_scale(PxScale::from(font_size));
    let outline = font.outline_glyph(glyph)?;
    // RUST FUNDAMENTAL: The `?` operator here propagates `None` if the glyph has no outline, because this function returns `Option<_>`.
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
            // RUST FUNDAMENTAL: `.get_mut(idx)` returns `Option<&mut T>`, giving a bounds-checked mutable reference instead of panicking on bad indexes.
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

// For compatibility with old system if needed
pub fn get_glyph(_ch: char) -> &'static [u8; 8] {
    // RUST FUNDAMENTAL: The leading underscore in `_ch` tells Rust that this parameter is intentionally unused.
    &[0; 8]
}
