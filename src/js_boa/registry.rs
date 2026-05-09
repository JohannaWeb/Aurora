use super::*;

#[derive(Clone)]
pub(super) struct NodeRegistry {
    pub(super) nodes: Rc<RefCell<BTreeMap<u32, NodePtr>>>,
    pub(super) next_id: Rc<RefCell<u32>>,
    dirty: Rc<RefCell<DirtyState>>,
}

unsafe impl Trace for NodeRegistry {
    empty_trace!();
}
impl Finalize for NodeRegistry {}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        Self {
            nodes: Rc::new(RefCell::new(BTreeMap::new())),
            next_id: Rc::new(RefCell::new(1)),
            dirty: Rc::new(RefCell::new(DirtyState::default())),
        }
    }

    pub(super) fn register(&self, node: NodePtr) -> u32 {
        let mut next_id = self.next_id.borrow_mut();
        let id = *next_id;
        *next_id += 1;
        self.nodes.borrow_mut().insert(id, node);
        id
    }

    pub(super) fn lookup(&self, id: u32) -> Option<NodePtr> {
        self.nodes.borrow().get(&id).cloned()
    }

    pub(super) fn mark_style_dirty(&self, _node: &NodePtr) {
        self.dirty.borrow_mut().style = true;
    }

    pub(super) fn mark_layout_dirty(&self, _node: &NodePtr) {
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
}

#[derive(Default)]
struct DirtyState {
    style: bool,
    layout: bool,
}
