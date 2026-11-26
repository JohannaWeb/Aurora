//! Image loads that page script can observe: `load`/`error` events,
//! `complete` and the natural size.
//!
//! The renderer loads and paints `<img>` on its own. This tracks the images
//! whose `src` script set (or whose state it asked for), through the
//! renderer's resource cache, so the bytes are fetched once.
//! <https://html.spec.whatwg.org/multipage/images.html#update-the-image-data>,
//! without `srcset`, and only for `src` values set while script runs.

use super::registry::NodeRegistry;
use crate::blitz_document::ImageLoad;
use crate::dom::{Node, NodePtr};
use std::rc::Rc;

pub(super) struct ImageState {
    node: NodePtr,
    /// The load in flight, if any.
    pending: Option<ImageLoad>,
    natural_size: Option<(u32, u32)>,
}

fn image_src(node: &NodePtr) -> Option<String> {
    match &*node.borrow() {
        Node::Element(el) if el.tag_name.eq_ignore_ascii_case("img") => {
            el.attributes.get("src").filter(|src| !src.is_empty()).cloned()
        }
        _ => None,
    }
}

fn is_image(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name.eq_ignore_ascii_case("img"))
}

/// Call after `src` was set or removed: drops any load in flight and starts
/// one for the new value.
pub(super) fn src_changed(registry: &Rc<NodeRegistry>, node: &NodePtr) {
    if !is_image(node) {
        return;
    }
    let pending = image_src(node).and_then(|src| registry.load_image(&src));
    let id = registry.register(node.clone());
    registry.images.borrow_mut().insert(id, ImageState { node: node.clone(), pending, natural_size: None });
}

/// Starts tracking an image script asks about before it was tracked.
fn ensure_tracked(registry: &Rc<NodeRegistry>, node: &NodePtr) -> u32 {
    let id = registry.register(node.clone());
    if !registry.images.borrow().contains_key(&id) {
        src_changed(registry, node);
    }
    id
}

pub(super) fn complete(registry: &Rc<NodeRegistry>, node: &NodePtr) -> bool {
    let id = ensure_tracked(registry, node);
    registry.images.borrow().get(&id).is_none_or(|image| image.pending.is_none())
}

pub(super) fn natural_size(registry: &Rc<NodeRegistry>, node: &NodePtr) -> (u32, u32) {
    let id = ensure_tracked(registry, node);
    registry.images.borrow().get(&id).and_then(|image| image.natural_size).unwrap_or_default()
}

pub(super) fn has_pending(registry: &NodeRegistry) -> bool {
    registry.images.borrow().values().any(|image| image.pending.is_some())
}

pub(super) fn has_finished(registry: &NodeRegistry) -> bool {
    registry.images.borrow().values().any(|image| image.pending.as_ref().is_some_and(ImageLoad::is_finished))
}

/// Takes the finished loads, as (element, whether it loaded), recording their
/// outcome; the caller fires `load` or `error` at each.
pub(super) fn take_finished(registry: &NodeRegistry) -> Vec<(NodePtr, bool)> {
    let mut finished = Vec::new();
    for image in registry.images.borrow_mut().values_mut() {
        let Some(outcome) = image.pending.as_ref().and_then(ImageLoad::outcome) else { continue };
        image.pending = None;
        image.natural_size = outcome;
        finished.push((image.node.clone(), outcome.is_some()));
    }
    finished
}
