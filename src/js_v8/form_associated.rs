//! Form association for form-associated custom elements.
//!
//! A custom element whose definition sets `static formAssociated = true` takes
//! part in forms the way a built-in control does: it acquires a *form owner*,
//! it inherits the disabled state of an ancestor `<fieldset>`, and it is told
//! about both through the form lifecycle callbacks.
//!
//! Aurora has no built-in form controls, so this module implements only the
//! parts the custom-element lifecycle needs: resolving an element's form owner
//! and disabled state, and enqueueing `formAssociatedCallback`,
//! `formDisabledCallback` and `formResetCallback` when either changes. The
//! callbacks are enqueued as custom-element reactions, so they drain at the
//! same `[CEReactions]` boundary as `connectedCallback`.
//!
//! `formStateRestoreCallback` has no call site: it fires when a session's form
//! state is restored, and Aurora has no session history to restore from. The
//! definition still captures it, so it lands the moment that exists.

use std::rc::Rc;

use crate::dom::{Node, NodePtr, parent_ptr};

use super::registry::NodeRegistry;
use super::tree::mutation::is_connected_to;

/// Bounds every ancestor walk, so a stale parent pointer that formed a cycle
/// cannot hang the mutation path. Mirrors the bound in `is_connected_to`.
const MAX_ANCESTOR_DEPTH: usize = 100_000;

/// Which form lifecycle callback a reaction will invoke, and with what.
///
/// The payload is scope-free — node ids and plain Rust values rather than V8
/// handles — because it is built on the mutation path, which has no V8 scope.
/// The arguments are constructed at drain time, exactly as
/// `Reaction::AttributeChanged` does with its strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FormReaction {
    /// `formAssociatedCallback(form)` — the new form owner, or `null`.
    Associated { form: Option<u32> },
    /// `formDisabledCallback(disabled)`.
    Disabled { disabled: bool },
    /// `formResetCallback()`.
    Reset,
}

fn tag_of(node: &NodePtr) -> Option<String> {
    match &*node.borrow() {
        Node::Element(el) => Some(el.tag_name.clone()),
        _ => None,
    }
}

fn attribute(node: &NodePtr, name: &str) -> Option<String> {
    match &*node.borrow() {
        Node::Element(el) => el.attributes.get(name).cloned(),
        _ => None,
    }
}

fn is_tag(node: &NodePtr, tag: &str) -> bool {
    tag_of(node).as_deref() == Some(tag)
}

fn element_children(node: &NodePtr) -> Vec<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    }
}

/// The first `<form>` in tree order whose `id` is `id`.
fn find_form_by_id(document: &NodePtr, id: &str) -> Option<NodePtr> {
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        if is_tag(&node, "form") && attribute(&node, "id").as_deref() == Some(id) {
            return Some(node);
        }
        // Reversed so the stack pops children in tree order, which is what
        // makes "the first matching form" mean the first one in the document.
        for child in element_children(&node).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

/// The element's form owner.
///
/// A `form` content attribute wins over ancestry and resolves by id against the
/// whole document — that is what lets a form-associated element live outside
/// its form's subtree. When the attribute is present but names nothing, the
/// element has *no* owner; it does not fall back to an ancestor form.
pub(crate) fn form_owner_of(document: &NodePtr, node: &NodePtr) -> Option<NodePtr> {
    // A disconnected element has no form owner: its `form` attribute cannot
    // resolve against a document it is not in.
    if !is_connected_to(document, node) {
        return None;
    }
    if let Some(id) = attribute(node, "form") {
        if id.is_empty() {
            return None;
        }
        return find_form_by_id(document, &id);
    }
    let mut current = parent_ptr(node);
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let ancestor = current?;
        if is_tag(&ancestor, "form") {
            return Some(ancestor);
        }
        current = parent_ptr(&ancestor);
    }
    None
}

/// Whether `child` is the first `<legend>` element child of `fieldset`. The
/// controls inside that legend escape the fieldset's disabled state, since it
/// is what holds the fieldset's own label and controls.
fn is_first_legend_child(fieldset: &NodePtr, child: &NodePtr) -> bool {
    element_children(fieldset)
        .iter()
        .find(|candidate| is_tag(candidate, "legend"))
        .is_some_and(|first| Rc::ptr_eq(first, child))
}

