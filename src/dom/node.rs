use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

/// Shared mutable DOM node pointer.
///
/// RUST FUNDAMENTAL: `Rc<T>` provides shared ownership and `RefCell<T>`
/// provides runtime-checked interior mutability.
pub type NodePtr = Rc<RefCell<Node>>;

/// Back-pointer from a node to its parent.
///
/// Stored as a `Weak` so the parent→child `Rc` ownership doesn't form a cycle.
/// Wrapped in a newtype with trivial equality so `ElementNode` can keep deriving
/// `PartialEq`/`Eq` (structural node equality intentionally ignores the parent
/// back-reference, and `Weak` doesn't implement `PartialEq` anyway).
#[derive(Debug, Clone, Default)]
pub struct ParentLink(pub Weak<RefCell<Node>>);

impl PartialEq for ParentLink {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ParentLink {}

/// Set `child`'s parent back-pointer to `parent` (no-op for non-element/text children).
pub fn set_parent(child: &NodePtr, parent: &NodePtr) {
    match &mut *child.borrow_mut() {
        Node::Element(el) => el.parent = ParentLink(Rc::downgrade(parent)),
        Node::Text(text) => text.parent = ParentLink(Rc::downgrade(parent)),
        _ => {}
    }
}

/// Clear `child`'s parent back-pointer (e.g. when detaching it from the tree).
pub fn clear_parent(child: &NodePtr) {
    match &mut *child.borrow_mut() {
        Node::Element(el) => el.parent = ParentLink(Weak::new()),
        Node::Text(text) => text.parent = ParentLink(Weak::new()),
        _ => {}
    }
}

/// Read `node`'s stored parent, if it is an element/text with a live parent pointer.
pub fn parent_ptr(node: &NodePtr) -> Option<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.parent.0.upgrade(),
        Node::Text(text) => text.parent.0.upgrade(),
        _ => None,
    }
}

/// Point every (element) child of `node` at `node`.
pub fn link_children(node: &NodePtr) {
    let (children, template_contents): (Vec<NodePtr>, Option<NodePtr>) = match &*node.borrow() {
        Node::Element(el) => (el.children.clone(), el.template_contents.clone()),
        Node::Document { children, .. } => (children.clone(), None),
        _ => return,
    };
    for child in &children {
        set_parent(child, node);
    }
    if let Some(content) = template_contents {
        link_children(&content);
    }
}

/// Recursively (re)establish parent back-pointers for an entire subtree.
///
/// Used to link a freshly parsed tree in one pass; mutation primitives maintain
/// the pointers incrementally thereafter.
pub fn reparent_subtree(node: &NodePtr) {
    let (children, template_contents): (Vec<NodePtr>, Option<NodePtr>) = match &*node.borrow() {
        Node::Element(el) => (el.children.clone(), el.template_contents.clone()),
        Node::Document { children, .. } => (children.clone(), None),
        _ => return,
    };
    for child in &children {
        set_parent(child, node);
        reparent_subtree(child);
    }
    if let Some(content) = template_contents {
        reparent_subtree(&content);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentMode {
    NoQuirks,
    Quirks,
    LimitedQuirks,
}

/// Enum representing different types of DOM nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Document root node containing top-level children.
    Document {
        children: Vec<NodePtr>,
        mode: DocumentMode,
    },
    /// Element node with tag name, attributes, and children.
    Element(ElementNode),
    /// Text node containing raw string content and a parent pointer.
    Text(TextNode),
}

/// HTML text node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextNode {
    pub content: String,
    /// Back-pointer to the parent node.
    pub parent: ParentLink,
}

/// HTML element node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementNode {
    /// HTML tag name, for example `div`, `p`, or `span`.
    pub tag_name: String,
    /// Map of attribute names to values.
    pub attributes: BTreeMap<String, String>,
    /// Child node pointers.
    pub children: Vec<NodePtr>,
    /// Parsed `<template>` contents, stored separately from light DOM children.
    pub template_contents: Option<NodePtr>,
    /// Back-pointer to the parent node, maintained by the mutation primitives so
    /// connectivity/ancestor queries are O(depth) instead of full-tree scans.
    pub parent: ParentLink,
}

impl Node {
    /// Create a document node wrapping top-level child nodes.
    pub fn document(children: Vec<NodePtr>) -> NodePtr {
        Self::document_with_mode(children, DocumentMode::NoQuirks)
    }

    pub fn document_with_mode(children: Vec<NodePtr>, mode: DocumentMode) -> NodePtr {
        Rc::new(RefCell::new(Self::Document { children, mode }))
    }

    pub fn document_fragment(children: Vec<NodePtr>) -> NodePtr {
        Self::element("#document-fragment", children)
    }

    /// Create an element node with tag name, attributes, and children.
    pub fn element_with_attributes(
        tag_name: impl Into<String>,
        attributes: BTreeMap<String, String>,
        children: Vec<NodePtr>,
    ) -> NodePtr {
        let node = Rc::new(RefCell::new(Self::Element(ElementNode {
            tag_name: tag_name.into(),
            attributes,
            children,
            template_contents: None,
            parent: ParentLink::default(),
        })));
        // Link any children supplied at construction time to this new node.
        link_children(&node);
        node
    }

    /// Create an element node with tag name and children.
    pub fn element(tag_name: impl Into<String>, children: Vec<NodePtr>) -> NodePtr {
        Self::element_with_attributes(tag_name, BTreeMap::new(), children)
    }

    /// Create a text node containing a string.
    pub fn text(value: impl Into<String>) -> NodePtr {
        Rc::new(RefCell::new(Self::Text(TextNode {
            content: value.into(),
            parent: ParentLink::default(),
        })))
    }
}
