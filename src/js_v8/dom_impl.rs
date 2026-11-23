//! Behaviour behind the generated bindings: `Dom` implements one
//! `<Interface>Members` trait per IDL interface that declares members. The
//! generated callbacks have already brand-checked `this` and converted every
//! argument per WebIDL by the time these run.

use super::bindings::{
    CharacterDataMembers, DocumentMembers, ElementMembers, Error, Fallible, GetRootNodeOptions,
    HTMLHyperlinkElementUtilsMembers, InnerHTMLMembers, NodeMembers,
    NonDocumentTypeChildNodeMembers, ParentNodeMembers,
};
use super::node_create::{
    call_global_hook, child_nodes_of, create_js_node, inner_html_target, interface_for,
    is_document_fragment, node_name, node_type, parent_of, parsed_html_nodes,
    reclaim_composed_fragment, sibling_of, track_fragment_children, v8_str,
};
use super::registry::NodeRegistry;
use super::selectors::query;
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

    /// <https://dom.spec.whatwg.org/#dom-node-normalize>: in every
    /// descendant run of adjacent text nodes, the first takes the run's
    /// joined data and the rest are removed; empty text nodes are removed.
    /// Template contents and shadow trees are not descendants.
    fn normalize(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<()> {
        normalize_children(registry, this);
        Ok(())
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

    /// <https://dom.spec.whatwg.org/#dom-node-nodevalue>: the data of text
    /// and comment nodes, null for every other node.
    fn node_value(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<String>> {
        Ok(match &*this.borrow() {
            Node::Text(text) | Node::Comment(text) => Some(text.content.clone()),
            _ => None,
        })
    }

    /// Setting it replaces a text or comment node's data (null as `""`) and
    /// does nothing on other nodes.
    fn set_node_value(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        value: Option<String>,
    ) -> Fallible<()> {
        if matches!(&*this.borrow(), Node::Text(_) | Node::Comment(_)) {
            let text = value.unwrap_or_default();
            mutation::apply_dom_mutation(registry, mutation::DomMutation::SetTextContent { node: this, text });
            registry.mark_layout_dirty(this);
        }
        Ok(())
    }

    /// <https://dom.spec.whatwg.org/#dom-node-isequalnode>
    fn is_equal_node(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, other: Option<NodePtr>) -> Fallible<bool> {
        Ok(other.is_some_and(|other| nodes_equal(this, &other)))
    }

    /// <https://dom.spec.whatwg.org/#dom-node-comparedocumentposition>.
    /// Aurora has no Attr nodes, so the attribute steps never apply.
    fn compare_document_position(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        other: NodePtr,
    ) -> Fallible<u16> {
        const DISCONNECTED: u16 = 0x01;
        const PRECEDING: u16 = 0x02;
        const FOLLOWING: u16 = 0x04;
        const CONTAINS: u16 = 0x08;
        const CONTAINED_BY: u16 = 0x10;
        const IMPLEMENTATION_SPECIFIC: u16 = 0x20;
        if Rc::ptr_eq(this, &other) {
            return Ok(0);
        }
        let document = document(registry, this);
        let this_chain = ancestors_from_root(registry, &document, this);
        let other_chain = ancestors_from_root(registry, &document, &other);
        if !Rc::ptr_eq(&this_chain[0], &other_chain[0]) {
            // Different trees: disconnected, ordered consistently by address.
            let order = if Rc::as_ptr(&other) < Rc::as_ptr(this) { PRECEDING } else { FOLLOWING };
            return Ok(DISCONNECTED | IMPLEMENTATION_SPECIFIC | order);
        }
        let shared = this_chain
            .iter()
            .zip(&other_chain)
            .take_while(|(a, b)| Rc::ptr_eq(a, b))
            .count();
        if shared == other_chain.len() {
            return Ok(CONTAINS | PRECEDING); // `other` is an ancestor
        }
        if shared == this_chain.len() {
            return Ok(CONTAINED_BY | FOLLOWING); // `other` is a descendant
        }
        // Siblings under the deepest common ancestor decide tree order.
        let siblings = child_nodes_of(&this_chain[shared - 1]);
        let index = |node: &NodePtr| siblings.iter().position(|n| Rc::ptr_eq(n, node));
        Ok(match (index(&this_chain[shared]), index(&other_chain[shared])) {
            (Some(mine), Some(theirs)) if theirs < mine => PRECEDING,
            _ => FOLLOWING,
        })
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

fn text_data(node: &NodePtr) -> Option<String> {
    match &*node.borrow() {
        Node::Text(text) => Some(text.content.clone()),
        _ => None,
    }
}

fn normalize_children(registry: &Rc<NodeRegistry>, parent: &NodePtr) {
    let mut run: Option<(NodePtr, String)> = None;
    let mut changed = false;
    for child in child_nodes_of(parent) {
        let Some(data) = text_data(&child) else {
            if let Some((first, joined)) = run.take() {
                changed |= finish_text_run(registry, &first, joined);
            }
            normalize_children(registry, &child);
            continue;
        };
        match &mut run {
            Some((_, joined)) => {
                joined.push_str(&data);
                mutation::apply_dom_mutation(registry, mutation::DomMutation::RemoveChild { parent, child: &child });
                changed = true;
            }
            None if data.is_empty() => {
                mutation::apply_dom_mutation(registry, mutation::DomMutation::RemoveChild { parent, child: &child });
                changed = true;
            }
            None => run = Some((child, data)),
        }
    }
    if let Some((first, joined)) = run {
        changed |= finish_text_run(registry, &first, joined);
    }
    if changed {
        registry.mark_layout_dirty(parent);
    }
}

/// Gives a run's first text node the run's joined data, if that changed it.
fn finish_text_run(registry: &Rc<NodeRegistry>, first: &NodePtr, joined: String) -> bool {
    if text_data(first).as_deref() == Some(joined.as_str()) {
        return false;
    }
    mutation::apply_dom_mutation(registry, mutation::DomMutation::SetTextContent { node: first, text: joined });
    true
}

/// `node` and its ancestors, root first.
fn ancestors_from_root(registry: &NodeRegistry, document: &NodePtr, node: &NodePtr) -> Vec<NodePtr> {
    let mut chain = vec![node.clone()];
    while let Some(parent) = parent_of(registry, document, chain.last().unwrap()) {
        chain.push(parent);
    }
    chain.reverse();
    chain
}

/// DOM's "equals" (<https://dom.spec.whatwg.org/#concept-node-equals>).
/// Elements compare by tag name and attribute set (Aurora keeps no separate
/// namespace or prefix), character data by data, and every node by its
/// children in order. Template contents and shadow roots are not children.
fn nodes_equal(a: &NodePtr, b: &NodePtr) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    let same = match (&*a.borrow(), &*b.borrow()) {
        (Node::Element(x), Node::Element(y)) => x.tag_name == y.tag_name && x.attributes == y.attributes,
        (Node::Text(x), Node::Text(y)) | (Node::Comment(x), Node::Comment(y)) => x.content == y.content,
        (Node::Document { .. }, Node::Document { .. }) => true,
        _ => false,
    };
    if !same {
        return false;
    }
    let (left, right) = (child_nodes_of(a), child_nodes_of(b));
    left.len() == right.len() && left.iter().zip(&right).all(|(x, y)| nodes_equal(x, y))
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

/// The element's attribute `name`, matched exactly. Aurora keeps no
/// attribute namespaces or prefixes, and does not lowercase names yet.
fn attribute(this: &NodePtr, name: &str) -> Option<String> {
    match &*this.borrow() {
        Node::Element(el) => el.attributes.get(name).cloned(),
        _ => None,
    }
}

fn set_attribute(registry: &Rc<NodeRegistry>, this: &NodePtr, name: &str, value: &str) {
    mutation::apply_dom_mutation(registry, mutation::DomMutation::SetAttribute { node: this, name, value });
}

fn remove_attribute(registry: &Rc<NodeRegistry>, this: &NodePtr, name: &str) {
    mutation::apply_dom_mutation(registry, mutation::DomMutation::RemoveAttribute { node: this, name });
}

/// The `NS` forms ignore the namespace, as the hand-written ones did, until
/// Aurora stores attribute namespaces.
const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// Aurora keeps no element namespace: SVG elements are the tags
/// `interface_for` maps to SVGElement, every other element is HTML.
fn is_svg(this: &NodePtr) -> bool {
    interface_for(this) == "SVGElement"
}

fn tag_name_of(this: &NodePtr) -> String {
    match &*this.borrow() {
        Node::Element(el) => el.tag_name.clone(),
        _ => String::new(),
    }
}

impl ElementMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-element-namespaceuri>
    fn namespace_uri(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<Option<String>> {
        Ok(Some(if is_svg(this) { SVG_NAMESPACE } else { HTML_NAMESPACE }.to_string()))
    }

    /// Always null: Aurora keeps no prefixes.
    fn prefix(_: Scope, _: &Rc<NodeRegistry>, _: &NodePtr) -> Fallible<Option<String>> {
        Ok(None)
    }

    /// <https://dom.spec.whatwg.org/#dom-element-localname>: HTML names are
    /// lowercase; SVG names keep their case (`clipPath`).
    fn local_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        let name = tag_name_of(this);
        Ok(if is_svg(this) { name } else { name.to_ascii_lowercase() })
    }

    /// <https://dom.spec.whatwg.org/#dom-element-tagname>: uppercase for
    /// HTML elements (in an HTML document), as-is otherwise.
    fn tag_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        let name = tag_name_of(this);
        Ok(if is_svg(this) { name } else { name.to_ascii_uppercase() })
    }

    /// <https://dom.spec.whatwg.org/#dom-element-id>
    fn id(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(attribute(this, "id").unwrap_or_default())
    }

    /// Clearing removes the attribute, as the hand-written accessor did.
    fn set_id(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        if value.is_empty() {
            remove_attribute(registry, this, "id");
        } else {
            set_attribute(registry, this, "id", &value);
        }
        Ok(())
    }

    /// <https://dom.spec.whatwg.org/#dom-element-hasattributes>
    fn has_attributes(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<bool> {
        Ok(matches!(&*this.borrow(), Node::Element(el) if !el.attributes.is_empty()))
    }

    /// <https://dom.spec.whatwg.org/#dom-element-getattribute>
    fn get_attribute(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, name: String) -> Fallible<Option<String>> {
        Ok(attribute(this, &name))
    }

    fn get_attribute_ns(
        _: Scope,
        _: &Rc<NodeRegistry>,
        this: &NodePtr,
        _namespace: Option<String>,
        local_name: String,
    ) -> Fallible<Option<String>> {
        Ok(attribute(this, &local_name))
    }

    /// <https://dom.spec.whatwg.org/#dom-element-setattribute>
    fn set_attribute(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, name: String, value: String) -> Fallible<()> {
        set_attribute(registry, this, &name, &value);
        Ok(())
    }

    fn set_attribute_ns(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        _namespace: Option<String>,
        qualified_name: String,
        value: String,
    ) -> Fallible<()> {
        set_attribute(registry, this, &qualified_name, &value);
        Ok(())
    }

    /// <https://dom.spec.whatwg.org/#dom-element-removeattribute>
    fn remove_attribute(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, name: String) -> Fallible<()> {
        remove_attribute(registry, this, &name);
        Ok(())
    }

    fn remove_attribute_ns(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        _namespace: Option<String>,
        local_name: String,
    ) -> Fallible<()> {
        remove_attribute(registry, this, &local_name);
        Ok(())
    }

    /// <https://dom.spec.whatwg.org/#dom-element-toggleattribute>: adds the
    /// attribute (as "") unless `force` is false, removes it unless `force`
    /// is true, and returns whether it is present afterwards.
    fn toggle_attribute(
        _: Scope,
        registry: &Rc<NodeRegistry>,
        this: &NodePtr,
        name: String,
        force: Option<bool>,
    ) -> Fallible<bool> {
        let present = attribute(this, &name).is_some();
        Ok(match (present, force) {
            (false, None | Some(true)) => {
                set_attribute(registry, this, &name, "");
                true
            }
            (true, None | Some(false)) => {
                remove_attribute(registry, this, &name);
                false
            }
            (present, _) => present,
        })
    }

    /// <https://dom.spec.whatwg.org/#dom-element-hasattribute>
    fn has_attribute(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, name: String) -> Fallible<bool> {
        Ok(attribute(this, &name).is_some())
    }

    fn has_attribute_ns(
        _: Scope,
        _: &Rc<NodeRegistry>,
        this: &NodePtr,
        _namespace: Option<String>,
        local_name: String,
    ) -> Fallible<bool> {
        Ok(attribute(this, &local_name).is_some())
    }

    /// <https://dom.spec.whatwg.org/#dom-element-closest>
    fn closest(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, selectors: String) -> Fallible<Option<NodePtr>> {
        let parsed = parse_selectors(&selectors)?;
        let (root, document) = (selector_root(registry, this), document(registry, this));
        let mut current = Some(this.clone());
        while let Some(node) = current.filter(|n| node_type(n) == 1) {
            if query::matches_any(&node, &parsed, &root) {
                return Ok(Some(node));
            }
            current = parent_of(registry, &document, &node);
        }
        Ok(None)
    }

    /// <https://dom.spec.whatwg.org/#dom-element-matches>
    fn matches(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, selectors: String) -> Fallible<bool> {
        let parsed = parse_selectors(&selectors)?;
        Ok(query::matches_any(this, &parsed, &selector_root(registry, this)))
    }

    fn webkit_matches_selector(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, selectors: String) -> Fallible<bool> {
        <Dom as ElementMembers>::matches(scope, registry, this, selectors)
    }

    /// <https://dom.spec.whatwg.org/#dom-element-getelementsbytagname>
    fn get_elements_by_tag_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, name: String) -> Fallible<Vec<NodePtr>> {
        Ok(elements_by_tag_name(this, &name))
    }

    /// <https://dom.spec.whatwg.org/#dom-element-getelementsbyclassname>
    fn get_elements_by_class_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, names: String) -> Fallible<Vec<NodePtr>> {
        Ok(elements_by_class_name(this, &names))
    }

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

/// The root selectors match against: the node's shadow root when it is in
/// one (ShadyDOM-scoped CSS), otherwise the document.
fn selector_root(registry: &NodeRegistry, this: &NodePtr) -> NodePtr {
    use crate::dom::ShadowTreeBackend;
    crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(this)
        .unwrap_or_else(|| document(registry, this))
}

/// <https://dom.spec.whatwg.org/#scope-match-a-selectors-string>: an
/// invalid selector throws SyntaxError.
fn parse_selectors(selectors: &str) -> Fallible<query::Selectors> {
    query::parse(selectors).ok_or_else(|| Error::Dom {
        name: "SyntaxError",
        message: format!("'{selectors}' is not a valid selector."),
    })
}

/// The node's descendant elements in tree order (template contents and
/// shadow trees are not descendants).
fn descendant_elements(node: &NodePtr, out: &mut Vec<NodePtr>) {
    for child in child_nodes_of(node) {
        if node_type(&child) == 1 {
            out.push(child.clone());
        }
        descendant_elements(&child, out);
    }
}

/// <https://dom.spec.whatwg.org/#concept-getelementsbytagname>. HTML
/// elements match case-insensitively, SVG elements exactly.
fn elements_by_tag_name(root: &NodePtr, name: &str) -> Vec<NodePtr> {
    let mut all = Vec::new();
    descendant_elements(root, &mut all);
    if name == "*" {
        return all;
    }
    all.retain(|el| {
        let tag = tag_name_of(el);
        if is_svg(el) { tag == name } else { tag.eq_ignore_ascii_case(name) }
    });
    all
}

/// <https://dom.spec.whatwg.org/#concept-getelementsbyclassname>: elements
/// carrying every class in `names` (split on ASCII whitespace).
fn elements_by_class_name(root: &NodePtr, names: &str) -> Vec<NodePtr> {
    let ascii_ws = |c: char| matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ');
    let wanted: Vec<&str> = names.split(ascii_ws).filter(|s| !s.is_empty()).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut all = Vec::new();
    descendant_elements(root, &mut all);
    all.retain(|el| {
        let class = attribute(el, "class").unwrap_or_default();
        let have: Vec<&str> = class.split(ascii_ws).collect();
        wanted.iter().all(|name| have.contains(name))
    });
    all
}

impl DocumentMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-document-getelementsbytagname>,
    /// as a fresh array until HTMLCollection exists.
    fn get_elements_by_tag_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, name: String) -> Fallible<Vec<NodePtr>> {
        Ok(elements_by_tag_name(this, &name))
    }

    /// <https://dom.spec.whatwg.org/#dom-document-getelementsbyclassname>
    fn get_elements_by_class_name(_: Scope, _: &Rc<NodeRegistry>, this: &NodePtr, names: String) -> Fallible<Vec<NodePtr>> {
        Ok(elements_by_class_name(this, &names))
    }
}

