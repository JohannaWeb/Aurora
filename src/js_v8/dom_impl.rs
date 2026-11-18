//! Behaviour behind the generated bindings: `Dom` implements one
//! `<Interface>Members` trait per IDL interface that declares members. The
//! generated callbacks have already brand-checked `this` and converted every
//! argument per WebIDL by the time these run.

use super::bindings::{CharacterDataMembers, Error, Fallible, NodeMembers};
use super::registry::NodeRegistry;
use super::tree::mutation;
use crate::dom::{Node, NodePtr};
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

type Scope<'a, 's, 'p> = &'a mut v8::PinScope<'s, 'p>;

/// The node's data as UTF-16 code units, the unit CharacterData offsets count.
fn code_units(node: &NodePtr) -> Vec<u16> {
    match &*node.borrow() {
        Node::Text(text) | Node::Comment(text) => text.content.encode_utf16().collect(),
        _ => Vec::new(),
    }
}

/// The end of the `count` units from `offset`, clamped to the data's length.
fn range_end(units: &[u16], offset: u32, count: u32) -> Fallible<usize> {
    let (offset, length) = (offset as usize, units.len());
    if offset > length {
        return Err(Error::Dom {
            name: "IndexSizeError",
            message: format!("The offset {offset} is greater than the node's length ({length})."),
        });
    }
    Ok(offset + (count as usize).min(length - offset))
}

/// <https://dom.spec.whatwg.org/#concept-cd-replace>. Strings cross into Rust
/// as UTF-8, so a split surrogate pair is stored as U+FFFD.
fn replace_data(
    registry: &Rc<NodeRegistry>,
    node: &NodePtr,
    offset: u32,
    count: u32,
    data: &str,
) -> Fallible<()> {
    let mut units = code_units(node);
    let end = range_end(&units, offset, count)?;
    units.splice(offset as usize..end, data.encode_utf16());
    mutation::apply_dom_mutation(
        registry,
        mutation::DomMutation::SetTextContent { node, text: String::from_utf16_lossy(&units) },
    );
    registry.mark_layout_dirty(node);
    Ok(())
}

impl CharacterDataMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-characterdata-data>
    fn data(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(String::from_utf16_lossy(&code_units(this)))
    }

    fn set_data(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        replace_data(registry, this, 0, u32::MAX, &value)
    }

    /// <https://dom.spec.whatwg.org/#dom-characterdata-length>
    fn length(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<u32> {
        Ok(code_units(this).len() as u32)
    }

    /// <https://dom.spec.whatwg.org/#concept-cd-substring>
    fn substring_data(
        _: Scope,
        _: &Rc<NodeRegistry>,
        this: &NodePtr,
        offset: u32,
        count: u32,
    ) -> Fallible<String> {
        let units = code_units(this);
        let end = range_end(&units, offset, count)?;
        Ok(String::from_utf16_lossy(&units[offset as usize..end]))
    }

    /// <https://dom.spec.whatwg.org/#dom-characterdata-appenddata>
    fn append_data(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, data: String) -> Fallible<()> {
        let length = code_units(this).len() as u32;
        replace_data(registry, this, length, 0, &data)
    }

    /// <https://dom.spec.whatwg.org/#dom-characterdata-insertdata>
    fn insert_data(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        offset: u32,
        data: String,
    ) -> Fallible<()> {
        replace_data(registry, this, offset, 0, &data)
    }

    /// <https://dom.spec.whatwg.org/#dom-characterdata-deletedata>
    fn delete_data(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        offset: u32,
        count: u32,
    ) -> Fallible<()> {
        replace_data(registry, this, offset, count, "")
    }

    /// <https://dom.spec.whatwg.org/#dom-characterdata-replacedata>
    fn replace_data(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        offset: u32,
        count: u32,
        data: String,
    ) -> Fallible<()> {
        replace_data(registry, this, offset, count, &data)
    }
}
