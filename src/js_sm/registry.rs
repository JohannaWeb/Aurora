use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};
use mozjs::gc::RootedTraceableBox;
use mozjs::jsapi::{Heap, JSObject};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct DirtyState {
    pub(super) needs_reflow: bool,
    pub(super) needs_style: bool,
}

pub(super) struct NodeRegistry {
    pub(super) nodes: BTreeMap<u32, NodePtr>,
    reverse_nodes: BTreeMap<usize, u32>,
    pub(super) next_id: u32,
    dirty: DirtyState,
    pub(super) layout_tree: Option<Rc<RefCell<LayoutTree>>>,
    pub(super) stylesheet: Option<Rc<RefCell<Stylesheet>>>,
    pub(super) viewport: Option<Rc<RefCell<ViewportSize>>>,
    pub(super) document: Option<NodePtr>,
    pub(super) js_wrappers: BTreeMap<u32, RootedTraceableBox<Heap<*mut JSObject>>>,
    /// event listeners: node_id → event_type → Vec<callback_id>
    /// callback_id N means `window.__cb{N}__` holds the JS function.
    pub(super) listeners: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
}

impl NodeRegistry {
    pub(super) fn new() -> Self {
        NodeRegistry {
            nodes: BTreeMap::new(),
            reverse_nodes: BTreeMap::new(),
            next_id: 1,
            dirty: DirtyState::default(),
            layout_tree: None,
            stylesheet: None,
            viewport: None,
            document: None,
            js_wrappers: BTreeMap::new(),
            listeners: BTreeMap::new(),
        }
    }

    pub(super) fn register(&mut self, node: NodePtr) -> u32 {
        let addr = Rc::as_ptr(&node) as usize;
        if let Some(&id) = self.reverse_nodes.get(&addr) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(id, node.clone());
        self.reverse_nodes.insert(addr, id);
        id
    }

    pub(super) fn node_id(&self, node: &NodePtr) -> Option<u32> {
        let addr = Rc::as_ptr(node) as usize;
        self.reverse_nodes.get(&addr).copied()
    }

    pub(super) fn lookup(&self, id: u32) -> Option<NodePtr> {
        self.nodes.get(&id).cloned()
    }

    pub(super) fn lookup_js_wrapper(&self, id: u32) -> Option<*mut JSObject> {
        self.js_wrappers.get(&id).map(|wrapper| wrapper.get())
    }

    pub(super) fn cache_js_wrapper(&mut self, id: u32, obj: *mut JSObject) {
        self.js_wrappers
            .entry(id)
            .or_insert_with(|| RootedTraceableBox::from_box(Heap::boxed(obj)));
    }

    pub(super) fn add_listener(&mut self, node_id: u32, event_type: String, cb_id: u32) {
        self.listeners
            .entry(node_id)
            .or_default()
            .entry(event_type)
            .or_default()
            .push(cb_id);
    }

    pub(super) fn get_listener_ids(&self, node_id: u32, event_type: &str) -> Vec<u32> {
        self.listeners
            .get(&node_id)
            .and_then(|m| m.get(event_type))
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<LayoutTree>>,
        stylesheet: Rc<RefCell<Stylesheet>>,
        viewport: Rc<RefCell<ViewportSize>>,
        document: NodePtr,
    ) {
        self.layout_tree = Some(layout_tree);
        self.stylesheet = Some(stylesheet);
        self.viewport = Some(viewport);
        self.document = Some(document);
    }

    pub(super) fn mark_needs_reflow(&mut self) {
        self.dirty.needs_reflow = true;
    }

    pub(super) fn take_needs_reflow(&mut self) -> bool {
        // Note: called via &mut SmState which requires &mut SmRuntime
        let v = self.dirty.needs_reflow;
        self.dirty.needs_reflow = false;
        v
    }

    pub(super) fn has_dirty_bits(&self) -> bool {
        self.dirty.needs_reflow || self.dirty.needs_style
    }

    pub(super) fn clear_dirty_bits(&mut self) {
        self.dirty = DirtyState::default();
    }

    pub(super) fn perform_sync_reflow(&self) {
        // Trigger layout recalc if shared layout tree is available.
        if let (Some(lt), Some(ss), Some(vp), Some(doc)) = (
            &self.layout_tree,
            &self.stylesheet,
            &self.viewport,
            &self.document,
        ) {
            let viewport_size = *vp.borrow();
            let style_tree = crate::style::StyleTree::from_dom(doc, &ss.borrow());
            *lt.borrow_mut() = crate::layout::LayoutTree::from_style_tree_with_viewport(
                &style_tree,
                crate::layout::ViewportSize {
                    width: viewport_size.width,
                    height: (viewport_size.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
                },
            );
        }
    }
}
