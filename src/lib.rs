//! Aurora browser engine.
//!
//! The public embedding API lives in [`api`] and is re-exported at the crate
//! root: [`Browser`], [`Page`], [`Capabilities`], [`Error`]. Everything else is
//! internal engine machinery, exposed as `pub(crate)` for the binary and tests.

pub mod api;
pub use api::{Browser, BrowserBuilder, Capabilities, Error, Page};

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
#[cfg(feature = "engine-sm")]
pub(crate) mod js_sm;
#[cfg(feature = "v8")]
pub(crate) mod js_v8;
pub(crate) mod layout;
pub(crate) mod logging;
pub(crate) mod media;
pub(crate) mod runner;
pub(crate) mod style;
pub(crate) mod window;

pub(crate) use media::MediaCache;
pub(crate) use runner::{ImageCache, SvgCache, load_missing_images, load_missing_svgs};