/// Whether the element is disabled: either it carries `disabled` itself, or it
/// descends from a disabled `<fieldset>` other than through that fieldset's
/// first `<legend>`.
pub(crate) fn is_actually_disabled(node: &NodePtr) -> bool {
    if attribute(node, "disabled").is_some() {
        return true;
    }
    let mut child = node.clone();
    let mut current = parent_ptr(node);
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let Some(ancestor) = current else {
            return false;
        };
        if is_tag(&ancestor, "fieldset")
            && attribute(&ancestor, "disabled").is_some()
            && !is_first_legend_child(&ancestor, &child)
        {
            return true;
        }
        current = parent_ptr(&ancestor);
        child = ancestor;
    }
    false
}

/// Recompute `node`'s form owner and disabled state, enqueueing
/// `formAssociatedCallback` / `formDisabledCallback` for whichever changed.
///
/// A no-op unless `tag` names a definition that opted into form association, so
/// the ordinary insertion path pays only a registry lookup per element.
pub(crate) fn sync_form_association(
    registry: &Rc<NodeRegistry>,
    document: &NodePtr,
    node: &NodePtr,
    tag: &str,
) {
    let Some(definition) = registry.ce_registry.lookup(tag) else {
        return;
    };
    if !definition.form_associated {
        return;
    }
    let node_id = registry.register(node.clone());

    let owner = form_owner_of(document, node).map(|form| registry.register(form));
    if owner != registry.ce_registry.form_owner(node_id) {
        registry.ce_registry.set_form_owner(node_id, owner);
        if let Some(callback) = &definition.form_associated_callback {
            registry.ce_registry.enqueue_form(
                node_id,
                callback.clone(),
                FormReaction::Associated { form: owner },
            );
        }
    }

    let disabled = is_actually_disabled(node);
    if disabled != registry.ce_registry.form_disabled(node_id) {
        registry.ce_registry.set_form_disabled(node_id, disabled);
        if let Some(callback) = &definition.form_disabled_callback {
            registry.ce_registry.enqueue_form(
                node_id,
                callback.clone(),
                FormReaction::Disabled { disabled },
            );
        }
    }
}

/// Drop `node`'s form association because it is leaving the document, telling
/// it so with `formAssociatedCallback(null)`.
///
/// Called *before* the subtree is detached, like the disconnected walk, so the
/// element's stored owner is still the one it is losing.
pub(crate) fn clear_form_association(registry: &Rc<NodeRegistry>, node: &NodePtr, tag: &str) {
    let Some(definition) = registry.ce_registry.lookup(tag) else {
        return;
    };
    if !definition.form_associated {
        return;
    }
    let node_id = registry.register(node.clone());
    if registry.ce_registry.form_owner(node_id).is_none() {
        return;
    }
    registry.ce_registry.set_form_owner(node_id, None);
    if let Some(callback) = &definition.form_associated_callback {
        registry.ce_registry.enqueue_form(
            node_id,
            callback.clone(),
            FormReaction::Associated { form: None },
        );
    }
}

/// Every element in `root`'s subtree, shadow trees included, in tree order.
fn subtree(root: &NodePtr) -> Vec<NodePtr> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        let (children, shadow) = match &*node.borrow() {
            Node::Element(el) => (el.children.clone(), el.shadow_root.clone()),
            Node::Document { children, .. } => (children.clone(), None),
            _ => (Vec::new(), None),
        };
        if matches!(&*node.borrow(), Node::Element(_)) {
            out.push(node.clone());
        }
        if let Some(shadow) = shadow {
            stack.push(shadow);
        }
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    out
}

/// Resync form association across `root`'s subtree after a `form` or `disabled`
/// attribute changed on it.
///
/// The subtree walk is not overkill: `disabled` on a `<fieldset>` changes the
/// disabled state of every form-associated descendant, and a `form` attribute
/// appearing on a container does not, so the cheapest correct rule is to
/// recompute the subtree and let the per-element change checks decide who
/// actually gets a callback.
pub(crate) fn sync_after_attribute_change(registry: &Rc<NodeRegistry>, node: &NodePtr, name: &str) {
    if !matches!(name, "form" | "disabled") {
        return;
    }
    if !registry.native_ce_reactions.get() {
        return;
    }
    let Some(document) = registry.document.borrow().clone() else {
        return;
    };
    for element in subtree(node) {
        if let Some(tag) = tag_of(&element) {
            sync_form_association(registry, &document, &element, &tag);
        }
    }
}

