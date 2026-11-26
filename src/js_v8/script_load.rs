//! Scripts that page script creates and inserts: fetched, run (in insertion
//! order unless `async`), then `load` or `error` fired at them.
//!
//! <https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element>,
//! for external classic scripts only. Parser-inserted scripts are run by the
//! page loader instead, and innerHTML never runs scripts, so only elements
//! made with `createElement("script")` start here. Not covered yet: inline
//! script-created scripts, module scripts, and clones of unstarted scripts.

use super::registry::NodeRegistry;
use super::tree::mutation::is_connected_to;
use crate::blitz_document::TextLoad;
use crate::dom::{Node, NodePtr};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Scripts {
    /// Script-created scripts that have not started, by node id, with whether
    /// they are still "non-blocking" (async until `async` is set to false).
    unstarted: BTreeMap<u32, bool>,
    /// Started external scripts, in the order they started.
    loading: Vec<LoadingScript>,
}

struct LoadingScript {
    node: NodePtr,
    /// `None` when there is nothing to load from, which fails the script.
    load: Option<TextLoad>,
    /// Runs after the in-order scripts that started before it.
    in_order: bool,
}

impl LoadingScript {
    fn is_finished(&self) -> bool {
        self.load.as_ref().is_none_or(TextLoad::is_finished)
    }
}

fn is_script(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name.eq_ignore_ascii_case("script"))
}

fn attribute(node: &NodePtr, name: &str) -> Option<String> {
    match &*node.borrow() {
        Node::Element(el) => el.attributes.get(name).cloned(),
        _ => None,
    }
}

/// `createElement("script")` made `node`.
pub(super) fn created(registry: &Rc<NodeRegistry>, node: &NodePtr) {
    let id = registry.register(node.clone());
    registry.scripts.borrow_mut().unstarted.insert(id, true);
}

/// <https://html.spec.whatwg.org/multipage/scripting.html#dom-script-async>
pub(super) fn is_async(registry: &Rc<NodeRegistry>, node: &NodePtr) -> bool {
    let id = registry.register(node.clone());
    let non_blocking = registry.scripts.borrow().unstarted.get(&id).copied().unwrap_or(false);
    non_blocking || attribute(node, "async").is_some()
}

/// Setting `async` from script clears "non-blocking".
pub(super) fn async_set(registry: &Rc<NodeRegistry>, node: &NodePtr) {
    let id = registry.register(node.clone());
    if let Some(non_blocking) = registry.scripts.borrow_mut().unstarted.get_mut(&id) {
        *non_blocking = false;
    }
}

/// Call after inserting `roots`: starts the scripts among them and their
/// descendants that became connected.
pub(super) fn inserted(registry: &Rc<NodeRegistry>, roots: &[NodePtr]) {
    let mut stack: Vec<NodePtr> = roots.iter().rev().cloned().collect();
    while let Some(node) = stack.pop() {
        if is_script(&node) {
            prepare(registry, &node);
        }
        if let Node::Element(el) = &*node.borrow() {
            stack.extend(el.shadow_root.iter().cloned());
            stack.extend(el.children.iter().rev().cloned());
        }
    }
}

/// Call after `src` was set: a connected script that has not started starts.
pub(super) fn src_set(registry: &Rc<NodeRegistry>, node: &NodePtr) {
    if is_script(node) {
        prepare(registry, node);
    }
}

fn prepare(registry: &Rc<NodeRegistry>, node: &NodePtr) {
    let id = registry.register(node.clone());
    let Some(non_blocking) = registry.scripts.borrow().unstarted.get(&id).copied() else {
        return;
    };
    let document = registry.document.borrow().clone();
    if !document.is_some_and(|document| is_connected_to(&document, node)) {
        return;
    }
    if !crate::runner::scripts::is_classic_script_type(attribute(node, "type").as_deref()) {
        return;
    }
    let Some(src) = attribute(node, "src") else {
        return;
    };
    registry.scripts.borrow_mut().unstarted.remove(&id);
    let load = (!src.is_empty()).then(|| registry.load_script(&src)).flatten();
    let in_order = !non_blocking && attribute(node, "async").is_none();
    registry.scripts.borrow_mut().loading.push(LoadingScript { node: node.clone(), load, in_order });
}

pub(super) fn has_pending(registry: &NodeRegistry) -> bool {
    !registry.scripts.borrow().loading.is_empty()
}

pub(super) fn has_ready(registry: &NodeRegistry) -> bool {
    let scripts = registry.scripts.borrow();
    let first_in_order = scripts.loading.iter().find(|script| script.in_order);
    scripts.loading.iter().any(|script| !script.in_order && script.is_finished())
        || first_in_order.is_some_and(LoadingScript::is_finished)
}

/// Takes the scripts that may run now, in order, with their source or why
/// they failed: async ones that finished, and finished in-order ones not
/// waiting on an earlier in-order one.
pub(super) fn take_ready(registry: &NodeRegistry) -> Vec<(NodePtr, Result<String, String>)> {
    let mut ready = Vec::new();
    let mut blocked = false;
    registry.scripts.borrow_mut().loading.retain(|script| {
        let finished = script.is_finished();
        if script.in_order && (blocked || !finished) {
            blocked = true;
            return true;
        }
        if !finished {
            return true;
        }
        let result = match &script.load {
            Some(load) => load.take().unwrap_or_else(|| Err("taken twice".into())),
            None => Err("no URL to load".into()),
        };
        ready.push((script.node.clone(), result));
        false
    });
    ready
}
