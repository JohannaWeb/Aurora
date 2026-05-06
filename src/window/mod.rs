//! Window and screenshot rendering.

mod app;
mod app_handler;
mod chrome;
mod input;
mod open;
mod scene_helpers;
mod screenshot;
mod scroll_metrics;

pub use input::WindowInput;
pub use open::open;

pub(crate) const BROWSER_CHROME_HEIGHT: f32 = 174.0;
