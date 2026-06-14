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

    pub(super) fn mark_style_dirty(&self, _node: &NodePtr) {
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
