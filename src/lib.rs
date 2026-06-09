//! Aurora browser engine — library interface for integration tests.

pub(crate) mod blitz_document;
pub mod render;

// Re-export modules needed by integration tests and the shared runner pipeline.
pub(crate) mod atlas;
pub(crate) mod css;
pub(crate) mod dom;
pub(crate) mod fetch;
pub(crate) mod font;
pub(crate) mod html;
pub(crate) mod identity;
#[cfg(feature = "engine-boa")]
pub(crate) mod js_boa;
pub(crate) mod js_engine;
pub(crate) mod js_sm;
pub(crate) mod layout;
pub(crate) mod media;
pub(crate) mod runner;
pub(crate) mod style;
pub(crate) mod window;

pub(crate) use media::MediaCache;
pub(crate) use runner::{ImageCache, SvgCache, load_missing_images, load_missing_svgs};
