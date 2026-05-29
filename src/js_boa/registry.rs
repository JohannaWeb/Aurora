use super::*;

#[derive(Clone)]
pub(super) struct NodeRegistry {
    pub(super) nodes: Rc<RefCell<BTreeMap<u32, NodePtr>>>,
    /// Reverse map: Rc pointer address → node ID. O(1) dispatch lookup.
    reverse_nodes: Rc<RefCell<BTreeMap<usize, u32>>>,
    pub(super) next_id: Rc<RefCell<u32>>,
    dirty: Rc<RefCell<DirtyState>>,
    pub(super) layout_tree: Rc<RefCell<Option<Rc<RefCell<crate::layout::LayoutTree>>>>>,
    pub(super) stylesheet: Rc<RefCell<Option<Rc<RefCell<crate::css::Stylesheet>>>>>,
    pub(super) viewport: Rc<RefCell<Option<Rc<RefCell<crate::layout::ViewportSize>>>>>,
    pub(super) document: Rc<RefCell<Option<NodePtr>>>,
    pub(super) listeners: Rc<RefCell<BTreeMap<u32, BTreeMap<String, Vec<JsObject>>>>>,
}

unsafe impl Trace for NodeRegistry {
    empty_trace!();
}
impl Finalize for NodeRegistry {}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        Self {
            nodes: Rc::new(RefCell::new(BTreeMap::new())),
            reverse_nodes: Rc::new(RefCell::new(BTreeMap::new())),
            next_id: Rc::new(RefCell::new(1)),
            dirty: Rc::new(RefCell::new(DirtyState::default())),
            layout_tree: Rc::new(RefCell::new(None)),
            stylesheet: Rc::new(RefCell::new(None)),
            viewport: Rc::new(RefCell::new(None)),
            document: Rc::new(RefCell::new(None)),
            listeners: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    pub(super) fn add_event_listener(&self, node_id: u32, event_type: String, callback: JsObject) {
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
        node_id: u32,
        event_type: &str,
        callback: &JsObject,
    ) {
        let mut listeners = self.listeners.borrow_mut();
        let Some(by_type) = listeners.get_mut(&node_id) else {
            return;
        };
        let Some(callbacks) = by_type.get_mut(event_type) else {
            return;
        };
        callbacks.retain(|stored| !JsObject::equals(stored, callback));
    }

    pub(super) fn get_listeners(&self, node_id: u32, event_type: &str) -> Vec<JsObject> {
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
        // If this node was already registered, return its existing ID so that
        // event listeners stored under the original ID remain reachable.
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

    /// O(1) lookup of a node's registered ID. Returns None if not registered.
    pub(super) fn node_id(&self, node: &NodePtr) -> Option<u32> {
        let key = Rc::as_ptr(node) as usize;
        self.reverse_nodes.borrow().get(&key).copied()
    }

    pub(super) fn mark_style_dirty(&self, node: &NodePtr) {
        self.dirty.borrow_mut().style = true;
    }

    pub(super) fn mark_layout_dirty(&self, node: &NodePtr) {
        let mut dirty = self.dirty.borrow_mut();
        dirty.style = true;
        dirty.layout = true;
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

    pub(super) fn perform_sync_reflow(&self) {
        if !self.has_dirty_bits() {
            return;
        }

        let layout_tree_opt = self.layout_tree.borrow();
        let stylesheet_opt = self.stylesheet.borrow();
        let viewport_opt = self.viewport.borrow();
        let document_opt = self.document.borrow();

        if let (Some(layout_tree), Some(stylesheet), Some(viewport), Some(document)) = (
            layout_tree_opt.as_ref(),
            stylesheet_opt.as_ref(),
            viewport_opt.as_ref(),
            document_opt.as_ref(),
        ) {
            let viewport_val = *viewport.borrow();
            let content_viewport = crate::layout::ViewportSize {
                width: viewport_val.width,
                height: (viewport_val.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
            };

            let style_tree = crate::style::StyleTree::from_dom(document, &stylesheet.borrow());

            let new_layout = crate::layout::LayoutTree::from_style_tree_with_viewport(
                &style_tree,
                content_viewport,
            );

            *layout_tree.borrow_mut() = new_layout;
            self.clear_dirty_bits();
        }
    }
}

#[derive(Default)]
struct DirtyState {
    style: bool,
    layout: bool,
}
