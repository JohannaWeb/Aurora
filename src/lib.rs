//! Aurora browser engine — library interface for integration tests.

pub mod render;

// Re-export modules needed by integration tests and the shared runner pipeline.
pub(crate) mod atlas;
pub(crate) mod css;
pub(crate) mod dom;
pub(crate) mod fetch;
pub(crate) mod font;
pub(crate) mod gpu_paint;
pub(crate) mod html;
pub(crate) mod identity;
pub(crate) mod js_boa;
pub(crate) mod layout;
pub(crate) mod runner;
pub(crate) mod style;
pub(crate) mod window;

pub(crate) use runner::{ImageCache, SvgCache, load_images, load_svgs};
