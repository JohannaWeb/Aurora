use crate::ImageCache;
use crate::blitz_document::BlitzDocument;
use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::identity::Identity;
use crate::js_boa::BoaRuntime;
use crate::layout::{LayoutTree, ViewportSize};
use crate::media::MediaCache;
use crate::style::StyleTree;
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
    pub media: MediaCache,
    pub runtime: Option<BoaRuntime>,
    pub blitz_doc: Option<BlitzDocument>,
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

        let style_tree = StyleTree::from_dom(&self.dom, &self.stylesheet.borrow());
        *self.layout.borrow_mut() =
            LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);

        let layout_borrow = self.layout.borrow();
        crate::load_missing_images(
            layout_borrow.root(),
            self.base_url.as_deref(),
            &self.identity,
            &mut self.images,
        );
        crate::load_missing_svgs(
            layout_borrow.root(),
            self.base_url.as_deref(),
            &self.identity,
            &mut self.svgs,
        );
        self.media.load_missing(
            layout_borrow.root(),
            self.base_url.as_deref(),
            &self.identity,
        );

        if let Some(blitz_doc) = &mut self.blitz_doc {
            let content_w = width;
            let content_h = ((height as f32) - crate::window::BROWSER_CHROME_HEIGHT).max(1.0) as u32;
            blitz_doc.resolve(content_w, content_h);
        }
    }
}
