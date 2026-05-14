//! Aurora render abstraction — backend-agnostic drawing commands.
//!
//! The trait RenderBackend decouples the paint layer from vello::Scene.
//! Two backends ship:
//!   - VelloBackend  — GPU path, wraps vello::Scene (used in the live window)
//!   - ImageBackend  — software path, draws to an image::RgbaImage (headless/CI)

mod commands;
pub mod headless;
mod image_backend;
mod vello_backend;

pub use commands::{BorderEdge, Bounds, DrawCommand, RenderBackend, Rgba};
pub use image_backend::ImageBackend;
pub use vello_backend::VelloBackend;
