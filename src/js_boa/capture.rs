use super::*;

#[derive(Clone)]
pub(super) struct NodeCapture {
    pub(super) node: NodePtr,
    pub(super) registry: NodeRegistry,
    pub(super) document: NodePtr,
}
unsafe impl Trace for NodeCapture {
    empty_trace!();
}
impl Finalize for NodeCapture {}

#[derive(Clone)]
pub(super) struct DocCapture {
    pub(super) document: NodePtr,
    pub(super) registry: NodeRegistry,
}
unsafe impl Trace for DocCapture {
    empty_trace!();
}
impl Finalize for DocCapture {}

#[derive(Clone)]
pub(super) struct WindowCapture {
    pub(super) storage: Rc<RefCell<BTreeMap<String, String>>>,
    pub(super) session: Rc<RefCell<BTreeMap<String, String>>>,
    pub(super) next_timer: Rc<RefCell<u32>>,
}
unsafe impl Trace for WindowCapture {
    empty_trace!();
}
impl Finalize for WindowCapture {}
