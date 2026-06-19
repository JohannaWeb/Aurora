//! Browser chrome: authored as a Dioxus component, rendered through Aurora's
//! own engines (Blitz/Vello for the live window, Taffy/CPU for screenshots).

mod dioxus_chrome;
mod display;

pub(in crate::window) use dioxus_chrome::{
    CHROME_HEIGHT, ChromeProps, ChromeRenderer, chrome_html,
};
pub(in crate::window) use display::chrome_display_url;
