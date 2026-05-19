use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::js_boa::BoaRuntime;
use crate::layout::document::LayoutDocument;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
use crate::ImageCache;
use opus::domain::Identity;
use std::cell::RefCell;
use std::rc::Rc;

pub struct WindowInput {
    pub dom: NodePtr,
    pub stylesheet: Rc<RefCell<Stylesheet>>,
    pub base_url: Option<String>,
    pub identity: Identity,
    pub viewport: Rc<RefCell<ViewportSize>>,
    pub layout: Rc<RefCell<LayoutTree>>,
    pub images: ImageCache,
    pub svgs: crate::SvgCache,
    pub runtime: Option<BoaRuntime>,
    /// Shared incremental layout document — also wired into the JS registry
    /// so DOM mutations can mark nodes dirty directly.
    pub layout_doc: Rc<RefCell<LayoutDocument>>,
}

impl WindowInput {
    pub(crate) fn reflow(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let content_viewport = ViewportSize {
            width: width as f32,
            height: ((height as f32) - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
        };

        *self.viewport.borrow_mut() = ViewportSize {
            width: width as f32,
            height: height as f32,
        };

        // Update viewport so Taffy recomputes at the new size.
        self.layout_doc.borrow_mut().set_viewport(content_viewport);

        let style_tree = StyleTree::from_dom(&self.dom, &self.stylesheet.borrow());

        // Incremental compute — Taffy skips clean subtrees.
        let root = self.layout_doc.borrow_mut().compute(&style_tree);
        *self.layout.borrow_mut() = LayoutTree::from_root(root);

        // Only reload images when layout changes (not on every style-only reflow).
        let layout_borrow = self.layout.borrow();
        self.images = crate::load_images(layout_borrow.root(), self.base_url.as_deref(), &self.identity);
        self.svgs = crate::load_svgs(layout_borrow.root(), self.base_url.as_deref(), &self.identity);
    }
}
