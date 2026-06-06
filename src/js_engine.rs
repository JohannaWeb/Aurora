use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};

/// Common interface implemented by every JS engine backend (SpiderMonkey, Boa, …).
///
/// All methods take `&mut self` so the trait is object-safe and can be stored as
/// `Box<dyn JsRuntime>` without any generic parameters leaking into callers.
pub(crate) trait JsRuntime {
    fn execute(&mut self, script: &str) -> Result<(), String>;

    fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<LayoutTree>>,
        stylesheet: Rc<RefCell<Stylesheet>>,
        viewport: Rc<RefCell<ViewportSize>>,
    );

    fn clear_dirty_bits(&mut self);
    fn has_dirty_bits(&self) -> bool;
    fn take_needs_reflow(&mut self) -> bool;

    fn tick(&mut self, now: Instant) -> bool;
    fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool;

    fn dispatch_event(&mut self, node: &NodePtr, event_type: &str) -> bool;

    fn next_deadline(&self) -> Option<Instant>;
    fn has_animation_frame_callbacks(&self) -> bool;
    fn has_ready_work(&self, now: Instant) -> bool;
}
