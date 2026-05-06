//! Browser chrome rendering helpers.

mod display;
mod identity;
mod nav;
mod scene;
mod tabs;
mod text;
mod top_bar;

pub(in crate::window) use display::{chrome_display_url, truncate_chrome_text};
pub(in crate::window) use scene::paint_browser_chrome_scene;
