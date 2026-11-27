use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::dom::{Node, NodePtr, ShadowTreeBackend};
use crate::window::SnapshotRebuildReason;

type EventListeners = BTreeMap<u32, BTreeMap<String, Vec<v8::Global<v8::Function>>>>;
type ComputedStyleKey = (usize, Option<crate::blitz_document::ComputedPseudo>, bool);
type ComputedStyleTargets = BTreeMap<ComputedStyleKey, Box<super::style_class::computed_style::ComputedStyleData>>;

pub(super) struct NodeRegistry {
    pub(super) nodes: RefCell<BTreeMap<u32, NodePtr>>,
    /// Reverse map: Rc pointer address → node ID.
    reverse_nodes: RefCell<BTreeMap<usize, u32>>,
    wrappers: RefCell<BTreeMap<u32, v8::Global<v8::Object>>>,
    /// Shared object templates, built on first use (see `templates`).
    pub(super) templates: RefCell<BTreeMap<super::templates::TemplateKind, v8::Global<v8::ObjectTemplate>>>,
    pub(super) interface_prototypes: RefCell<BTreeMap<&'static str, v8::Global<v8::Object>>>,
    pub(super) next_id: RefCell<u32>,
    dirty: RefCell<DirtyState>,
    // Outer `RefCell` (not `Rc<RefCell>`): the whole registry is already shared
    // as `Rc<NodeRegistry>`, so a per-field `Rc` adds nothing. The *inner*
    // `Rc<RefCell<T>>` is the genuinely shared handle (set via `set_shared_state`
    // from the pipeline, which keeps its own clone).
    pub(super) layout_tree: RefCell<Option<Rc<RefCell<crate::layout::LayoutTree>>>>,
    pub(super) stylesheet: RefCell<Option<Rc<RefCell<crate::css::Stylesheet>>>>,
    pub(super) viewport: RefCell<Option<Rc<RefCell<crate::layout::ViewportSize>>>>,
    render_document: RefCell<Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>>,
    pub(super) document: RefCell<Option<NodePtr>>,
    pub(super) listeners: RefCell<EventListeners>,
    /// `MutationObserver` instances by observer id (callback + observer object).
    pub(super) mo_observers: RefCell<BTreeMap<u32, super::mutation_observer::MoObserver>>,
    /// Active `observe()` registrations and their pending records.
    pub(super) mo_entries: RefCell<Vec<super::mutation_observer::MoEntry>>,
    /// Monotonic id source for MutationObservers (kept separate from node ids).
    mo_next: RefCell<u32>,
    /// `window.performance` clock origin and User Timing entries.
    pub(super) performance: RefCell<super::performance::PerformanceState>,
    snapshot_rebuild_reason: RefCell<Option<SnapshotRebuildReason>>,
    /// Cached `getComputedStyle` targets keyed by `(node_id, pseudo)` so repeat
    /// calls do not leak per-call heap allocations.
    computed_style_targets: RefCell<ComputedStyleTargets>,
    /// Native mirror of the JS `customElements` registry (Phase 1 of the native
    /// custom-element-reaction plan). Populated from `customElements.define`.
    pub(super) ce_registry: super::custom_elements::CeRegistry,
    /// The document's focused element (`document.activeElement`), if any.
    pub(super) focused: RefCell<Option<NodePtr>>,
    /// Current values of `<input>` elements whose value was set or edited
    /// (HTML's "dirty value"), by node id; others show their `value` attribute.
    pub(super) input_values: RefCell<BTreeMap<u32, String>>,
    /// How far the viewport is scrolled, in CSS pixels.
    pub(super) scroll: std::cell::Cell<(f64, f64)>,
    /// Script scrolled the viewport and the window has not taken it yet; until
    /// it does, the window's own position does not overwrite `scroll`.
    pub(super) scroll_requested: std::cell::Cell<bool>,
    /// The viewport scrolled since `scroll` was last fired.
    pub(super) scroll_event_pending: std::cell::Cell<bool>,
    /// The viewport size last seen by `tick`, to detect resizes.
    pub(super) last_viewport: std::cell::Cell<Option<(f32, f32)>>,
    /// The layout and scroll position intersections were last checked at;
    /// `None` after an `observe()`.
    pub(super) intersections_checked: std::cell::Cell<Option<(u64, (f64, f64))>>,
    /// Whether script has ever observed an intersection.
    pub(super) observes_intersections: std::cell::Cell<bool>,
    /// Layout generation the ResizeObserver observations were last checked
    /// against; `None` forces a check.
    pub(super) resizes_checked: std::cell::Cell<Option<u64>>,
    pub(super) observes_resizes: std::cell::Cell<bool>,
    /// Images script set `src` on or asked about, by node id.
    pub(super) images: RefCell<BTreeMap<u32, super::image_load::ImageState>>,
    /// Scripts page script created, until they ran.
    pub(super) scripts: RefCell<super::script_load::Scripts>,
    /// A navigation requested by script (location, form submission), for the
    /// window to perform.
    pub(super) pending_navigation: RefCell<Option<String>>,
    /// When set, the native insertion path enqueues custom-element lifecycle
    /// reactions (connected/disconnected/attributeChanged) and the JS shim's
    /// upgrade path defers connectedCallback to the native trampoline. Default
    /// ON since the Polymer orchestration migrated into the trampoline
    /// (validated at parity on real YouTube); set env
    /// `AURORA_NATIVE_CE_REACTIONS=0` to fall back to the JS-shim-driven path.
    /// `Cell` so it can be toggled deterministically (tests).
    pub(super) native_ce_reactions: std::cell::Cell<bool>,
}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        Self {
            nodes: RefCell::new(BTreeMap::new()),
            reverse_nodes: RefCell::new(BTreeMap::new()),
            wrappers: RefCell::new(BTreeMap::new()),
            templates: RefCell::new(BTreeMap::new()),
            interface_prototypes: RefCell::new(BTreeMap::new()),
            next_id: RefCell::new(1),
            dirty: RefCell::new(DirtyState::default()),
            layout_tree: RefCell::new(None),
            stylesheet: RefCell::new(None),
            viewport: RefCell::new(None),
            render_document: RefCell::new(None),
            document: RefCell::new(None),
            listeners: RefCell::new(BTreeMap::new()),
            mo_observers: RefCell::new(BTreeMap::new()),
            mo_entries: RefCell::new(Vec::new()),
            mo_next: RefCell::new(1),
            performance: RefCell::new(super::performance::PerformanceState::new()),
            snapshot_rebuild_reason: RefCell::new(None),
            computed_style_targets: RefCell::new(BTreeMap::new()),
            ce_registry: super::custom_elements::CeRegistry::default(),
            focused: RefCell::new(None),
            input_values: RefCell::new(BTreeMap::new()),
            scroll: std::cell::Cell::new((0.0, 0.0)),
            scroll_requested: std::cell::Cell::new(false),
            scroll_event_pending: std::cell::Cell::new(false),
            last_viewport: std::cell::Cell::new(None),
            intersections_checked: std::cell::Cell::new(None),
            observes_intersections: std::cell::Cell::new(false),
            resizes_checked: std::cell::Cell::new(None),
            observes_resizes: std::cell::Cell::new(false),
            images: RefCell::new(BTreeMap::new()),
            scripts: RefCell::new(super::script_load::Scripts::default()),
            pending_navigation: RefCell::new(None),
            native_ce_reactions: std::cell::Cell::new(native_ce_reactions_default(
                std::env::var("AURORA_NATIVE_CE_REACTIONS").ok().as_deref(),
            )),
        }
    }

    /// Allocate a fresh MutationObserver id.
    pub(super) fn alloc_observer_id(&self) -> u32 {
        let mut n = self.mo_next.borrow_mut();
        let id = *n;
        *n += 1;
        id
    }

    pub(super) fn add_event_listener(
        &self,
        node_id: u32,
        event_type: String,
        callback: v8::Global<v8::Function>,
    ) {
        let mut listeners = self.listeners.borrow_mut();
        listeners
            .entry(node_id)
            .or_default()
            .entry(event_type)
            .or_default()
            .push(callback);
    }

    pub(super) fn remove_event_listener(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        node_id: u32,
        event_type: &str,
        callback: v8::Local<v8::Function>,
    ) {
        let mut listeners = self.listeners.borrow_mut();
        if let Some(by_type) = listeners.get_mut(&node_id) {
            if let Some(list) = by_type.get_mut(event_type) {
                list.retain(|stored| {
                    let stored_local = v8::Local::new(scope, stored);
                    !stored_local.strict_equals(callback.into())
                });
            }
        }
    }

    pub(super) fn get_listeners(
        &self,
        node_id: u32,
        event_type: &str,
    ) -> Vec<v8::Global<v8::Function>> {
        let listeners = self.listeners.borrow();
        listeners
            .get(&node_id)
            .and_then(|l| l.get(event_type))
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn set_shared_state(
        &self,
        layout_tree: Rc<RefCell<crate::layout::LayoutTree>>,
        stylesheet: Rc<RefCell<crate::css::Stylesheet>>,
        viewport: Rc<RefCell<crate::layout::ViewportSize>>,
        document: NodePtr,
    ) {
        *self.layout_tree.borrow_mut() = Some(layout_tree);
        *self.stylesheet.borrow_mut() = Some(stylesheet);
        *self.viewport.borrow_mut() = Some(viewport);
        *self.document.borrow_mut() = Some(document);
    }

    pub(super) fn set_render_document(
        &self,
        render_document: Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>,
    ) {
        let viewport = render_document
            .as_ref()
            .map(|doc| doc.borrow().viewport_css_size());
        self.last_viewport.set(viewport);
        if let Some(doc) = render_document.as_ref() {
            let scroll_y = self.scroll.get().1;
            if scroll_y != 0.0 {
                doc.borrow_mut().set_scroll_y(scroll_y);
            }
        }
        *self.render_document.borrow_mut() = render_document;
    }

    pub(super) fn schedule_snapshot_rebuild_reason(&self, reason: SnapshotRebuildReason) {
        *self.snapshot_rebuild_reason.borrow_mut() = Some(reason);
    }

    pub(super) fn take_snapshot_rebuild_reason(&self) -> Option<SnapshotRebuildReason> {
        self.snapshot_rebuild_reason.borrow_mut().take()
    }

    pub(super) fn has_render_document(&self) -> bool {
        self.render_document.borrow().is_some()
    }

    pub(super) fn has_render_mapping(&self, node: &NodePtr) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_some_and(|render_document| {
                render_document
                    .borrow()
                    .blitz_node_id_for_dom(node)
                    .is_some()
            })
    }

    pub(super) fn render_document(
        &self,
    ) -> Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>> {
        self.render_document.borrow().clone()
    }

    /// Hit-test a viewport point against the Blitz layout, returning the
    /// deepest mapped DOM node at that point. Backs `document.elementFromPoint`.
    pub(super) fn hit_test(&self, x: f32, y: f32) -> Option<NodePtr> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        render_document.borrow().hit_test_viewport_point(x, y)
    }

    pub(super) fn blitz_node_id(&self, node: &NodePtr) -> Option<blitz_dom::NodeId> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().blitz_node_id_for_dom(node))
    }

    pub(super) fn dom_node_for_blitz_id(&self, node_id: blitz_dom::NodeId) -> Option<NodePtr> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().dom_node_for_blitz_id(node_id))
    }

    #[allow(dead_code)]
    pub(super) fn query_selector_dom(&self, selector: &str, start: &NodePtr) -> Option<NodePtr> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().query_selector_dom(selector, start))
    }

    #[allow(dead_code)]
    pub(super) fn query_selector_all_dom(
        &self,
        selector: &str,
        start: &NodePtr,
    ) -> Option<Vec<NodePtr>> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().query_selector_all_dom(selector, start))
    }

    #[allow(dead_code)]
    pub(super) fn get_element_by_id_dom(&self, id: &str) -> Option<NodePtr> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().get_element_by_id_dom(id))
    }

    pub(super) fn collect_by_tag_dom(&self, tag: &str, start: &NodePtr) -> Option<Vec<NodePtr>> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().collect_by_tag_dom(tag, start))
    }

    #[allow(dead_code)]
    pub(super) fn selector_matches_dom(&self, node: &NodePtr, selector: &str) -> Option<bool> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().selector_matches_dom(node, selector))
    }

    #[allow(dead_code)]
    pub(super) fn closest_dom(&self, node: &NodePtr, selector: &str) -> Option<Option<NodePtr>> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().closest_dom(node, selector))
    }

    #[allow(dead_code)]
    pub(super) fn sync_append_child_to_render_document(
        &self,
        parent: &NodePtr,
        child: &NodePtr,
    ) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_append_child(parent, child)
            })
    }

    pub(super) fn sync_insert_before_to_render_document(
        &self,
        parent: &NodePtr,
        new_child: &NodePtr,
        ref_child: Option<&NodePtr>,
    ) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_insert_before(parent, new_child, ref_child)
            })
    }

    pub(super) fn sync_remove_child_from_render_document(&self, child: &NodePtr) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| render_document.borrow_mut().sync_remove_child(child))
    }

    #[allow(dead_code)]
    pub(super) fn sync_replace_child_in_render_document(
        &self,
        parent: &NodePtr,
        new_child: &NodePtr,
        old_child: &NodePtr,
    ) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_replace_child(parent, new_child, old_child)
            })
    }

    pub(super) fn sync_attribute_to_render_document(
        &self,
        node: &NodePtr,
        name: &str,
        value: &str,
    ) -> bool {
        if self.is_unmirrored_detached_node(node) {
            return true;
        }
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_set_attribute(node, name, value)
            })
    }

    pub(super) fn sync_remove_attribute_from_render_document(
        &self,
        node: &NodePtr,
        name: &str,
    ) -> bool {
        if self.is_unmirrored_detached_node(node) {
            return true;
        }
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_remove_attribute(node, name)
            })
    }

    pub(super) fn sync_all_attributes_to_render_document(&self, node: &NodePtr) {
        if self.is_unmirrored_detached_node(node) {
            return;
        }
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_all_attributes(node);
        }
    }

    pub(super) fn sync_text_to_render_document(&self, node: &NodePtr, text: &str) -> bool {
        if self.is_unmirrored_detached_node(node) {
            return true;
        }
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| render_document.borrow_mut().sync_text_node(node, text))
    }

    /// `node`'s border box relative to the viewport, for
    /// `getBoundingClientRect`; `None` when it is not laid out.
    pub(super) fn client_rect(&self, node: &NodePtr) -> Option<crate::blitz_document::LayoutMetrics> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        render_document.borrow().dom_node_client_rect(node)
    }

    /// `node`'s boxes for ResizeObserver; `None` when it is not laid out.
    pub(super) fn box_sizes(&self, node: &NodePtr) -> Option<crate::blitz_document::BoxSizes> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        render_document.borrow().dom_node_box_sizes(node)
    }

    /// The document's scrollable size in CSS pixels: at least the viewport.
    pub(super) fn scrolling_area(&self) -> Option<(f32, f32)> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let doc = render_document.borrow();
        let (width, height) = doc.viewport_css_size();
        Some((width, doc.document_height().max(height)))
    }

    /// Blitz/Stylo border-box geometry for `node`, if a render document is
    /// attached and the node is laid out. Backs the JS layout accessors.
    pub(super) fn layout_metrics(
        &self,
        node: &NodePtr,
    ) -> Option<crate::blitz_document::LayoutMetrics> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        render_document.borrow().dom_node_layout_metrics(node)
    }

    /// Whether stylesheets still hold back rendering.
    pub(super) fn is_render_blocked(&self) -> bool {
        let render_document = self.render_document.borrow().as_ref().cloned();
        render_document.is_some_and(|document| document.borrow().has_pending_critical_resources())
    }

    pub(super) fn layout_generation(&self) -> Option<u64> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let generation = render_document.borrow().layout_generation();
        Some(generation)
    }

    pub(super) fn is_rendered(&self, node: &NodePtr) -> bool {
        let render_document = self.render_document.borrow().as_ref().cloned();
        render_document.is_some_and(|document| document.borrow().is_rendered(node))
    }

    /// Starts loading an image for script to observe; `None` without a render
    /// document.
    pub(super) fn load_image(&self, src: &str) -> Option<crate::blitz_document::ImageLoad> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let load = render_document.borrow().load_image(src);
        Some(load)
    }

    /// Starts fetching a script; `None` without a render document.
    pub(super) fn load_script(&self, src: &str) -> Option<crate::blitz_document::TextLoad> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let load = render_document.borrow().load_script(src);
        Some(load)
    }

    /// Scrolls the viewport for page script, clamped to the document, and asks
    /// the window to follow. Returns whether the position changed. The window
    /// only scrolls vertically, so `x` stays 0.
    pub(super) fn scroll_viewport_to(&self, _x: f64, y: f64) -> bool {
        let max_y = self
            .render_document
            .borrow()
            .as_ref()
            .map(|doc| {
                let doc = doc.borrow();
                f64::from(doc.document_height() - doc.viewport_css_size().1).max(0.0)
            })
            .unwrap_or(0.0);
        let y = if y.is_finite() { y.clamp(0.0, max_y) } else { 0.0 };
        let position = (0.0, y);
        if position == self.scroll.get() {
            return false;
        }
        self.set_scroll(position);
        self.scroll_requested.set(true);
        self.scroll_event_pending.set(true);
        true
    }

    /// Moves the viewport, in the render document too, so layout reads
    /// (`getBoundingClientRect`, `elementFromPoint`) see it before the next
    /// paint does.
    pub(super) fn set_scroll(&self, position: (f64, f64)) {
        self.scroll.set(position);
        if let Some(doc) = self.render_document.borrow().as_ref() {
            doc.borrow_mut().set_scroll_y(position.1);
        }
    }

    /// The viewport size in CSS pixels, without a render document `None`.
    pub(super) fn viewport_size(&self) -> Option<(f32, f32)> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let size = render_document.borrow().viewport_css_size();
        Some(size)
    }

    /// Evaluates a CSS media query against the render document's Stylo device.
    pub(super) fn match_media(&self, query: &str) -> (bool, String) {
        let Some(render_document) = self.render_document.borrow().as_ref().cloned() else {
            return (false, query.to_string());
        };
        let mut doc = render_document.borrow_mut();
        doc.matches_media_query(query)
    }

    pub(super) fn computed_style_target(
        registry: &Rc<NodeRegistry>,
        node: &NodePtr,
        pseudo: Option<crate::blitz_document::ComputedPseudo>,
        valid_pseudo: bool,
    ) -> *const super::style_class::computed_style::ComputedStyleData {
        let mut targets = registry.computed_style_targets.borrow_mut();
        let key = (Rc::as_ptr(node) as usize, pseudo, valid_pseudo);
        let entry = targets.entry(key).or_insert_with(|| {
            Box::new(super::style_class::computed_style::ComputedStyleData {
                node: node.clone(),
                registry: registry.clone(),
                pseudo,
                valid_pseudo,
            })
        });
        entry.as_ref() as *const _
    }

    pub(super) fn computed_style_property(
        &self,
        node: &NodePtr,
        pseudo: Option<crate::blitz_document::ComputedPseudo>,
        property: &str,
    ) -> Option<String> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let mut doc = render_document.borrow_mut();
        doc.computed_style_property(node, pseudo, property)
    }

    pub(super) fn computed_style_property_names(
        &self,
        node: &NodePtr,
        pseudo: Option<crate::blitz_document::ComputedPseudo>,
    ) -> Vec<String> {
        let Some(render_document) = self.render_document.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let mut doc = render_document.borrow_mut();
        doc.computed_style_property_names(node, pseudo)
    }

    pub(super) fn sync_shadow_root_to_render_document(
        &self,
        host: &NodePtr,
        shadow_root: &NodePtr,
        mode: &str,
    ) -> bool {
        if self.is_unmirrored_detached_node(host) {
            return true;
        }
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document
                    .borrow_mut()
                    .sync_attach_shadow_root(host, shadow_root, mode)
            })
    }

    fn is_unmirrored_detached_node(&self, node: &NodePtr) -> bool {
        // Detached templates acquire a render mapping when inserted. Their
        // local mutations must still succeed and notify MutationObservers.
        !self.has_render_mapping(node)
            && self.document.borrow().as_ref().is_some_and(|document| {
                !super::tree::mutation::is_connected_to(document, node)
            })
    }

    pub(super) fn sync_children_to_render_document(&self, parent: &NodePtr) -> bool {
        let backend = crate::dom::SyntheticShadowTreeBackend;
        if let Some(root) = backend.nearest_shadow_root(parent)
            && let Some(host) = backend.host_for_shadow_root(&root)
        {
            backend.distribute_slots(&host);
        }
        // Slots are flattened by the renderer, so rebuild their containing tree.
        let mut target = parent.clone();
        while matches!(&*target.borrow(), Node::Element(el) if el.tag_name == "slot") {
            let Some(ancestor) = crate::dom::parent_ptr(&target) else { return true };
            target = ancestor;
        }
        if !self.has_render_mapping(&target) {
            return true;
        }
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document.borrow_mut().sync_replace_children(&target)
            })
    }

    pub(super) fn register(&self, node: NodePtr) -> u32 {
        let key = Rc::as_ptr(&node) as usize;
        if let Some(&existing) = self.reverse_nodes.borrow().get(&key) {
            return existing;
        }
        let mut next_id = self.next_id.borrow_mut();
        let id = *next_id;
        *next_id += 1;
        drop(next_id);
        self.nodes.borrow_mut().insert(id, node);
        self.reverse_nodes.borrow_mut().insert(key, id);
        id
    }

    pub(super) fn lookup(&self, id: u32) -> Option<NodePtr> {
        self.nodes.borrow().get(&id).cloned()
    }

    pub(super) fn registered_nodes(&self) -> Vec<NodePtr> {
        self.nodes.borrow().values().cloned().collect()
    }

    pub(super) fn lookup_js_wrapper<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        id: u32,
    ) -> Option<v8::Local<'s, v8::Object>> {
        self.wrappers
            .borrow()
            .get(&id)
            .map(|wrapper| v8::Local::new(scope, wrapper))
    }

    pub(super) fn store_js_wrapper(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        id: u32,
        object: v8::Local<v8::Object>,
    ) {
        self.wrappers
            .borrow_mut()
            .insert(id, v8::Global::new(scope, object));
    }

    #[allow(dead_code)]
    pub(super) fn node_id(&self, node: &NodePtr) -> Option<u32> {
        let key = Rc::as_ptr(node) as usize;
        self.reverse_nodes.borrow().get(&key).copied()
    }

    pub(super) fn take_needs_reflow(&self) -> bool {
        let mut dirty = self.dirty.borrow_mut();
        let needs_reflow = dirty.style || dirty.layout;
        dirty.style = false;
        dirty.layout = false;
        needs_reflow
    }

    pub(super) fn clear_dirty_bits(&self) {
        let mut dirty = self.dirty.borrow_mut();
        dirty.style = false;
        dirty.layout = false;
    }

    pub(super) fn has_dirty_bits(&self) -> bool {
        let dirty = self.dirty.borrow();
        dirty.style || dirty.layout
    }

    pub(super) fn mark_style_dirty(&self, node: &NodePtr) {
        self.sync_all_attributes_to_render_document(node);
        self.dirty.borrow_mut().style = true;
    }

    pub(super) fn mark_layout_dirty(&self, _node: &NodePtr) {
        self.dirty.borrow_mut().layout = true;
    }
}

#[derive(Default)]
struct DirtyState {
    style: bool,
    layout: bool,
}

/// Resolve the `AURORA_NATIVE_CE_REACTIONS` env var to the flag's initial
/// value: on unless explicitly opted out with `0`/`false`/`off`.
fn native_ce_reactions_default(var: Option<&str>) -> bool {
    !matches!(var, Some("0") | Some("false") | Some("off"))
}

#[cfg(test)]
mod tests {
    use super::native_ce_reactions_default;

    #[test]
    fn native_ce_reactions_default_on_with_explicit_opt_out() {
        assert!(native_ce_reactions_default(None));
        assert!(native_ce_reactions_default(Some("1")));
        assert!(native_ce_reactions_default(Some("")));
        assert!(!native_ce_reactions_default(Some("0")));
        assert!(!native_ce_reactions_default(Some("false")));
        assert!(!native_ce_reactions_default(Some("off")));
    }
}
