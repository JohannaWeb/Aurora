//! Behaviour behind the generated bindings: `Dom` implements one
//! `<Interface>Members` trait per IDL interface that declares members. The
//! generated callbacks have already brand-checked `this` and converted every
//! argument per WebIDL by the time these run.

use super::bindings::{Fallible, NodeMembers};
use super::registry::NodeRegistry;
use super::tree::mutation;
use crate::dom::NodePtr;
use std::rc::Rc;

pub(super) struct Dom;

impl NodeMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-node-contains>
    fn contains(
        _: &mut v8::PinScope<'_, '_>,
        _: &Rc<NodeRegistry>,
        this: &NodePtr,
        other: Option<NodePtr>,
    ) -> Fallible<bool> {
        Ok(other.is_some_and(|other| mutation::contains_ptr(this, &other)))
    }
}
