use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Shared mutable DOM node pointer.
///
/// RUST FUNDAMENTAL: `Rc<T>` provides shared ownership and `RefCell<T>`
/// provides runtime-checked interior mutability.
pub type NodePtr = Rc<RefCell<Node>>;

/// Enum representing different types of DOM nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Document root node containing top-level children.
    Document { children: Vec<NodePtr> },
    /// Element node with tag name, attributes, and children.
    Element(ElementNode),
    /// Text node containing raw string content.
    Text(String),
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
}

impl Node {
    /// Create a document node wrapping top-level child nodes.
    pub fn document(children: Vec<NodePtr>) -> NodePtr {
        Rc::new(RefCell::new(Self::Document { children }))
    }

    /// Create an element node with tag name, attributes, and children.
    pub fn element_with_attributes(
        tag_name: impl Into<String>,
        attributes: BTreeMap<String, String>,
        children: Vec<NodePtr>,
    ) -> NodePtr {
        Rc::new(RefCell::new(Self::Element(ElementNode {
            tag_name: tag_name.into(),
            attributes,
            children,
        })))
    }

    /// Create an element node with tag name and children.
    pub fn element(tag_name: impl Into<String>, children: Vec<NodePtr>) -> NodePtr {
        Self::element_with_attributes(tag_name, BTreeMap::new(), children)
    }

    /// Create a text node containing a string.
    pub fn text(value: impl Into<String>) -> NodePtr {
        Rc::new(RefCell::new(Self::Text(value.into())))
    }
}
