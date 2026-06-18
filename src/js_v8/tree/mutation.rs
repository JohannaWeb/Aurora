use super::*;
use crate::js_v8::mutation_observer;
use crate::js_v8::registry::NodeRegistry;
use crate::window::SnapshotRebuildReason;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DomMutationKind {
    AppendChild,
    PrependChild,
    InsertBefore,
    RemoveChild,
    ReplaceChild,
    SetAttribute,
    RemoveAttribute,
    SetTextContent,
    ReplaceChildren,
    AttachShadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DomMutationResult {
    pub(crate) kind: DomMutationKind,
    pub(crate) render_synced: bool,
    pub(crate) target_id: u32,
    pub(crate) changed: bool,
}

pub(crate) enum DomMutation<'a> {
    AppendChild {
        parent: &'a NodePtr,
        child: &'a NodePtr,
    },
    PrependChild {
        parent: &'a NodePtr,
        child: &'a NodePtr,
    },
    InsertBefore {
        parent: &'a NodePtr,
        new_child: &'a NodePtr,
        ref_child: Option<&'a NodePtr>,
    },
    RemoveChild {
        parent: &'a NodePtr,
        child: &'a NodePtr,
    },
    ReplaceChild {
        parent: &'a NodePtr,
        new_child: &'a NodePtr,
        old_child: &'a NodePtr,
    },
    SetAttribute {
        node: &'a NodePtr,
        name: &'a str,
        value: &'a str,
    },
    RemoveAttribute {
        node: &'a NodePtr,
        name: &'a str,
    },
    SetTextContent {
        node: &'a NodePtr,
        text: String,
    },
    ReplaceChildren {
        node: &'a NodePtr,
        children: Vec<NodePtr>,
    },
    AttachShadow {
        host: &'a NodePtr,
        shadow_root: &'a NodePtr,
        mode: &'a str,
    },
}

