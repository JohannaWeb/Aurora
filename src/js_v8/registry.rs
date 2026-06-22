use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use v8;

use crate::dom::NodePtr;
use crate::window::SnapshotRebuildReason;

pub(super) struct NodeRegistry {
    pub(super) nodes: RefCell<BTreeMap<u32, NodePtr>>,
    /// Reverse map: Rc pointer address → node ID.
    reverse_nodes: RefCell<BTreeMap<usize, u32>>,
    wrappers: RefCell<BTreeMap<u32, v8::Global<v8::Object>>>,
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
    pub(super) listeners:
        RefCell<BTreeMap<u32, BTreeMap<String, Vec<v8::Global<v8::Function>>>>>,
    /// `MutationObserver` instances by observer id (callback + observer object).
    pub(super) mo_observers: RefCell<BTreeMap<u32, super::mutation_observer::MoObserver>>,
    /// Active `observe()` registrations and their pending records.
    pub(super) mo_entries: RefCell<Vec<super::mutation_observer::MoEntry>>,
    /// Monotonic id source for MutationObservers (kept separate from node ids).
    mo_next: RefCell<u32>,
    snapshot_rebuild_reason: RefCell<Option<SnapshotRebuildReason>>,
    /// Native mirror of the JS `customElements` registry (Phase 1 of the native
    /// custom-element-reaction plan). Populated from `customElements.define`.
    pub(super) ce_registry: super::custom_elements::CeRegistry,
    /// When set (env `AURORA_NATIVE_CE_REACTIONS`, or via the runtime setter),
    /// the native insertion path enqueues `connectedCallback` reactions and the
    /// JS shim suppresses its own firing (Phase 2 A/B flag). Default off —
    /// YouTube's Polymer path stays on the JS shim until later phases migrate its
    /// orchestration. `Cell` so it can be toggled deterministically (tests).
    pub(super) native_ce_reactions: std::cell::Cell<bool>,
}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        Self {
            nodes: RefCell::new(BTreeMap::new()),
            reverse_nodes: RefCell::new(BTreeMap::new()),
            wrappers: RefCell::new(BTreeMap::new()),
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
            snapshot_rebuild_reason: RefCell::new(None),
            ce_registry: super::custom_elements::CeRegistry::default(),
            native_ce_reactions: std::cell::Cell::new(
                std::env::var_os("AURORA_NATIVE_CE_REACTIONS").is_some(),
            ),
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

    /// Hit-test content coordinates against the Blitz layout, returning the
    /// deepest mapped DOM node at that point. Backs `document.elementFromPoint`.
    pub(super) fn hit_test(&self, x: f32, y: f32) -> Option<NodePtr> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let hit = render_document.borrow().hit_test_dom_node(x, y);
        hit
    }

    pub(super) fn blitz_node_id(&self, node: &NodePtr) -> Option<usize> {
        self.render_document
            .borrow()
            .as_ref()
            .and_then(|doc| doc.borrow().blitz_node_id_for_dom(node))
    }

    pub(super) fn dom_node_for_blitz_id(&self, node_id: usize) -> Option<NodePtr> {
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
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_all_attributes(node);
        }
    }

    pub(super) fn sync_text_to_render_document(&self, node: &NodePtr, text: &str) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| render_document.borrow_mut().sync_text_node(node, text))
    }

    /// Blitz/Stylo border-box geometry for `node`, if a render document is
    /// attached and the node is laid out. Backs the JS layout accessors.
    pub(super) fn layout_metrics(
        &self,
        node: &NodePtr,
    ) -> Option<crate::blitz_document::LayoutMetrics> {
        let render_document = self.render_document.borrow().as_ref().cloned()?;
        let metrics = render_document.borrow().dom_node_layout_metrics(node);
        metrics
    }

    pub(super) fn sync_shadow_root_to_render_document(
        &self,
        host: &NodePtr,
        shadow_root: &NodePtr,
        mode: &str,
    ) -> bool {
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

    pub(super) fn sync_clear_children_in_render_document(&self, parent: &NodePtr) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| render_document.borrow_mut().sync_clear_children(parent))
    }

    pub(super) fn sync_children_to_render_document(&self, parent: &NodePtr) -> bool {
        self.render_document
            .borrow()
            .as_ref()
            .cloned()
            .is_none_or(|render_document| {
                render_document.borrow_mut().sync_replace_children(parent)
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