/// Enqueue `formResetCallback` for every form-associated custom element owned
/// by `form`. Backs `form.reset()`.
pub(crate) fn enqueue_reset_reactions(registry: &Rc<NodeRegistry>, form: &NodePtr) {
    let Some(document) = registry.document.borrow().clone() else {
        return;
    };
    for element in subtree(&document) {
        let Some(tag) = tag_of(&element) else {
            continue;
        };
        let Some(definition) = registry.ce_registry.lookup(&tag) else {
            continue;
        };
        if !definition.form_associated {
            continue;
        }
        if !form_owner_of(&document, &element).is_some_and(|owner| Rc::ptr_eq(&owner, form)) {
            continue;
        }
        let node_id = registry.register(element.clone());
        if let Some(callback) = &definition.form_reset_callback {
            registry
                .ce_registry
                .enqueue_form(node_id, callback.clone(), FormReaction::Reset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Node;
    use std::collections::BTreeMap;

    fn element(tag: &str, attrs: &[(&str, &str)]) -> NodePtr {
        let attributes: BTreeMap<String, String> = attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Node::element_with_attributes(tag, attributes, Vec::new())
    }

    /// Append `child` to `parent`, keeping the parent back-pointer the
    /// ancestor walks rely on.
    fn append(parent: &NodePtr, child: &NodePtr) {
        match &mut *parent.borrow_mut() {
            Node::Element(el) => el.children.push(child.clone()),
            Node::Document { children, .. } => children.push(child.clone()),
            _ => panic!("only elements and documents take children"),
        }
        crate::dom::set_parent(child, parent);
    }

    /// A document-rooted tree, with `body` returned so tests can hang content
    /// off a connected node.
    fn document_with_body() -> (NodePtr, NodePtr) {
        let document = Node::document(Vec::new());
        let body = element("body", &[]);
        append(&document, &body);
        (document, body)
    }

    #[test]
    fn form_owner_is_the_nearest_ancestor_form() {
        let (document, body) = document_with_body();
        let outer = element("form", &[]);
        let inner = element("form", &[]);
        let control = element("my-input", &[]);
        append(&body, &outer);
        append(&outer, &inner);
        append(&inner, &control);

        let owner = form_owner_of(&document, &control).expect("an ancestor form owns the control");
        assert!(Rc::ptr_eq(&owner, &inner), "the nearest form wins");
    }

    #[test]
    fn form_attribute_associates_across_the_document_and_beats_ancestry() {
        let (document, body) = document_with_body();
        let ancestor_form = element("form", &[]);
        let target_form = element("form", &[("id", "target")]);
        let control = element("my-input", &[("form", "target")]);
        append(&body, &target_form);
        append(&body, &ancestor_form);
        append(&ancestor_form, &control);

        let owner = form_owner_of(&document, &control).expect("the form attribute resolves");
        assert!(
            Rc::ptr_eq(&owner, &target_form),
            "the form attribute wins over the ancestor form"
        );
    }

    #[test]
    fn a_form_attribute_naming_nothing_leaves_the_element_unowned() {
        let (document, body) = document_with_body();
        let ancestor_form = element("form", &[]);
        let control = element("my-input", &[("form", "missing")]);
        append(&body, &ancestor_form);
        append(&ancestor_form, &control);

        assert!(
            form_owner_of(&document, &control).is_none(),
            "a form attribute that resolves to nothing must not fall back to the ancestor"
        );
    }

    #[test]
    fn a_disconnected_element_has_no_form_owner() {
        let (document, _body) = document_with_body();
        let form = element("form", &[]);
        let control = element("my-input", &[]);
        append(&form, &control);

        assert!(form_owner_of(&document, &control).is_none());
    }

    #[test]
    fn a_disabled_fieldset_disables_its_descendants() {
        let (_document, body) = document_with_body();
        let fieldset = element("fieldset", &[("disabled", "")]);
        let control = element("my-input", &[]);
        append(&body, &fieldset);
        append(&fieldset, &control);

        assert!(is_actually_disabled(&control));
    }

    #[test]
    fn the_first_legend_escapes_a_disabled_fieldset() {
        let (_document, body) = document_with_body();
        let fieldset = element("fieldset", &[("disabled", "")]);
        let first_legend = element("legend", &[]);
        let second_legend = element("legend", &[]);
        let escaping = element("my-input", &[]);
        let trapped = element("my-input", &[]);
        append(&body, &fieldset);
        append(&fieldset, &first_legend);
        append(&fieldset, &second_legend);
        append(&first_legend, &escaping);
        append(&second_legend, &trapped);

        assert!(
            !is_actually_disabled(&escaping),
            "controls in the first legend stay enabled"
        );
        assert!(
            is_actually_disabled(&trapped),
            "only the first legend escapes"
        );
    }

    #[test]
    fn an_own_disabled_attribute_disables_regardless_of_ancestry() {
        let (_document, body) = document_with_body();
        let control = element("my-input", &[("disabled", "")]);
        append(&body, &control);

        assert!(is_actually_disabled(&control));
    }
}