pub(crate) fn apply_dom_mutation(
    registry: &Rc<NodeRegistry>,
    mutation: DomMutation<'_>,
) -> DomMutationResult {
    fn schedule_sync_failure(registry: &Rc<NodeRegistry>) {
        registry.schedule_snapshot_rebuild_reason(SnapshotRebuildReason::SyncOperationFailed);
    }

    match mutation {
        DomMutation::AppendChild { parent, child } => {
            let target_id = registry.register(parent.clone());
            let child_id = registry.register(child.clone());
            let render_synced = registry.sync_append_child_to_render_document(parent, child);
            append_child_ptr(parent, child);
            if render_synced {
                mutation_observer::queue_childlist(registry, target_id, vec![child_id], vec![]);
            } else {
                schedule_sync_failure(registry);
            }
            registry.mark_layout_dirty(parent);
            DomMutationResult {
                kind: DomMutationKind::AppendChild,
                render_synced,
                target_id,
                changed: true,
            }
        }
        DomMutation::PrependChild { parent, child } => {
            let target_id = registry.register(parent.clone());
            let child_id = registry.register(child.clone());
            let render_synced = registry.sync_insert_before_to_render_document(parent, child, None);
            prepend_child_ptr(parent, child);
            if render_synced {
                mutation_observer::queue_childlist(registry, target_id, vec![child_id], vec![]);
            } else {
                schedule_sync_failure(registry);
            }
            registry.mark_layout_dirty(parent);
            DomMutationResult {
                kind: DomMutationKind::PrependChild,
                render_synced,
                target_id,
                changed: true,
            }
        }
        DomMutation::InsertBefore {
            parent,
            new_child,
            ref_child,
        } => {
            let target_id = registry.register(parent.clone());
            let new_id = registry.register(new_child.clone());
            let render_synced =
                registry.sync_insert_before_to_render_document(parent, new_child, ref_child);
            insert_before_ptr(parent, new_child, ref_child);
            if render_synced {
                mutation_observer::queue_childlist(registry, target_id, vec![new_id], vec![]);
            } else {
                schedule_sync_failure(registry);
            }
            registry.mark_layout_dirty(parent);
            DomMutationResult {
                kind: DomMutationKind::InsertBefore,
                render_synced,
                target_id,
                changed: true,
            }
        }
        DomMutation::RemoveChild { parent, child } => {
            let target_id = registry.register(parent.clone());
            let child_id = registry.register(child.clone());
            let render_synced = registry.sync_remove_child_from_render_document(child);
            remove_child_ptr(parent, child);
            if render_synced {
                mutation_observer::queue_childlist(registry, target_id, vec![], vec![child_id]);
            } else {
                schedule_sync_failure(registry);
            }
            registry.mark_layout_dirty(parent);
            DomMutationResult {
                kind: DomMutationKind::RemoveChild,
                render_synced,
                target_id,
                changed: true,
            }
        }
        DomMutation::ReplaceChild {
            parent,
            new_child,
            old_child,
        } => {
            let target_id = registry.register(parent.clone());
            let new_id = registry.register(new_child.clone());
            let old_id = registry.register(old_child.clone());
            let render_synced =
                registry.sync_replace_child_in_render_document(parent, new_child, old_child);
            replace_child_ptr(parent, new_child, old_child);
            if render_synced {
                mutation_observer::queue_childlist(registry, target_id, vec![new_id], vec![old_id]);
            } else {
                schedule_sync_failure(registry);
            }
            registry.mark_layout_dirty(parent);
            DomMutationResult {
                kind: DomMutationKind::ReplaceChild,
                render_synced,
                target_id,
                changed: true,
            }
        }
        DomMutation::SetAttribute { node, name, value } => {
            let target_id = registry.register(node.clone());
            let mut changed = false;
            if let Node::Element(el) = &mut *node.borrow_mut() {
                el.attributes.insert(name.to_string(), value.to_string());
                changed = true;
            }
            let render_synced = if changed {
                registry.mark_style_dirty(node);
                let render_synced = registry.sync_attribute_to_render_document(node, name, value);
                if render_synced {
                    mutation_observer::queue_attribute(registry, target_id, name);
                } else {
                    schedule_sync_failure(registry);
                }
                render_synced
            } else {
                true
            };
            DomMutationResult {
                kind: DomMutationKind::SetAttribute,
                render_synced,
                target_id,
                changed,
            }
        }
        DomMutation::RemoveAttribute { node, name } => {
            let target_id = registry.register(node.clone());
            let mut changed = false;
            if let Node::Element(el) = &mut *node.borrow_mut() {
                el.attributes.remove(name);
                changed = true;
            }
            let render_synced = if changed {
                registry.mark_style_dirty(node);
                let render_synced = registry.sync_remove_attribute_from_render_document(node, name);
                if render_synced {
                    mutation_observer::queue_attribute(registry, target_id, name);
                } else {
                    schedule_sync_failure(registry);
                }
                render_synced
            } else {
                true
            };
            DomMutationResult {
                kind: DomMutationKind::RemoveAttribute,
                render_synced,
                target_id,
                changed,
            }
        }
        DomMutation::SetTextContent { node, text } => {
            let target_id = registry.register(node.clone());
            let mut render_synced = true;
            // Apply the structural mutation under the borrow, then release it
            // BEFORE syncing. The render-sync hooks (`sync_text_node`,
            // `sync_clear_children`) walk parent pointers via `parent_ptr`/
            // `is_shadow_root_node`, which re-borrow `node`; holding the
            // `borrow_mut` across the sync call aborts with "already mutably
            // borrowed" (hit constantly by Polymer rewriting `textContent`).
            enum TextTarget {
                TextNode,
                Element,
                Unsupported,
            }
            let target = match &mut *node.borrow_mut() {
                Node::Text(t) => {
                    t.content = text.clone();
                    TextTarget::TextNode
                }
                Node::Element(el) => {
                    el.children = vec![Node::text(text.clone())];
                    TextTarget::Element
                }
                Node::Document { .. } => TextTarget::Unsupported,
            };
            let changed = !matches!(target, TextTarget::Unsupported);
            match target {
                TextTarget::TextNode => {
                    render_synced = registry.sync_text_to_render_document(node, &text);
                    if !render_synced {
                        schedule_sync_failure(registry);
                    }
                }
                TextTarget::Element => {
                    let cleared = registry.sync_clear_children_in_render_document(node);
                    crate::dom::reparent_subtree(node);
                    let reattached = registry.sync_children_to_render_document(node);
                    render_synced = cleared && reattached;
                    if !render_synced {
                        schedule_sync_failure(registry);
                    }
                }
                TextTarget::Unsupported => {}
            }
            DomMutationResult {
                kind: DomMutationKind::SetTextContent,
                render_synced,
                target_id,
                changed,
            }
        }
        DomMutation::ReplaceChildren { node, children } => {
            let target_id = registry.register(node.clone());
            let changed = match &mut *node.borrow_mut() {
                Node::Element(el) => {
                    el.children = children;
                    true
                }
                Node::Document {
                    children: existing, ..
                } => {
                    *existing = children;
                    true
                }
                _ => false,
            };
            let mut render_synced = true;
            if changed {
                let cleared = registry.sync_clear_children_in_render_document(node);
                crate::dom::reparent_subtree(node);
                let reattached = registry.sync_children_to_render_document(node);
                render_synced = cleared && reattached;
                if !render_synced {
                    schedule_sync_failure(registry);
                }
            }
            DomMutationResult {
                kind: DomMutationKind::ReplaceChildren,
                render_synced,
                target_id,
                changed,
            }
        }
        DomMutation::AttachShadow {
            host,
            shadow_root,
            mode,
        } => {
            let target_id = registry.register(host.clone());
            let render_synced =
                registry.sync_shadow_root_to_render_document(host, shadow_root, mode);
            if !render_synced {
                schedule_sync_failure(registry);
            }
            DomMutationResult {
                kind: DomMutationKind::AttachShadow,
                render_synced,
                target_id,
                changed: true,
            }
        }
    }
}