impl ParentNodeMembers for Dom {
    /// <https://dom.spec.whatwg.org/#dom-parentnode-queryselector>
    fn query_selector(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, selectors: String) -> Fallible<Option<NodePtr>> {
        let parsed = parse_selectors(&selectors)?;
        Ok(query::first_match(&selector_root(registry, this), &parsed, this))
    }

    /// <https://dom.spec.whatwg.org/#dom-parentnode-queryselectorall>, as a
    /// fresh array until NodeList exists.
    fn query_selector_all(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, selectors: String) -> Fallible<Vec<NodePtr>> {
        let parsed = parse_selectors(&selectors)?;
        Ok(query::all_matches(&selector_root(registry, this), &parsed, this))
    }

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

// --- HTMLHyperlinkElementUtils ---------------------------------------------
// <https://html.spec.whatwg.org/multipage/links.html#htmlhyperlinkelementutils>,
// with the URL Standard's component getters and setters from `url::quirks`.

/// The document's URL, as JS sees it through `location.href`. `<base href>`
/// is not applied yet.
fn document_base_url(scope: Scope) -> Option<url::Url> {
    let global = scope.get_current_context().global(scope);
    let key = v8_str(scope, "location");
    let location = v8::Local::<v8::Object>::try_from(global.get(scope, key.into())?).ok()?;
    let key = v8_str(scope, "href");
    let href = location.get(scope, key.into())?.to_rust_string_lossy(scope);
    url::Url::parse(&href).ok()
}

/// The element's url: its `href` attribute parsed against the document's
/// URL, or `None` without the attribute or when parsing fails.
fn hyperlink_url(scope: Scope, this: &NodePtr) -> Option<url::Url> {
    let href = attribute(this, "href")?;
    match document_base_url(scope) {
        Some(base) => base.join(&href).ok(),
        None => url::Url::parse(&href).ok(),
    }
}

/// Applies `change` to the element's url and, if it succeeded, writes the
/// result back to `href` ("update href"). Nothing happens without a url.
fn change_hyperlink_url(
    scope: Scope,
    registry: &Rc<NodeRegistry>,
    this: &NodePtr,
    change: impl FnOnce(&mut url::Url) -> Result<(), ()>,
) {
    let Some(mut url) = hyperlink_url(scope, this) else { return };
    if change(&mut url).is_ok() {
        set_attribute(registry, this, "href", url.as_str());
    }
}

fn url_part(scope: Scope, this: &NodePtr, part: fn(&url::Url) -> &str) -> Fallible<String> {
    Ok(hyperlink_url(scope, this).map(|url| part(&url).to_string()).unwrap_or_default())
}

impl HTMLHyperlinkElementUtilsMembers for Dom {
    /// The serialized url, or the raw attribute when it does not parse.
    fn href(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(match hyperlink_url(scope, this) {
            Some(url) => url.into(),
            None => attribute(this, "href").unwrap_or_default(),
        })
    }

