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
#[cfg(debug_assertions)]
use std::collections::VecDeque;
use std::rc::Rc;
#[cfg(debug_assertions)]
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotRebuildReason {
    ExplicitDirty,
    MissingMapping,
    PaintFailure,
    SyncOperationFailed,
    DebugValidationFailed,
    InitialLoad,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotRebuildThresholdMode {
    Warn,
    Panic,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SnapshotRebuildThreshold {
    max_per_second: usize,
    mode: SnapshotRebuildThresholdMode,
}

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
    pub(crate) pending_snapshot_rebuild_reason: Option<SnapshotRebuildReason>,
    pub(crate) pending_snapshot_rebuild_source: Option<String>,
    pub(crate) snapshot_rebuild_count: u64,
    pub(crate) consecutive_snapshot_rebuilds: u64,
    pub(crate) last_snapshot_rebuild_reason: Option<SnapshotRebuildReason>,
    pub(crate) last_snapshot_rebuild_source: Option<String>,
    pub(crate) last_snapshot_rebuild_op_id: Option<u64>,
    #[cfg(debug_assertions)]
    pub(crate) snapshot_rebuild_events: VecDeque<Instant>,
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
        if let Some(runtime) = self.runtime.as_mut()
            && let Some(reason) = runtime.take_snapshot_rebuild_reason()
        {
            self.mark_blitz_snapshot_dirty(reason);
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

    #[track_caller]
    pub(crate) fn mark_blitz_snapshot_dirty(&mut self, reason: SnapshotRebuildReason) {
        self.blitz_snapshot_dirty = true;
        self.pending_snapshot_rebuild_reason = Some(reason);
        let caller = std::panic::Location::caller();
        self.pending_snapshot_rebuild_source = Some(format!(
            "{}:{}:{}",
            caller.file(),
            caller.line(),
            caller.column()
        ));
    }

    fn sync_blitz_snapshot(&mut self, content_w: u32, content_h: u32) {
        if !self.blitz_snapshot_dirty {
            return;
        }

        let reason = self
            .pending_snapshot_rebuild_reason
            .unwrap_or(SnapshotRebuildReason::ExplicitDirty);
        let source = self.pending_snapshot_rebuild_source.clone();
        let last_op_id = self.blitz_doc.as_ref().and_then(|doc| {
            doc.borrow()
                .last_mirror_mutation_trace()
                .map(|trace| trace.op_id)
        });
        log::warn!(
            "Rebuilding Blitz snapshot from legacy DOM: reason={:?} source={:?} last_mirror_op_id={:?}",
            reason,
            source,
            last_op_id
        );
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
                self.pending_snapshot_rebuild_reason = None;
                self.pending_snapshot_rebuild_source = None;
                self.snapshot_rebuild_count += 1;
                self.consecutive_snapshot_rebuilds += 1;
                self.last_snapshot_rebuild_reason = Some(reason);
                self.last_snapshot_rebuild_source = source;
                self.last_snapshot_rebuild_op_id = last_op_id;
                self.debug_check_excessive_snapshot_rebuilds(reason, last_op_id);
            }
            None => {
                log::warn!(
                    "Blitz snapshot rebuild failed; keeping previous renderer snapshot: reason={:?} source={:?} last_mirror_op_id={:?}",
                    reason,
                    source,
                    last_op_id
                );
            }
        }
    }

    /// Marks the document as needing a reflow. This should be called by the JS bridge
    /// after any DOM or style mutation that affects the visual state.
    pub fn mark_dirty(&mut self) {
        self.needs_reflow = true;
        self.mark_blitz_snapshot_dirty(SnapshotRebuildReason::ExplicitDirty);
    }

    #[cfg(debug_assertions)]
    fn debug_check_excessive_snapshot_rebuilds(
        &mut self,
        reason: SnapshotRebuildReason,
        last_op_id: Option<u64>,
    ) {
        let Some(threshold) = snapshot_rebuild_threshold_from_env() else {
            return;
        };
        let Some(message) = self.debug_record_snapshot_rebuild_event(threshold, reason, last_op_id)
        else {
            return;
        };
        match threshold.mode {
            SnapshotRebuildThresholdMode::Warn => log::warn!("{message}"),
            SnapshotRebuildThresholdMode::Panic => panic!("{message}"),
        }
    }

    #[cfg(debug_assertions)]
    fn debug_record_snapshot_rebuild_event(
        &mut self,
        threshold: SnapshotRebuildThreshold,
        reason: SnapshotRebuildReason,
        last_op_id: Option<u64>,
    ) -> Option<String> {
        let now = Instant::now();
        self.snapshot_rebuild_events.push_back(now);
        while self
            .snapshot_rebuild_events
            .front()
            .is_some_and(|event| now.duration_since(*event) > Duration::from_secs(1))
        {
            self.snapshot_rebuild_events.pop_front();
        }
        if self.snapshot_rebuild_events.len() <= threshold.max_per_second {
            return None;
        }

        Some(format!(
            "Excessive Blitz snapshot rebuilds: {} rebuilds in the last second, threshold={}, reason={:?}, last_mirror_op_id={:?}. Incremental sync is incomplete or repeatedly failing.",
            self.snapshot_rebuild_events.len(),
            threshold.max_per_second,
            reason,
            last_op_id
        ))
    }

    #[cfg(not(debug_assertions))]
    fn debug_check_excessive_snapshot_rebuilds(
        &mut self,
        _reason: SnapshotRebuildReason,
        _last_op_id: Option<u64>,
    ) {
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
        let new_blitz_doc =
            BlitzDocument::try_from_dom(&new_dom, Some(url), &self.identity, content_w, content_h)
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
                new_blitz_doc.clone(),
            )
            .expect("V8 backend is required for JavaScript execution");
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
            self.mark_blitz_snapshot_dirty(SnapshotRebuildReason::InitialLoad);
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

#[cfg(debug_assertions)]
fn snapshot_rebuild_threshold_from_env() -> Option<SnapshotRebuildThreshold> {
    parse_snapshot_rebuild_threshold(
        std::env::var("AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND").ok(),
    )
}

#[cfg(debug_assertions)]
fn parse_snapshot_rebuild_threshold(raw: Option<String>) -> Option<SnapshotRebuildThreshold> {
    let Some(raw) = raw else {
        return Some(SnapshotRebuildThreshold {
            max_per_second: 10,
            mode: SnapshotRebuildThresholdMode::Warn,
        });
    };
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("off") || raw == "0" {
        return None;
    }
    let (mode, number) = match raw.strip_prefix("panic:") {
        Some(number) => (SnapshotRebuildThresholdMode::Panic, number),
        None => (SnapshotRebuildThresholdMode::Warn, raw),
    };
    let max_per_second = number.parse::<usize>().unwrap_or(10).max(1);
    Some(SnapshotRebuildThreshold {
        max_per_second,
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Node;
    use crate::identity::{Capability, IdentityKind};
    use std::sync::Mutex;

    static WINDOW_INPUT_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_identity() -> Identity {
        Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace, Capability::NetworkAccess],
        )
    }

    fn test_input() -> WindowInput {
        let dom = crate::html::Parser::new("<html><body><p id='item'>hello</p></body></html>")
            .parse_document();
        crate::dom::reparent_subtree(&dom);
        let identity = test_identity();
        let mut stylesheet = Stylesheet::from_dom(&dom, None, &identity);
        stylesheet.merge(Stylesheet::user_agent_stylesheet());
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let viewport = ViewportSize {
            width: 800.0,
            height: 600.0,
        };
        let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, viewport);
        let blitz_doc = BlitzDocument::try_from_dom(&dom, None, &identity, 800, 600)
            .map(|doc| Rc::new(RefCell::new(doc)));

        WindowInput {
            dom,
            stylesheet: Rc::new(RefCell::new(stylesheet)),
            base_url: None,
            identity,
            viewport: Rc::new(RefCell::new(viewport)),
            layout: Rc::new(RefCell::new(layout)),
            images: crate::ImageCache::default(),
            svgs: crate::SvgCache::default(),
            media: MediaCache::default(),
            runtime: None,
            blitz_doc,
            needs_reflow: false,
            blitz_snapshot_dirty: false,
            pending_snapshot_rebuild_reason: None,
            pending_snapshot_rebuild_source: None,
            snapshot_rebuild_count: 0,
            consecutive_snapshot_rebuilds: 0,
            last_snapshot_rebuild_reason: None,
            last_snapshot_rebuild_source: None,
            last_snapshot_rebuild_op_id: None,
            #[cfg(debug_assertions)]
            snapshot_rebuild_events: VecDeque::new(),
        }
    }

    fn find_element_by_id(node: &NodePtr, id: &str) -> Option<NodePtr> {
        match &*node.borrow() {
            Node::Element(el) if el.attributes.get("id").is_some_and(|value| value == id) => {
                Some(node.clone())
            }
            Node::Element(el) => el
                .children
                .iter()
                .find_map(|child| find_element_by_id(child, id)),
            Node::Document { children, .. } => children
                .iter()
                .find_map(|child| find_element_by_id(child, id)),
            Node::Text(_) => None,
        }
    }

    #[test]
    fn snapshot_rebuild_accounting_records_reason_and_clears_pending_reason() {
        let _guard = WINDOW_INPUT_TEST_LOCK.lock().unwrap();
        let mut input = test_input();

        input.mark_blitz_snapshot_dirty(SnapshotRebuildReason::PaintFailure);
        input.sync_blitz_snapshot(800, 600);

        assert!(!input.blitz_snapshot_dirty);
        assert_eq!(input.pending_snapshot_rebuild_reason, None);
        assert_eq!(input.pending_snapshot_rebuild_source, None);
        assert_eq!(input.snapshot_rebuild_count, 1);
        assert_eq!(input.consecutive_snapshot_rebuilds, 1);
        assert_eq!(
            input.last_snapshot_rebuild_reason,
            Some(SnapshotRebuildReason::PaintFailure)
        );
        assert!(
            input
                .last_snapshot_rebuild_source
                .as_deref()
                .is_some_and(|source| source.contains("src/window/input.rs"))
        );
        assert_eq!(input.last_snapshot_rebuild_op_id, None);
    }

    #[test]
    fn snapshot_rebuild_accounting_records_last_mirror_operation_id() {
        let _guard = WINDOW_INPUT_TEST_LOCK.lock().unwrap();
        let mut input = test_input();
        let item = find_element_by_id(&input.dom, "item").expect("fixture should have item");
        if let Node::Element(el) = &mut *item.borrow_mut() {
            el.attributes
                .insert("data-state".to_string(), "ready".to_string());
        }
        let blitz_doc = input
            .blitz_doc
            .as_ref()
            .expect("fixture should have Blitz document");
        assert!(
            blitz_doc
                .borrow_mut()
                .sync_set_attribute(&item, "data-state", "ready")
        );
        let expected_op_id = blitz_doc
            .borrow()
            .last_mirror_mutation_trace()
            .expect("sync should record mutation trace")
            .op_id;

        input.mark_blitz_snapshot_dirty(SnapshotRebuildReason::ExplicitDirty);
        input.sync_blitz_snapshot(800, 600);

        assert_eq!(input.snapshot_rebuild_count, 1);
        assert_eq!(input.last_snapshot_rebuild_op_id, Some(expected_op_id));
        assert_eq!(
            input.last_snapshot_rebuild_reason,
            Some(SnapshotRebuildReason::ExplicitDirty)
        );
    }

    #[test]
    fn mark_dirty_sets_explicit_snapshot_rebuild_reason() {
        let _guard = WINDOW_INPUT_TEST_LOCK.lock().unwrap();
        let mut input = test_input();

        input.mark_dirty();

        assert!(input.needs_reflow);
        assert!(input.blitz_snapshot_dirty);
        assert_eq!(
            input.pending_snapshot_rebuild_reason,
            Some(SnapshotRebuildReason::ExplicitDirty)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn snapshot_rebuild_threshold_parser_defaults_to_warn_at_ten() {
        assert_eq!(
            parse_snapshot_rebuild_threshold(None),
            Some(SnapshotRebuildThreshold {
                max_per_second: 10,
                mode: SnapshotRebuildThresholdMode::Warn,
            })
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn snapshot_rebuild_threshold_parser_supports_panic_mode_and_disable() {
        assert_eq!(
            parse_snapshot_rebuild_threshold(Some("panic:2".to_string())),
            Some(SnapshotRebuildThreshold {
                max_per_second: 2,
                mode: SnapshotRebuildThresholdMode::Panic,
            })
        );
        assert_eq!(
            parse_snapshot_rebuild_threshold(Some("0".to_string())),
            None
        );
        assert_eq!(
            parse_snapshot_rebuild_threshold(Some("off".to_string())),
            None
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn snapshot_rebuild_threshold_reports_when_rebuild_rate_exceeds_limit() {
        let _guard = WINDOW_INPUT_TEST_LOCK.lock().unwrap();
        let mut input = test_input();
        let threshold = SnapshotRebuildThreshold {
            max_per_second: 1,
            mode: SnapshotRebuildThresholdMode::Panic,
        };

        assert_eq!(
            input.debug_record_snapshot_rebuild_event(
                threshold,
                SnapshotRebuildReason::ExplicitDirty,
                Some(7),
            ),
            None
        );
        let message = input
            .debug_record_snapshot_rebuild_event(
                threshold,
                SnapshotRebuildReason::SyncOperationFailed,
                Some(7),
            )
            .expect("second rebuild inside one second should exceed threshold");

        assert!(message.contains("threshold=1"));
        assert!(message.contains("SyncOperationFailed"));
        assert!(message.contains("last_mirror_op_id=Some(7)"));
    }
}