fn take_document_fragment_children(node: &NodePtr) -> Option<Vec<NodePtr>> {
    let mut borrow = node.borrow_mut();
    match &mut *borrow {
        Node::Element(el) if el.tag_name == "#document-fragment" => {
            Some(std::mem::take(&mut el.children))
        }
        _ => None,
    }
}

pub(crate) fn collect_text(node: &NodePtr) -> String {
    let b = node.borrow();
    match &*b {
        Node::Text(t) => t.content.clone(),
        Node::Element(el) => el
            .children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
        Node::Document { children, .. } => children
            .iter()
            .map(collect_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(crate) fn set_text_content(node: &NodePtr, text: &str) {
    match &mut *node.borrow_mut() {
        Node::Element(el) => el.children = vec![Node::text(text.to_string())],
        // Per spec, setting `textContent` on a Text node replaces its data.
        // Without this, writes to a text node (e.g. Polymer binding updates
        // rewriting `[[expr]]` annotations) were silently dropped.
        Node::Text(t) => t.content = text.to_string(),
        Node::Document { .. } => {}
    }
}

pub(crate) fn prepend_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(child) {
        for frag_child in children.into_iter().rev() {
            detach_from_parent(&frag_child);
            prepend_child_ptr(parent, &frag_child);
        }
        return;
    }
    detach_from_parent(child);
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children, .. } => children,
        _ => return,
    };
    kids.insert(0, child.clone());
    drop(p);
    crate::dom::set_parent(child, parent);
}

