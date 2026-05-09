use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::js_boa::BoaRuntime;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
use crate::ImageCache;
use opus::domain::Identity;

pub struct WindowInput {
    pub dom: NodePtr,
    pub stylesheet: Stylesheet,
    pub base_url: Option<String>,
    pub identity: Identity,
    pub viewport: ViewportSize,
    pub layout: LayoutTree,
    pub images: ImageCache,
    pub runtime: Option<BoaRuntime>,
}

impl WindowInput {
    pub(crate) fn reflow(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.viewport = ViewportSize {
            width: width as f32,
            height: height as f32,
        };
        let content_viewport = ViewportSize {
            width: width as f32,
            height: ((height as f32) - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
        };
        let style_tree = StyleTree::from_dom(&self.dom, &self.stylesheet);
        self.layout = LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);
        self.images =
            crate::load_images(self.layout.root(), self.base_url.as_deref(), &self.identity);
    }
}
