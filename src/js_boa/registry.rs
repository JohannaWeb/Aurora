use super::*;

#[derive(Clone)]
pub(super) struct NodeRegistry {
    pub(super) nodes: Rc<RefCell<BTreeMap<u32, NodePtr>>>,
    pub(super) next_id: Rc<RefCell<u32>>,
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
}
