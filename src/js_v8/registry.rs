use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use v8;

use crate::dom::NodePtr;

pub(super) struct NodeRegistry {
    pub(super) nodes: Rc<RefCell<BTreeMap<u32, NodePtr>>>,
    /// Reverse map: Rc pointer address → node ID.
    reverse_nodes: Rc<RefCell<BTreeMap<usize, u32>>>,
    wrappers: Rc<RefCell<BTreeMap<u32, v8::Global<v8::Object>>>>,
    pub(super) next_id: Rc<RefCell<u32>>,
    dirty: Rc<RefCell<DirtyState>>,
    pub(super) layout_tree: Rc<RefCell<Option<Rc<RefCell<crate::layout::LayoutTree>>>>>,
    pub(super) stylesheet: Rc<RefCell<Option<Rc<RefCell<crate::css::Stylesheet>>>>>,
    pub(super) viewport: Rc<RefCell<Option<Rc<RefCell<crate::layout::ViewportSize>>>>>,
    render_document: Rc<RefCell<Option<Rc<RefCell<crate::blitz_document::BlitzDocument>>>>>,
    pub(super) document: Rc<RefCell<Option<NodePtr>>>,
    pub(super) listeners:
        Rc<RefCell<BTreeMap<u32, BTreeMap<String, Vec<v8::Global<v8::Function>>>>>>,
    /// `MutationObserver` instances by observer id (callback + observer object).
    pub(super) mo_observers: Rc<RefCell<BTreeMap<u32, super::mutation_observer::MoObserver>>>,
    /// Active `observe()` registrations and their pending records.
    pub(super) mo_entries: Rc<RefCell<Vec<super::mutation_observer::MoEntry>>>,
    /// Monotonic id source for MutationObservers (kept separate from node ids).
    mo_next: Rc<RefCell<u32>>,
}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        Self {
            nodes: Rc::new(RefCell::new(BTreeMap::new())),
            reverse_nodes: Rc::new(RefCell::new(BTreeMap::new())),
            wrappers: Rc::new(RefCell::new(BTreeMap::new())),
            next_id: Rc::new(RefCell::new(1)),
            dirty: Rc::new(RefCell::new(DirtyState::default())),
            layout_tree: Rc::new(RefCell::new(None)),
            stylesheet: Rc::new(RefCell::new(None)),
            viewport: Rc::new(RefCell::new(None)),
            render_document: Rc::new(RefCell::new(None)),
            document: Rc::new(RefCell::new(None)),
            listeners: Rc::new(RefCell::new(BTreeMap::new())),
            mo_observers: Rc::new(RefCell::new(BTreeMap::new())),
            mo_entries: Rc::new(RefCell::new(Vec::new())),
            mo_next: Rc::new(RefCell::new(1)),
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

    pub(super) fn sync_append_child_to_render_document(&self, parent: &NodePtr, child: &NodePtr) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_append_child(parent, child);
        }
    }

    pub(super) fn sync_insert_before_to_render_document(
        &self,
        parent: &NodePtr,
        new_child: &NodePtr,
        ref_child: Option<&NodePtr>,
    ) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document
                .borrow_mut()
                .sync_insert_before(parent, new_child, ref_child);
        }
    }

    pub(super) fn sync_remove_child_from_render_document(&self, child: &NodePtr) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_remove_child(child);
        }
    }

    pub(super) fn sync_replace_child_in_render_document(
        &self,
        parent: &NodePtr,
        new_child: &NodePtr,
        old_child: &NodePtr,
    ) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document
                .borrow_mut()
                .sync_replace_child(parent, new_child, old_child);
        }
    }

    pub(super) fn sync_attribute_to_render_document(
        &self,
        node: &NodePtr,
        name: &str,
        value: &str,
    ) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document
                .borrow_mut()
                .sync_set_attribute(node, name, value);
        }
    }

    pub(super) fn sync_remove_attribute_from_render_document(&self, node: &NodePtr, name: &str) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document
                .borrow_mut()
                .sync_remove_attribute(node, name);
        }
    }

    pub(super) fn sync_all_attributes_to_render_document(&self, node: &NodePtr) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_all_attributes(node);
        }
    }

    pub(super) fn sync_text_to_render_document(&self, node: &NodePtr, text: &str) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_text_node(node, text);
        }
    }

    pub(super) fn sync_shadow_root_to_render_document(
        &self,
        host: &NodePtr,
        shadow_root: &NodePtr,
    ) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document
                .borrow_mut()
                .sync_attach_shadow_root(host, shadow_root);
        }
    }

    pub(super) fn sync_clear_children_in_render_document(&self, parent: &NodePtr) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_clear_children(parent);
        }
    }

    pub(super) fn sync_children_to_render_document(&self, parent: &NodePtr) {
        if let Some(render_document) = self.render_document.borrow().as_ref().cloned() {
            render_document.borrow_mut().sync_replace_children(parent);
        }
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
