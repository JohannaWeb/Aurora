//! Window and screenshot rendering.
#![allow(dead_code, unused_imports, unused_variables)]

mod app;
mod app_handler;
mod chrome;
mod input;
mod open;
mod screenshot;
mod scroll_metrics;

pub use input::WindowInput;
pub use open::open;

pub(crate) const BROWSER_CHROME_HEIGHT: f32 = 72.0;