/// Remove `child` from its current parent's child list, if it has one.
///
/// Insertion is a *move* in the DOM: appending/inserting a node that already
/// lives somewhere first detaches it. Skipping this left the node parented in
/// two places at once (e.g. `fragment.appendChild(div.firstChild)` never emptied
/// the div), which spun YouTube's icon clear-and-rebuild loop forever.
fn detach_from_parent(child: &NodePtr) {
    let Some(parent) = crate::dom::parent_ptr(child) else {
        return;
    };
    let mut p = parent.borrow_mut();
    let kids: &mut Vec<NodePtr> = match &mut *p {
        Node::Element(el) => &mut el.children,
        Node::Document { children, .. } => children,
        _ => return,
    };
    kids.retain(|c| !Rc::ptr_eq(c, child));
}

pub(crate) fn append_child_ptr(parent: &NodePtr, child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(child) {
        for frag_child in children {
            append_child_ptr(parent, &frag_child);
        }
        return;
    }
    detach_from_parent(child);
    let mut appended = false;
    if let Node::Element(el) = &mut *parent.borrow_mut() {
        el.children.push(child.clone());
        appended = true;
    } else if let Node::Document { children, .. } = &mut *parent.borrow_mut() {
        children.push(child.clone());
        appended = true;
    }
    if appended {
        crate::dom::set_parent(child, parent);
    }
}

pub(crate) fn insert_before_ptr(
    parent: &NodePtr,
    new_child: &NodePtr,
    ref_child: Option<&NodePtr>,
) {
    if let Some(children) = take_document_fragment_children(new_child) {
        let mut ref_cursor = ref_child.cloned();
        for frag_child in children {
            insert_before_ptr(parent, &frag_child, ref_cursor.as_ref());
            ref_cursor = Some(frag_child);
        }
        return;
    }
    // Detach first (move semantics), then resolve the ref position so indices are
    // correct even when moving a node within its current parent.
    detach_from_parent(new_child);
    {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        match ref_child.and_then(|rc| kids.iter().position(|c| Rc::ptr_eq(c, rc))) {
            Some(pos) => kids.insert(pos, new_child.clone()),
            None => kids.push(new_child.clone()),
        }
    }
    crate::dom::set_parent(new_child, parent);
}

pub(crate) fn remove_child_ptr(parent: &NodePtr, child: &NodePtr) {
    let removed = {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        let before = kids.len();
        kids.retain(|c| !Rc::ptr_eq(c, child));
        kids.len() != before
    };
    if removed {
        crate::dom::clear_parent(child);
    }
}

pub(crate) fn replace_child_ptr(parent: &NodePtr, new_child: &NodePtr, old_child: &NodePtr) {
    if let Some(children) = take_document_fragment_children(new_child) {
        let mut replaced = false;
        {
            let mut p = parent.borrow_mut();
            let kids: &mut Vec<NodePtr> = match &mut *p {
                Node::Element(el) => &mut el.children,
                Node::Document { children, .. } => children,
                _ => return,
            };
            if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
                kids.remove(pos);
                for (idx, frag_child) in children.into_iter().enumerate() {
                    kids.insert(pos + idx, frag_child.clone());
                    crate::dom::set_parent(&frag_child, parent);
                }
                replaced = true;
            }
        }
        if replaced {
            crate::dom::clear_parent(old_child);
        }
        return;
    }
    detach_from_parent(new_child);
    let replaced = {
        let mut p = parent.borrow_mut();
        let kids: &mut Vec<NodePtr> = match &mut *p {
            Node::Element(el) => &mut el.children,
            Node::Document { children, .. } => children,
            _ => return,
        };
        match kids.iter().position(|c| Rc::ptr_eq(c, old_child)) {
            Some(pos) => {
                kids[pos] = new_child.clone();
                true
            }
            None => false,
        }
    };
    if replaced {
        crate::dom::set_parent(new_child, parent);
        crate::dom::clear_parent(old_child);
    }
}

