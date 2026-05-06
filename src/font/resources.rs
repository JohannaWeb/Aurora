use ab_glyph::FontRef;
use rustybuzz::Face;
use std::sync::OnceLock;

use crate::atlas::GlyphAtlas;

use super::atlas_builder::AtlasBuilder;

static GLYPH_ATLAS: OnceLock<GlyphAtlas> = OnceLock::new();
static FONT_DATA: &[u8] = include_bytes!("../../fonts/default.ttf");
static FONT_FACE: OnceLock<Face<'static>> = OnceLock::new();

pub(super) const ATLAS_BASE_SIZE: f32 = 32.0;

pub(super) fn get_font_face() -> &'static Face<'static> {
    FONT_FACE
        .get_or_init(|| Face::from_slice(FONT_DATA, 0).expect("Failed to parse font for shaping"))
}

pub(super) fn get_ab_font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_DATA).expect("Failed to parse font for rasterization")
}

pub(super) fn get_glyph_atlas() -> &'static GlyphAtlas {
    GLYPH_ATLAS.get_or_init(AtlasBuilder::build)
}
