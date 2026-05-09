use super::*;
use boa_gc::Tracer;
use std::time::{Duration, Instant};

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
    #[allow(dead_code)]
    pub(super) storage: Rc<RefCell<BTreeMap<String, String>>>,
    #[allow(dead_code)]
    pub(super) session: Rc<RefCell<BTreeMap<String, String>>>,
    pub(super) next_timer: Rc<RefCell<u32>>,
    pub(super) timers: Rc<RefCell<Vec<TimerEntry>>>,
    pub(super) animation_frames: Rc<RefCell<Vec<AnimationFrameEntry>>>,
    pub(super) microtasks: Rc<RefCell<Vec<JsObject>>>,
    pub(super) time_origin: Instant,
}
unsafe impl Trace for WindowCapture {
    unsafe fn trace(&self, tracer: &mut Tracer) {
        for entry in self.timers.borrow().iter() {
            Trace::trace(entry, tracer);
        }
        for entry in self.animation_frames.borrow().iter() {
            Trace::trace(entry, tracer);
        }
        for callback in self.microtasks.borrow().iter() {
            Trace::trace(callback, tracer);
        }
    }

    unsafe fn trace_non_roots(&self) {
        for entry in self.timers.borrow().iter() {
            Trace::trace_non_roots(entry);
        }
        for entry in self.animation_frames.borrow().iter() {
            Trace::trace_non_roots(entry);
        }
        for callback in self.microtasks.borrow().iter() {
            Trace::trace_non_roots(callback);
        }
    }

    fn run_finalizer(&self) {
        Finalize::finalize(self);
        for entry in self.timers.borrow().iter() {
            Trace::run_finalizer(entry);
        }
        for entry in self.animation_frames.borrow().iter() {
            Trace::run_finalizer(entry);
        }
        for callback in self.microtasks.borrow().iter() {
            Trace::run_finalizer(callback);
        }
    }
}
impl Finalize for WindowCapture {}

#[derive(Clone, Trace, Finalize)]
pub(super) struct TimerEntry {
    pub(super) id: u32,
    #[unsafe_ignore_trace]
    pub(super) deadline: Instant,
    #[unsafe_ignore_trace]
    pub(super) interval: Option<Duration>,
    pub(super) callback: JsObject,
}

#[derive(Clone, Trace, Finalize)]
pub(super) struct AnimationFrameEntry {
    pub(super) id: u32,
    pub(super) callback: JsObject,
}