pub(crate) fn clone_node(node: &NodePtr, deep: bool) -> NodePtr {
    let cloned = {
        let b = node.borrow();
        match &*b {
            Node::Text(t) => Node::text(t.content.clone()),
            Node::Element(el) => {
                let children = if deep {
                    el.children.iter().map(|c| clone_node(c, true)).collect()
                } else {
                    vec![]
                };
                Node::element_with_attributes(el.tag_name.clone(), el.attributes.clone(), children)
            }
            Node::Document { children, mode } => {
                let children = if deep {
                    children.iter().map(|c| clone_node(c, true)).collect()
                } else {
                    vec![]
                };
                Node::document_with_mode(children, *mode)
            }
        }
    };
    if deep {
        crate::dom::reparent_subtree(&cloned);
    }
    cloned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Capability, Identity, IdentityKind};
    use std::cell::RefCell;

    fn registry() -> Rc<NodeRegistry> {
        Rc::new(NodeRegistry::new())
    }

    fn test_identity() -> Identity {
        Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace, Capability::NetworkAccess],
        )
    }

    fn element_children(node: &NodePtr) -> Vec<NodePtr> {
        let Node::Element(el) = &*node.borrow() else {
            panic!("expected element");
        };
        el.children.clone()
    }

    #[test]
    fn dispatcher_applies_append_child_and_marks_dirty() {
        let registry = registry();
        let parent = Node::element("div", Vec::new());
        let child = Node::element("span", Vec::new());

        let result = apply_dom_mutation(
            &registry,
            DomMutation::AppendChild {
                parent: &parent,
                child: &child,
            },
        );

        assert_eq!(result.kind, DomMutationKind::AppendChild);
        assert!(result.render_synced);
        assert_eq!(element_children(&parent).len(), 1);
        assert!(Rc::ptr_eq(&element_children(&parent)[0], &child));
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_applies_insert_before() {
        let registry = registry();
        let first = Node::element("first", Vec::new());
        let second = Node::element("second", Vec::new());
        let parent = Node::element("div", vec![second.clone()]);
        crate::dom::reparent_subtree(&parent);

        let result = apply_dom_mutation(
            &registry,
            DomMutation::InsertBefore {
                parent: &parent,
                new_child: &first,
                ref_child: Some(&second),
            },
        );

        let children = element_children(&parent);
        assert_eq!(result.kind, DomMutationKind::InsertBefore);
        assert!(Rc::ptr_eq(&children[0], &first));
        assert!(Rc::ptr_eq(&children[1], &second));
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_applies_remove_child() {
        let registry = registry();
        let child = Node::element("span", Vec::new());
        let parent = Node::element("div", vec![child.clone()]);
        crate::dom::reparent_subtree(&parent);

        let result = apply_dom_mutation(
            &registry,
            DomMutation::RemoveChild {
                parent: &parent,
                child: &child,
            },
        );

        assert_eq!(result.kind, DomMutationKind::RemoveChild);
        assert!(element_children(&parent).is_empty());
        assert!(crate::dom::parent_ptr(&child).is_none());
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_applies_replace_child() {
        let registry = registry();
        let old_child = Node::element("old", Vec::new());
        let new_child = Node::element("new", Vec::new());
        let parent = Node::element("div", vec![old_child.clone()]);
        crate::dom::reparent_subtree(&parent);

        let result = apply_dom_mutation(
            &registry,
            DomMutation::ReplaceChild {
                parent: &parent,
                new_child: &new_child,
                old_child: &old_child,
            },
        );

        let children = element_children(&parent);
        assert_eq!(result.kind, DomMutationKind::ReplaceChild);
        assert_eq!(children.len(), 1);
        assert!(Rc::ptr_eq(&children[0], &new_child));
        assert!(crate::dom::parent_ptr(&old_child).is_none());
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_applies_set_attribute_and_marks_dirty() {
        let registry = registry();
        let node = Node::element("div", Vec::new());

        let result = apply_dom_mutation(
            &registry,
            DomMutation::SetAttribute {
                node: &node,
                name: "data-state",
                value: "ready",
            },
        );

        let Node::Element(el) = &*node.borrow() else {
            panic!("expected element");
        };
        assert_eq!(result.kind, DomMutationKind::SetAttribute);
        assert!(result.changed);
        assert_eq!(
            el.attributes.get("data-state").map(String::as_str),
            Some("ready")
        );
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_applies_remove_attribute_and_marks_dirty() {
        let registry = registry();
        let node = Node::element_with_attributes(
            "div",
            std::collections::BTreeMap::from([("data-state".to_string(), "ready".to_string())]),
            Vec::new(),
        );

        let result = apply_dom_mutation(
            &registry,
            DomMutation::RemoveAttribute {
                node: &node,
                name: "data-state",
            },
        );

        let Node::Element(el) = &*node.borrow() else {
            panic!("expected element");
        };
        assert_eq!(result.kind, DomMutationKind::RemoveAttribute);
        assert!(result.changed);
        assert!(!el.attributes.contains_key("data-state"));
        assert!(registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_attribute_mutation_ignores_non_elements() {
        let registry = registry();
        let node = Node::text("hello");

        let result = apply_dom_mutation(
            &registry,
            DomMutation::SetAttribute {
                node: &node,
                name: "data-state",
                value: "ready",
            },
        );

        assert_eq!(result.kind, DomMutationKind::SetAttribute);
        assert!(!result.changed);
        assert!(!registry.has_dirty_bits());
    }

    #[test]
    fn dispatcher_schedules_rebuild_when_render_sync_fails() {
        let registry = registry();
        let identity = test_identity();
        let render_dom =
            crate::html::Parser::new("<html><body><div></div></body></html>").parse_document();
        let render_doc = crate::blitz_document::BlitzDocument::try_from_dom(
            &render_dom,
            None,
            &identity,
            800,
            600,
        )
        .expect("render document should build");
        registry.set_render_document(Some(Rc::new(RefCell::new(render_doc))));

        let node = Node::element("div", Vec::new());
        let result = apply_dom_mutation(
            &registry,
            DomMutation::SetAttribute {
                node: &node,
                name: "data-state",
                value: "ready",
            },
        );

        assert_eq!(result.kind, DomMutationKind::SetAttribute);
        assert!(!result.render_synced);
        assert_eq!(
            registry.take_snapshot_rebuild_reason(),
            Some(SnapshotRebuildReason::SyncOperationFailed)
        );
    }
}

/// Whether `node` is reachable from `document` by walking parent pointers.
///
/// Walks up via `find_parent` (which uses the O(depth) parent back-pointer with
/// a self-healing scan fallback) instead of scanning the whole document subtree
/// downward, which made `isConnected` an O(N)-per-call hot spot during boot.
pub(crate) fn is_connected_to(document: &NodePtr, node: &NodePtr) -> bool {
    let mut current = node.clone();
    // Bounded to guard against a cycle introduced by a stale parent pointer.
    for _ in 0..100_000 {
        if Rc::ptr_eq(&current, document) {
            return true;
        }
        match crate::js_v8::selectors::query::find_parent(document, &current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

pub(crate) fn contains_ptr(parent: &NodePtr, other: &NodePtr) -> bool {
    if Rc::ptr_eq(parent, other) {
        return true;
    }
    // Borrow and recurse by reference; cloning the children `Vec` at every level
    // turned descendant checks into an allocation-heavy hot path. Children are
    // distinct `RefCell`s, so holding `parent`'s borrow across the recursion is
    // safe for an (acyclic) DOM tree.
    let borrow = parent.borrow();
    let kids: &[NodePtr] = match &*borrow {
        Node::Element(el) => &el.children,
        Node::Document { children, .. } => children,
        _ => return false,
    };
    kids.iter().any(|child| contains_ptr(child, other))
}