    fn set_href(_: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        set_attribute(registry, this, "href", &value);
        Ok(())
    }

    fn origin(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(hyperlink_url(scope, this).map(|url| url::quirks::origin(&url)).unwrap_or_default())
    }

    /// `":"` without a url, as the spec says.
    fn protocol(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        Ok(hyperlink_url(scope, this).map_or(":".into(), |url| url::quirks::protocol(&url).to_string()))
    }

    fn set_protocol(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_protocol(url, &value));
        Ok(())
    }

    fn username(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::username)
    }

    fn set_username(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_username(url, &value));
        Ok(())
    }

    fn password(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::password)
    }

    fn set_password(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_password(url, &value));
        Ok(())
    }

    fn host(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::host)
    }

    fn set_host(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_host(url, &value));
        Ok(())
    }

    fn hostname(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::hostname)
    }

    fn set_hostname(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_hostname(url, &value));
        Ok(())
    }

    fn port(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::port)
    }

    fn set_port(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| url::quirks::set_port(url, &value));
        Ok(())
    }

    fn pathname(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::pathname)
    }

    fn set_pathname(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| {
            url::quirks::set_pathname(url, &value);
            Ok(())
        });
        Ok(())
    }

    fn search(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::search)
    }

    fn set_search(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| {
            url::quirks::set_search(url, &value);
            Ok(())
        });
        Ok(())
    }

    fn hash(scope: Scope, _: &Rc<NodeRegistry>, this: &NodePtr) -> Fallible<String> {
        url_part(scope, this, url::quirks::hash)
    }

    fn set_hash(scope: Scope, registry: &Rc<NodeRegistry>, this: &NodePtr, value: String) -> Fallible<()> {
        change_hyperlink_url(scope, registry, this, |url| {
            url::quirks::set_hash(url, &value);
            Ok(())
        });
        Ok(())
    }
}
