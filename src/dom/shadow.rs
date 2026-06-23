//! Shadow DOM backend abstraction.
//!
//! Aurora does not implement native Shadow DOM. Instead it models a shadow root
//! as a `#document-fragment` parented under its host element, and the Blitz
//! mirror flattens that fragment into a synthetic
//! `<div data-aurora-shadow-root="true">` (see
//! `BlitzDocument::sync_attach_shadow_root`). The composed-tree visibility and
//! shadow-boundary checks used by selector queries are derived from those
//! synthetic markers.
//!
//! That behavior is currently spread across `blitz_document.rs` and the V8
//! bridge. This module isolates it behind [`ShadowTreeBackend`] so the synthetic
//! semantics can be tightened — or eventually replaced with native shadow
//! semantics — without reworking every call site. The only implementation today
//! is [`SyntheticShadowTreeBackend`], which reproduces the existing behavior
//! exactly; this is an abstraction step, not a behavior change.

use super::node::{Node, NodePtr};
use super::{parent_ptr, set_parent};
use std::rc::Rc;

/// Strategy for representing Shadow DOM in Aurora.
///
/// Operates on the authoritative legacy DOM (`NodePtr` tree); the Blitz mirror's
/// synthetic shadow node is a rendering detail produced separately. Every method
/// describes shadow structure as JS can observe it.
///
/// `attach_shadow` and the shadow-boundary predicates are wired into the live
/// V8 bridge and `BlitzDocument` today. `append_shadow_child`, `composed_children`,
/// `host_for_shadow_root`, and `is_in_shadow_tree` are the migration surface for
/// Task 4.2 (Shadow DOM semantics) and are currently exercised only by tests, so
/// they are allowed to be unused in production builds for now.
#[allow(dead_code)]
pub trait ShadowTreeBackend {
    /// Attach a shadow root to `host`, returning the shadow root fragment.
    ///
    /// Idempotent: a host that already has a shadow root returns the existing
    /// one rather than replacing it, matching `Element.attachShadow` semantics
    /// for already-hosting elements. `mode` is recorded by the renderer when the
    /// fragment is mirrored; the legacy tree only needs the host↔root link.
    fn attach_shadow(&self, host: &NodePtr, mode: &str) -> NodePtr;

    /// Adopt an existing document fragment as `host`'s shadow root.
    ///
    /// ShadyDOM creates logical roots as ordinary detached fragments and later
    /// exposes them through a component's `root`/`shadowRoot` properties. The
    /// engine must link that exact fragment to the host rather than allocating a
    /// second root, otherwise connectivity and rendering operate on different
    /// trees.
    fn adopt_shadow_root(&self, host: &NodePtr, shadow_root: &NodePtr) -> bool;

    /// Append `child` into `shadow_root`'s flattened child list.
    fn append_shadow_child(&self, shadow_root: &NodePtr, child: &NodePtr);

    /// The composed-tree children of `node`: its light-DOM children followed by
    /// its shadow root fragment, if any. This mirrors how the renderer flattens
    /// a host (light children plus the synthetic shadow container).
    fn composed_children(&self, node: &NodePtr) -> Vec<NodePtr>;

    /// Distribute light-DOM children of `host` into its shadow tree's slots.
    fn distribute_slots(&self, host: &NodePtr);

    /// The light-DOM nodes assigned to `slot`.
    fn assigned_nodes(&self, slot: &NodePtr) -> Vec<NodePtr>;

    /// The host element that owns `shadow_root`, if `shadow_root` is a shadow
    /// root.
    fn host_for_shadow_root(&self, shadow_root: &NodePtr) -> Option<NodePtr>;

    /// Whether `node` is itself a shadow root (a `#document-fragment` registered
    /// as its parent's `shadow_root`).
    fn is_shadow_root(&self, node: &NodePtr) -> bool;

    /// The nearest enclosing shadow root of `node`, walking up parent pointers;
    /// returns `node` itself if it is a shadow root.
    fn nearest_shadow_root(&self, node: &NodePtr) -> Option<NodePtr>;

    /// Whether `node` sits inside (or is) any shadow tree.
    fn is_in_shadow_tree(&self, node: &NodePtr) -> bool {
        self.nearest_shadow_root(node).is_some()
    }
}

/// The current synthetic shadow implementation backed by `#document-fragment`
/// shadow roots and `data-aurora-shadow-*` Blitz markers.
#[derive(Debug, Default, Clone, Copy)]
pub struct SyntheticShadowTreeBackend;

fn is_document_fragment(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name == "#document-fragment")
}

fn find_slots(root: &NodePtr, out: &mut Vec<NodePtr>) {
    let node = root.borrow();
    if let Node::Element(el) = &*node {
        if el.tag_name == "slot" {
            out.push(root.clone());
        }
        for child in &el.children {
            find_slots(child, out);
        }
    }
}

