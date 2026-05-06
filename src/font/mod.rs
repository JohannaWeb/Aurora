//! True font support with pre-baked glyph atlas and shaping.

mod atlas_builder;
mod glyph;
mod metrics;
mod raster;
mod resources;
mod shape;

#[allow(unused_imports)]
pub use glyph::{PositionedGlyph, RasterGlyph, TextRun};
#[allow(unused_imports)]
pub use metrics::{get_atlas_texture, get_glyph, get_glyph_metrics, measure_text};
pub use raster::rasterize_glyph;
pub use shape::layout_text_run;
