//! True font support with pre-baked glyph atlas and shaping.

mod atlas_builder;
mod glyph;
mod metrics;
mod raster;
mod resources;
mod shape;

pub use metrics::{get_atlas_texture, get_glyph_metrics};
#[cfg(feature = "taffy-document")]
pub use metrics::measure_text;
pub use raster::rasterize_glyph;
pub use shape::layout_text_run;