impl ShadowTreeBackend for SyntheticShadowTreeBackend {
    fn attach_shadow(&self, host: &NodePtr, _mode: &str) -> NodePtr {
        let shadow_root = match &mut *host.borrow_mut() {
            Node::Element(el) => el
                .shadow_root
                .get_or_insert_with(|| Node::document_fragment(Vec::new()))
                .clone(),
            _ => Node::document_fragment(Vec::new()),
        };
        set_parent(&shadow_root, host);
        shadow_root
    }

    fn adopt_shadow_root(&self, host: &NodePtr, shadow_root: &NodePtr) -> bool {
        if !is_document_fragment(shadow_root) {
            return false;
        }

        // A logical root is detached by construction, but defensively remove it
        // from a regular parent if a polyfill temporarily inserted it there.
        if let Some(previous_parent) = parent_ptr(shadow_root)
            && !Rc::ptr_eq(&previous_parent, host)
        {
            if let Node::Element(el) = &mut *previous_parent.borrow_mut() {
                el.children.retain(|child| !Rc::ptr_eq(child, shadow_root));
            }
        }

        let previous_root = match &mut *host.borrow_mut() {
            Node::Element(el) => el.shadow_root.replace(shadow_root.clone()),
            _ => return false,
        };
        if let Some(previous_root) = previous_root
            && !Rc::ptr_eq(&previous_root, shadow_root)
        {
            super::clear_parent(&previous_root);
        }
        set_parent(shadow_root, host);
        super::reparent_subtree(shadow_root);
        true
    }

    fn append_shadow_child(&self, shadow_root: &NodePtr, child: &NodePtr) {
        if let Node::Element(el) = &mut *shadow_root.borrow_mut() {
            el.children.push(child.clone());
        }
        set_parent(child, shadow_root);
    }

    fn composed_children(&self, node: &NodePtr) -> Vec<NodePtr> {
        let (tag_name, shadow_root, _template_contents, children, assigned_nodes) = {
            let node_borrow = node.borrow();
            match &*node_borrow {
                Node::Element(el) => (
                    Some(el.tag_name.clone()),
                    el.shadow_root.clone(),
                    el.template_contents.clone(),
                    el.children.clone(),
                    Some(el.assigned_nodes.clone()),
                ),
                Node::Document { children, .. } => (None, None, None, children.clone(), None),
                Node::Text(_) => return Vec::new(),
            }
        };

        if let Some(shadow_root) = shadow_root {
            // Host element: the shadow root itself is the single composed child.
            // Its own children (with slots expanded) are computed when
            // composed_children is called on the shadow root fragment.
            return vec![shadow_root];
        }
        if tag_name.as_deref() == Some("slot") {
            return assigned_nodes.unwrap_or_default();
        }

        // Normal element or shadow root: collect children
        let base_children = children;

        base_children
            .into_iter()
            .flat_map(|c| {
                let is_slot = c
                    .borrow()
                    .as_element()
                    .is_some_and(|e| e.tag_name == "slot");
                if is_slot {
                    self.composed_children(&c)
                } else {
                    vec![c]
                }
            })
            .collect()
    }

    fn distribute_slots(&self, host: &NodePtr) {
        let shadow_root = match &*host.borrow() {
            Node::Element(el) => el.shadow_root.clone(),
            _ => return,
        };
        let Some(shadow_root) = shadow_root else {
            return;
        };

        let mut slots = Vec::new();
        find_slots(&shadow_root, &mut slots);

        let light_children = match &*host.borrow() {
            Node::Element(el) => el.children.clone(),
            _ => Vec::new(),
        };

        for slot in &slots {
            if let Some(el) = slot.borrow_mut().as_element_mut() {
                el.assigned_nodes.clear();
            }
        }

        for child in light_children {
            let slot_name = child
                .borrow()
                .as_element()
                .and_then(|el| el.attributes.get("slot").cloned());

            let target_slot = slots.iter().find(|slot| {
                let slot_borrow = slot.borrow();
                let Some(slot_el) = slot_borrow.as_element() else {
                    return false;
                };
                let name = slot_el.attributes.get("name");
                match (slot_name.as_deref(), name.map(|s| s.as_str())) {
                    (Some(sn), Some(n)) => sn == n,
                    (None, None) | (None, Some("")) => true,
                    _ => false,
                }
            });

            if let Some(slot) = target_slot {
                if let Some(slot_el) = slot.borrow_mut().as_element_mut() {
                    slot_el.assigned_nodes.push(child);
                }
            }
        }
    }

    fn assigned_nodes(&self, slot: &NodePtr) -> Vec<NodePtr> {
        slot.borrow()
            .as_element()
            .map(|el| el.assigned_nodes.clone())
            .unwrap_or_default()
    }

