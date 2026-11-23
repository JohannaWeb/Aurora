//! Behaviour behind the generated bindings: `Dom` implements one
//! `<Interface>Members` trait per IDL interface that declares members. The
//! generated callbacks have already brand-checked `this` and converted every
//! argument per WebIDL by the time these run.

use super::bindings::{
    CharacterDataMembers, ElementMembers, Error, Fallible, GetRootNodeOptions, InnerHTMLMembers,
    NodeMembers, NonDocumentTypeChildNodeMembers, ParentNodeMembers,
};
use super::node_create::{
    call_global_hook, child_nodes_of, create_js_node, inner_html_target, is_document_fragment,
    node_name, node_type, parent_of, parsed_html_nodes, reclaim_composed_fragment, sibling_of,
    track_fragment_children,
};
use super::registry::NodeRegistry;
use super::tree::{mutation, navigation};
use crate::dom::{Node, NodePtr};
use std::rc::Rc;

pub(super) struct Dom;

type Scope<'a, 's, 'p> = &'a mut v8::PinScope<'s, 'p>;


/// The document the registry serves, which every wrapped node belongs to
/// until Aurora has more than one Document.
fn document(registry: &NodeRegistry, this: &NodePtr) -> NodePtr {
    registry.document.borrow().clone().unwrap_or_else(|| this.clone())
}

