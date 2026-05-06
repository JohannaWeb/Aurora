use vello::Scene;

use super::display::chrome_display_url;
use super::identity::paint_identity;
use super::nav::paint_nav_and_url;
use super::tabs::paint_tabs;
use super::top_bar::paint_top_bar;

pub(in crate::window) fn paint_browser_chrome_scene(scene: &mut Scene, width: u32, url: &str) {
    let display_url = chrome_display_url(url);
    paint_top_bar(scene, width);
    paint_tabs(scene, width);
    paint_nav_and_url(scene, width, &display_url);
    paint_identity(scene, width);
}