    fn host_for_shadow_root(&self, shadow_root: &NodePtr) -> Option<NodePtr> {
        if self.is_shadow_root(shadow_root) {
            parent_ptr(shadow_root)
        } else {
            None
        }
    }

    fn is_shadow_root(&self, node: &NodePtr) -> bool {
        if !is_document_fragment(node) {
            return false;
        }
        let Some(parent) = parent_ptr(node) else {
            return false;
        };
        match &*parent.borrow() {
            Node::Element(el) => el
                .shadow_root
                .as_ref()
                .is_some_and(|shadow_root| Rc::ptr_eq(shadow_root, node)),
            _ => false,
        }
    }

    fn nearest_shadow_root(&self, node: &NodePtr) -> Option<NodePtr> {
        let mut current = Some(node.clone());
        while let Some(candidate) = current {
            if self.is_shadow_root(&candidate) {
                return Some(candidate);
            }
            current = parent_ptr(&candidate);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> SyntheticShadowTreeBackend {
        SyntheticShadowTreeBackend
    }

    #[test]
    fn attach_shadow_links_root_to_host_and_is_idempotent() {
        let host = Node::element("my-el", Vec::new());

        let root = backend().attach_shadow(&host, "open");

        // The fragment is recorded as the host's shadow root and parented to it.
        assert!(backend().is_shadow_root(&root));
        let host_ref = host.borrow();
        let Node::Element(el) = &*host_ref else {
            panic!("expected element host");
        };
        assert!(
            el.shadow_root
                .as_ref()
                .is_some_and(|sr| Rc::ptr_eq(sr, &root))
        );
        drop(host_ref);
        assert!(parent_ptr(&root).is_some_and(|p| Rc::ptr_eq(&p, &host)));

        // A second attach returns the same root rather than replacing it.
        let root_again = backend().attach_shadow(&host, "open");
        assert!(Rc::ptr_eq(&root, &root_again));
    }

    #[test]
    fn append_shadow_child_populates_root_and_parents_child() {
        let host = Node::element("my-el", Vec::new());
        let root = backend().attach_shadow(&host, "open");
        let child = Node::element("span", Vec::new());

        backend().append_shadow_child(&root, &child);

        let root_ref = root.borrow();
        let Node::Element(el) = &*root_ref else {
            panic!("expected fragment");
        };
        assert_eq!(el.children.len(), 1);
        assert!(Rc::ptr_eq(&el.children[0], &child));
        drop(root_ref);
        assert!(parent_ptr(&child).is_some_and(|p| Rc::ptr_eq(&p, &root)));
        // The child is now inside a shadow tree.
        assert!(backend().is_in_shadow_tree(&child));
    }

    #[test]
    fn host_for_shadow_root_resolves_only_for_roots() {
        let host = Node::element("my-el", Vec::new());
        let root = backend().attach_shadow(&host, "open");
        assert!(
            backend()
                .host_for_shadow_root(&root)
                .is_some_and(|h| Rc::ptr_eq(&h, &host))
        );

        // A plain detached fragment is not a shadow root.
        let stray = Node::document_fragment(Vec::new());
        assert!(backend().host_for_shadow_root(&stray).is_none());
        assert!(!backend().is_shadow_root(&stray));
        assert!(!backend().is_in_shadow_tree(&host));
    }

    #[test]
    fn nearest_shadow_root_walks_up_to_enclosing_root() {
        let host = Node::element("my-el", Vec::new());
        let root = backend().attach_shadow(&host, "open");
        let mid = Node::element("div", Vec::new());
        let leaf = Node::element("span", Vec::new());
        backend().append_shadow_child(&root, &mid);
        if let Node::Element(el) = &mut *mid.borrow_mut() {
            el.children.push(leaf.clone());
        }
        set_parent(&leaf, &mid);

        assert!(
            backend()
                .nearest_shadow_root(&leaf)
                .is_some_and(|r| Rc::ptr_eq(&r, &root))
        );
        assert!(
            backend()
                .nearest_shadow_root(&root)
                .is_some_and(|r| Rc::ptr_eq(&r, &root))
        );
        // A light-DOM node outside any shadow tree has none.
        assert!(backend().nearest_shadow_root(&host).is_none());
    }

    #[test]
    fn composed_children_returns_shadow_root_as_sole_child_of_host() {
        let light = Node::element("p", Vec::new());
        let host = Node::element("my-el", vec![light.clone()]);
        let root = backend().attach_shadow(&host, "open");

        let composed = backend().composed_children(&host);
        assert_eq!(composed.len(), 1);
        assert!(Rc::ptr_eq(&composed[0], &root));

        // A host without a shadow root composes to just its light children.
        let plain = Node::element("div", vec![Node::element("b", Vec::new())]);
        assert_eq!(backend().composed_children(&plain).len(), 1);
    }
}
