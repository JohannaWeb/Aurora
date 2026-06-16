//! Screenshot chrome rendering.
//!
//! The chrome is authored once as a Dioxus component (see
//! `window::chrome::dioxus_chrome`). For screenshots we render that same HTML
//! through Aurora's own style + layout engine and rasterize it into the top
//! strip of the image — so the screenshot and the live window share a single
//! source of truth rather than two hand-kept-in-sync painters.

use super::ScreenshotImage;
use super::layout::render_layout_with_text;
use crate::identity::Identity;
use crate::window::chrome::{CHROME_HEIGHT, ChromeProps, chrome_html};

pub(super) fn render_browser_chrome(
    img: &mut ScreenshotImage,
    width: u32,
    url: &str,
    dom: &crate::dom::NodePtr,
    identity: &Identity,
) {
    let html = chrome_html(ChromeProps::from_render_state(url, dom, identity));

    let chrome_dom = crate::html::Parser::new(&html).parse_document();
    crate::dom::reparent_subtree(&chrome_dom);

    let mut stylesheet = crate::css::Stylesheet::from_dom(&chrome_dom, None, identity);
    stylesheet.merge(crate::css::Stylesheet::user_agent_stylesheet());

    let style_tree = crate::style::StyleTree::from_dom(&chrome_dom, &stylesheet);
    let viewport = crate::layout::ViewportSize {
        width: width as f32,
        height: CHROME_HEIGHT as f32,
    };
    let layout = crate::layout::LayoutTree::from_style_tree_with_viewport(&style_tree, viewport);

    render_layout_with_text(&layout, img, 0, 0);
}
