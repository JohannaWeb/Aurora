//! GPU glyph atlas for efficient text rendering.
//!
//! Public API: glyph metrics, atlas storage, and glyph packing.

mod atlas;
mod metrics;
mod packer;

#[cfg(test)]
mod tests;

pub use atlas::GlyphAtlas;
pub use metrics::GlyphMetrics;
pub use packer::AtlasPacker;