impl NodeMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-node-nodetype>
    fn node_type(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<u16> {
        Ok(node_type(this) as u16)
    }

    /// <https://dom.spec.whatwg.org/#dom-node-nodename>
    fn node_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(node_name(this))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-isconnected>
    fn is_connected(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<bool> {
        Ok(mutation::is_connected_to(&document(registry, this), this))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-ownerdocument>: null for a document.
    fn owner_document(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        let document = document(registry, this);
        Ok((!Rc::ptr_eq(&document, this) && !matches!(&*this.borrow(), Node::Document { .. }))
            .then_some(document))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-parentnode>
    fn parent_node(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(parent_of(registry, &document(registry, this), this))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-parentelement>
    fn parent_element(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(parent_of(registry, &document(registry, this), this).filter(|p| node_type(p) == 1))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-childnodes>, as a fresh array
    /// until NodeList exists.
    fn child_nodes(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Vec<NodePtr>> {
        Ok(child_nodes_of(this))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-textcontent>
    fn text_content(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<String>> {
        Ok(Some(mutation::collect_text(this)))
    }

    fn set_text_content(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        value: Option<String>,
    ) -> Fallible<()> {
        let text = value.unwrap_or_default();
        mutation::apply_dom_mutation(registry, mutation::DomMutation::SetTextContent { node: this, text });
        registry.mark_layout_dirty(this);
        Ok(())
    }

    /// <https://dom.spec.whatwg.org/#dom-node-firstchild>
    fn first_child(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(navigation::first_child(this, false))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-lastchild>
    fn last_child(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(navigation::last_child(this, false))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-previoussibling>
    fn previous_sibling(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(sibling_of(registry, &document(registry, this), this, -1, false))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-nextsibling>
    fn next_sibling(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(sibling_of(registry, &document(registry, this), this, 1, false))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-contains>
    fn contains(
        _: &mut v8::PinScope<'_, '_>,
        _: &Rc<NodeRegistry>,
        this: &NodePtr,
        other: Option<NodePtr>,
    ) -> Fallible<bool> {
        Ok(other.is_some_and(|other| mutation::contains_ptr(this, &other)))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-clonenode>
    fn clone_node(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, subtree: bool) -> Fallible<NodePtr> {
        let clone = mutation::clone_node(this, subtree);
        if is_document_fragment(&clone) {
            let wrapper = create_js_node(scope, clone.clone(), registry, &document(registry, this));
            call_global_hook(scope, "__aurora_track_fragment__", wrapper);
        }
        Ok(clone)
    }

    /// <https://dom.spec.whatwg.org/#dom-node-haschildnodes>
    fn has_child_nodes(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<bool> {
        Ok(!child_nodes_of(this).is_empty())
    }

    /// <https://dom.spec.whatwg.org/#dom-node-getrootnode>: the root of the
    /// node's tree, or with `composed`, of its shadow-including tree (a
    /// shadow root's host continues the walk).
    fn get_root_node(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        options: GetRootNodeOptions,
    ) -> Fallible<NodePtr> {
        use crate::dom::ShadowTreeBackend;
        let document = document(registry, this);
        let mut root = this.clone();
        loop {
            while let Some(parent) = parent_of(registry, &document, &root) {
                root = parent;
            }
            match crate::dom::SyntheticShadowTreeBackend.host_for_shadow_root(&root) {
                Some(host) if options.composed => root = host,
                _ => return Ok(root),
            }
        }
    }

    /// <https://dom.spec.whatwg.org/#dom-node-issamenode>
    fn is_same_node(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, other: Option<NodePtr>) -> Fallible<bool> {
        Ok(other.is_some_and(|other| Rc::ptr_eq(this, &other)))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-insertbefore>
    fn insert_before(
        scope: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        node: NodePtr,
        child: Option<NodePtr>,
    ) -> Fallible<NodePtr> {
        insert(scope, registry, this, &node, |parent, node| mutation::DomMutation::InsertBefore {
            parent,
            new_child: node,
            ref_child: child.as_ref(),
        })?;
        Ok(node)
    }

    /// <https://dom.spec.whatwg.org/#dom-node-appendchild>
    fn append_child(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, node: NodePtr) -> Fallible<NodePtr> {
        insert(scope, registry, this, &node, |parent, child| mutation::DomMutation::AppendChild {
            parent,
            child,
        })?;
        Ok(node)
    }

    /// <https://dom.spec.whatwg.org/#dom-node-replacechild>
    fn replace_child(
        scope: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        node: NodePtr,
        child: NodePtr,
    ) -> Fallible<NodePtr> {
        insert(scope, registry, this, &node, |parent, node| mutation::DomMutation::ReplaceChild {
            parent,
            new_child: node,
            old_child: &child,
        })?;
        Ok(child)
    }

    /// <https://dom.spec.whatwg.org/#dom-node-removechild>
    fn remove_child(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, child: NodePtr) -> Fallible<NodePtr> {
        mutation::apply_dom_mutation(registry, mutation::DomMutation::RemoveChild { parent: this, child: &child });
        Ok(child)
    }
}

/// The shared steps of insertBefore, appendChild and replaceChild: the
/// hierarchy checks of "ensure pre-insertion validity" (the child checks are
/// not enforced yet), the mutation, then custom-element tracking of the
/// inserted node or, for a fragment, of the children it carried.
fn insert<'n>(
    scope: Scope,
    registry: &Rc<NodeRegistry>,
    parent: &'n NodePtr,
    node: &'n NodePtr,
    mutation: impl FnOnce(&'n NodePtr, &'n NodePtr) -> mutation::DomMutation<'n>,
) -> Fallible<()> {
    if let Some(message) = mutation::hierarchy_error(parent, node) {
        return Err(Error::Dom { name: "HierarchyRequestError", message: message.into() });
    }
    let document = document(registry, parent);
    let wrapper = create_js_node(scope, node.clone(), registry, &document);
    reclaim_composed_fragment(scope, wrapper.into(), node);
    let fragment_children = if is_document_fragment(node) { child_nodes_of(node) } else { Vec::new() };
    mutation::apply_dom_mutation(registry, mutation(parent, node));
    if fragment_children.is_empty() {
        call_global_hook(scope, "__aurora_track_custom_element__", wrapper);
    } else {
        track_fragment_children(scope, registry, &document, &fragment_children);
    }
    registry.mark_layout_dirty(parent);
    Ok(())
}

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

fn element_children(node: &NodePtr) -> impl DoubleEndedIterator<Item = NodePtr> {
    child_nodes_of(node).into_iter().filter(|child| node_type(child) == 1)
}

impl ElementMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-element-classname>
    fn class_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(match &*this.borrow() {
            Node::Element(el) => el.attributes.get("class").cloned().unwrap_or_default(),
            _ => String::new(),
        })
    }

    /// Clearing removes the attribute, as the hand-written accessor did.
    fn set_class_name(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        let mutation = if value.is_empty() {
            mutation::DomMutation::RemoveAttribute { node: this, name: "class" }
        } else {
            mutation::DomMutation::SetAttribute { node: this, name: "class", value: &value }
        };
        mutation::apply_dom_mutation(registry, mutation);
        Ok(())
    }
}

impl ParentNodeMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-parentnode-children>, as a fresh
    /// array until HTMLCollection exists.
    fn children(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Vec<NodePtr>> {
        Ok(element_children(this).collect())
    }

    /// <https://dom.spec.whatwg.org/#dom-parentnode-firstelementchild>
    fn first_element_child(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(element_children(this).next())
    }

    /// <https://dom.spec.whatwg.org/#dom-parentnode-lastelementchild>
    fn last_element_child(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(element_children(this).next_back())
    }

    /// <https://dom.spec.whatwg.org/#dom-parentnode-childelementcount>
    fn child_element_count(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<u32> {
        Ok(element_children(this).count() as u32)
    }
}

impl NonDocumentTypeChildNodeMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-nondocumenttypechildnode-previouselementsibling>
    fn previous_element_sibling(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(sibling_of(registry, &document(registry, this), this, -1, true))
    }

    /// <https://dom.spec.whatwg.org/#dom-nondocumenttypechildnode-nextelementsibling>
    fn next_element_sibling(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<NodePtr>> {
        Ok(sibling_of(registry, &document(registry, this), this, 1, true))
    }
}

impl InnerHTMLMembers for Dom {
    /// <https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#dom-element-innerhtml>;
    /// a template reads and writes its content fragment.
    fn inner_html(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        let target = inner_html_target(this);
        Ok(child_nodes_of(&target).iter().map(crate::dom::serialize_outer_html).collect())
    }

    fn set_inner_html(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        let target = inner_html_target(this);
        let children = parsed_html_nodes(&value);
        mutation::apply_dom_mutation(registry, mutation::DomMutation::ReplaceChildren { node: &target, children });
        registry.mark_layout_dirty(this);
        Ok(())
    }
}
