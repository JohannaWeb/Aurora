use crate::ImageCache;
use crate::blitz_document::BlitzDocument;
use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::identity::Identity;
use crate::js_engine::JsRuntime;
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
    pub runtime: Option<Box<dyn JsRuntime>>,
    // The live window renderer uses Blitz DOM + Blitz Paint.
    pub blitz_doc: Option<Rc<RefCell<BlitzDocument>>>,
    pub(crate) needs_reflow: bool,
    pub(crate) blitz_snapshot_dirty: bool,
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

        let content_w = content_viewport.width as u32;
        let content_h = content_viewport.height as u32;
        if let Some(blitz_doc) = self.blitz_doc.as_ref() {
            blitz_doc.borrow_mut().set_viewport(content_w, content_h);
        }
        self.sync_blitz_snapshot(content_w, content_h);

        let sync_legacy_layout = self.blitz_doc.is_none()
            || matches!(
                std::env::var("AURORA_LEGACY_LAYOUT_SYNC").as_deref(),
                Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
            );
        if sync_legacy_layout {
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
        }
        self.needs_reflow = false;
    }

    pub(crate) fn mark_blitz_snapshot_dirty(&mut self) {
        self.blitz_snapshot_dirty = true;
    }

    fn sync_blitz_snapshot(&mut self, content_w: u32, content_h: u32) {
        if !self.blitz_snapshot_dirty {
            return;
        }

        match BlitzDocument::try_from_dom(
            &self.dom,
            self.base_url.as_deref(),
            &self.identity,
            content_w,
            content_h,
        ) {
            Some(next_doc) => {
                let handle = match self.blitz_doc.as_ref() {
                    Some(existing) => {
                        *existing.borrow_mut() = next_doc;
                        existing.clone()
                    }
                    None => {
                        let handle = Rc::new(RefCell::new(next_doc));
                        self.blitz_doc = Some(handle.clone());
                        handle
                    }
                };
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.set_render_document(Some(handle));
                }
                self.blitz_snapshot_dirty = false;
            }
            None => {
                log::warn!("Blitz snapshot rebuild failed; keeping previous renderer snapshot");
            }
        }
    }

    /// Marks the document as needing a reflow. This should be called by the JS bridge
    /// after any DOM or style mutation that affects the visual state.
    pub fn mark_dirty(&mut self) {
        self.needs_reflow = true;
        self.mark_blitz_snapshot_dirty();
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
        crate::dom::reparent_subtree(&new_dom);

        // 3. Extract and compile Stylesheet
        let mut new_stylesheet = Stylesheet::from_dom(&new_dom, Some(url), &self.identity);
        new_stylesheet.merge(Stylesheet::user_agent_stylesheet());
        let viewport = *self.viewport.borrow();
        let content_w = viewport.width as u32;
        let content_h = (viewport.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0) as u32;
        let new_blitz_doc = BlitzDocument::try_from_dom(
            &new_dom,
            Some(url),
            &self.identity,
            content_w,
            content_h,
        )
        .map(|doc| Rc::new(RefCell::new(doc)));

        // 4. Initialize scripts/runtime and fetch externals in parallel.
        //
        // Drop the previous runtime *before* creating the new one. A V8
        // `OwnedIsolate` is entered on creation and exited on drop, so isolates
        // must be dropped in reverse order of creation. Building the new isolate
        // while the old one is still alive and then replacing the field would
        // drop them oldest-first and panic ("must be dropped in the reverse
        // order of creation"). Tearing down the old isolate first keeps exactly
        // one live at a time.
        self.runtime = None;

        let scripts = crate::runner::scripts::extract_scripts(&new_dom);
        let new_runtime = if !scripts.is_empty() {
            println!("JS: Processing {} scripts...", scripts.len());
            let fetched: Vec<Option<String>> = {
                let handles: Vec<_> = scripts
                    .iter()
                    .map(|script| {
                        let url_str = url.to_string();
                        let identity = self.identity.clone();
                        let source = script.source.clone();
                        let is_url = script.is_url;
                        std::thread::spawn(move || {
                            crate::runner::pipeline::fetch_script(
                                source,
                                is_url,
                                Some(&url_str),
                                &identity,
                            )
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or(None))
                    .collect()
            };
            let mut rt: Box<dyn JsRuntime> = crate::js_engine::create_runtime(
                crate::js_engine::EngineKind::from_env(),
                &new_dom,
            )
            .expect("V8 backend is required for JavaScript execution");
            rt.set_render_document(new_blitz_doc.clone());
            for (script, content) in scripts.iter().zip(fetched.into_iter()) {
                let Some(content) = content else { continue };
                rt.set_current_script(Some(&script.node));
                if let Err(e) = rt.execute(&content) {
                    eprintln!("JS Error: {}", e);
                }
                rt.set_current_script(None);
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
        self.blitz_doc = new_blitz_doc;

        // Clear caches
        self.images.clear();
        self.svgs.clear();
        self.media = crate::media::MediaCache::default();

        // 6. Reset viewport & trigger full reflow
        if self.blitz_doc.is_none() {
            self.mark_blitz_snapshot_dirty();
        }

        // Re-bind runtime shared state if runtime exists
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_shared_state(
                self.layout.clone(),
                self.stylesheet.clone(),
                self.viewport.clone(),
            );
            runtime.set_render_document(self.blitz_doc.clone());
            runtime.clear_dirty_bits();
        }

        self.reflow(viewport.width as u32, viewport.height as u32);
        self.needs_reflow = true;
    }
}
