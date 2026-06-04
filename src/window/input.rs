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
    // By design, Aurora keeps this legacy layout path alive for tests,
    // screenshots, JS layout accessors, and current hit testing.
    pub layout: Rc<RefCell<LayoutTree>>,
    pub images: ImageCache,
    pub svgs: crate::SvgCache,
    pub media: MediaCache,
    pub runtime: Option<BoaRuntime>,
    // The live window renderer uses Blitz DOM + Blitz Paint.
    pub blitz_doc: Option<BlitzDocument>,
    pub(crate) needs_reflow: bool,
}

impl WindowInput {
    pub(crate) fn reflow(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let viewport = ViewportSize {
            width: width as f32,
            height: height as f32,
        };
        let content_viewport = ViewportSize {
            width: viewport.width,
            height: (viewport.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
        };

        *self.viewport.borrow_mut() = viewport;

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
            // Keep the current renderer path in sync with the same content viewport.
            let content_w = content_viewport.width as u32;
            let content_h = content_viewport.height as u32;
            
            // Re-serialize the mutated legacy DOM to HTML, then reload it into blitz_doc.
            // This ensures JS mutations are rendered in the blitz-dom / blitz-paint path.
            let html = crate::js_boa::serialize_outer_html(&self.dom);
            *blitz_doc = BlitzDocument::from_html(
                &html,
                self.base_url.as_deref(),
                &self.identity,
                content_w,
                content_h,
            );
        }
        self.needs_reflow = false;
    }

    /// Marks the document as needing a reflow. This should be called by the JS bridge
    /// after any DOM or style mutation that affects the visual state.
    pub fn mark_dirty(&mut self) {
        self.needs_reflow = true;
    }

    /// Navigates to a new URL by fetching HTML, parsing the DOM, extracting styles,
    /// executing script tags, resetting caches, and performing a full viewport reflow.
    pub fn navigate_to(&mut self, url: &str) {
        println!("Navigating to URL: {}", url);

        // 1. Fetch new HTML
        let html = match crate::fetch::fetch_html(url, &self.identity) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to navigate to {}: {}", url, e);
                return;
            }
        };

        // 2. Parse DOM
        let new_dom = crate::html::Parser::new(&html).parse_document();

        // 3. Extract and compile Stylesheet
        let mut new_stylesheet = Stylesheet::from_dom(&new_dom, Some(url), &self.identity);
        new_stylesheet.merge(Stylesheet::user_agent_stylesheet());

        // 4. Initialize scripts/runtime — fetch externals in parallel, skip oversized ones.
        let scripts = crate::runner::scripts::extract_scripts(&new_dom);
        let new_runtime = if !scripts.is_empty() {
            println!("Boa: Processing {} scripts...", scripts.len());
            let fetched: Vec<Option<String>> = {
                let handles: Vec<_> = scripts
                    .into_iter()
                    .map(|(source, is_url)| {
                        let url_str = url.to_string();
                        let identity = self.identity.clone();
                        std::thread::spawn(move || {
                            crate::runner::pipeline::fetch_script(source, is_url, Some(&url_str), &identity)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
            };
            let mut rt = crate::js_boa::BoaRuntime::new(Rc::clone(&new_dom));
            for content in fetched.into_iter().flatten() {
                if let Err(e) = rt.execute(&content) {
                    eprintln!("JS Error: {}", e);
                }
            }
            Some(rt)
        } else {
            None
        };

        // 5. Update self fields
        self.dom = new_dom;
        self.base_url = Some(url.to_string());
        *self.stylesheet.borrow_mut() = new_stylesheet;
        self.runtime = new_runtime;

        // Clear caches
        self.images.clear();
        self.svgs.clear();
        self.media = crate::media::MediaCache::default();

        // 6. Reset viewport & trigger full reflow
        let viewport = *self.viewport.borrow();

        // Re-bind runtime shared state if runtime exists
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_shared_state(
                self.layout.clone(),
                self.stylesheet.clone(),
                self.viewport.clone(),
            );
            runtime.clear_dirty_bits();
        }

        self.reflow(viewport.width as u32, viewport.height as u32);
        self.needs_reflow = true;
    }
}
