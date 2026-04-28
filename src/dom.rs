// Import BTreeMap for ordered attribute storage
// RUST FUNDAMENTAL: Standard-library collections live under `std::collections`.
// Importing just the type name keeps later signatures short and readable.
use std::collections::BTreeMap;
// Import Display and Formatter traits for custom printing
// RUST FUNDAMENTAL: Traits often need to be in scope when you implement or use them explicitly.
use std::fmt::{self, Display, Formatter};

// Import Rc for shared reference counting pointers
// RUST FUNDAMENTAL: Smart pointers in Rust are normal library types, not built-in language syntax.
use std::rc::Rc;
// Import RefCell for interior mutability (allow mutation through shared ref)
use std::cell::RefCell;

// Type alias: NodePtr wraps a Node in Rc<RefCell<>> for shared mutable access
// RUST FUNDAMENTAL: A type alias created with `type` does not create a brand-new type.
// It is only a shorter name for an existing type, so `NodePtr` behaves exactly like `Rc<RefCell<Node>>`.
// The benefit is readability: the code can talk about "a node pointer" instead of repeating the full smart-pointer stack everywhere.
pub type NodePtr = Rc<RefCell<Node>>;
// RUST FUNDAMENTAL: DOM-like trees often need shared ownership and sometimes cycles.
// Plain Rust ownership prefers one clear owner, but a browser DOM often wants multiple handles to the same node.
// `Rc<T>` adds shared ownership by keeping a reference count; cloning the `Rc` clones the pointer, not the node itself.
// `RefCell<T>` adds "interior mutability", which means you can mutate the value even when it sits behind shared ownership.
// The tradeoff is that borrow rules are checked at runtime instead of compile time, so invalid borrow patterns panic.

// Enum representing different types of DOM nodes
// RUST FUNDAMENTAL: Rust enums are tagged unions: one value can be in exactly one variant at a time,
// and each variant can carry different data. Pattern matching is the normal way to inspect which variant you have
// and to destructure the fields stored inside that variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    // Document root node containing top-level children
    // RUST FUNDAMENTAL: This is a struct-like enum variant with a named field.
    // Named fields are useful when the payload has semantic meaning, because the code can destructure by field name:
    // `if let Node::Document { children } = node { ... }`.
    Document { children: Vec<NodePtr> },

    // Element node (tag) with attributes and child nodes
    // RUST FUNDAMENTAL: This is a tuple-style enum variant.
    // Tuple variants are convenient when the payload is conceptually "just one value of some type",
    // and you destructure it positionally: `if let Node::Element(el) = node { ... }`.
    Element(ElementNode),

    // Text node containing raw string content
    // RUST FUNDAMENTAL: `Text(String)` is also a tuple variant, but here the payload is the raw text itself.
    // The variant tells us what kind of node it is, and the `String` stores the owned text data.
    Text(String),
}

// Struct representing an HTML element with tag name, attributes, and children
// RUST FUNDAMENTAL: `pub struct` makes the type name visible outside the module.
// It does not automatically make every field public; each field still needs its own `pub` if outside code should read or write it directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementNode {
    // HTML tag name (e.g., "div", "p", "span")
    // RUST FUNDAMENTAL: `String` owns its UTF-8 bytes and usually stores them on the heap.
    // That makes it different from `&str`, which is only a borrowed view into string data owned somewhere else.
    pub tag_name: String,

    // Map of attribute names to values (e.g., id, class, src)
    // RUST FUNDAMENTAL: `BTreeMap<K, V>` keeps keys sorted and iterates in key order.
    // Unlike `HashMap`, it is deterministic for iteration, which can make debugging, testing, and printing more predictable.
    pub attributes: BTreeMap<String, String>,

    // Vector of child node pointers
    // RUST FUNDAMENTAL: `Vec<NodePtr>` means the children live in a growable contiguous collection,
    // and each child is itself behind `Rc<RefCell<_>>`, so children can be shared and mutated independently.
    pub children: Vec<NodePtr>,
}

// Implementation of Node factory methods
// RUST FUNDAMENTAL: An `impl` block is where methods and associated functions for a type are defined.
// Functions that take `self`, `&self`, or `&mut self` are methods.
// Functions without a self parameter are associated functions and are called like `Type::function_name(...)`.
impl Node {
    // Create a document node wrapping top-level child nodes
    // RUST FUNDAMENTAL: This constructor returns the aliased smart-pointer type instead of a plain `Node`.
    // The node is wrapped immediately so callers always get the shared/mutable representation the rest of the DOM uses.
    pub fn document(children: Vec<NodePtr>) -> NodePtr {
        // Wrap the Document variant in Rc<RefCell<>> for shared mutable access
        // RUST FUNDAMENTAL: `Rc::new(...)` allocates a reference-counted owner for the value.
        // `RefCell::new(...)` sits inside it and enables runtime-checked borrowing for mutation later.
        // We move `children` into the node here, so this constructor becomes the new owner of that vector.
        Rc::new(RefCell::new(Self::Document { children }))
    }

    // Create an element node with tag name, attributes, and children
    // RUST FUNDAMENTAL: `impl Into<String>` makes the API more ergonomic because callers can pass any value
    // that knows how to convert into a `String`, such as `String` or `&str`.
    // That keeps the conversion logic at the boundary instead of forcing every caller to write `.to_string()` first.
    pub fn element_with_attributes(
        // Tag name convertible to String
        // RUST FUNDAMENTAL: This is shorthand for "some concrete type chosen by the caller that implements `Into<String>`".
        // The function stays generic, but callers do not need to see an explicit type parameter name.
        tag_name: impl Into<String>,

        // Map of attributes for this element
        attributes: BTreeMap<String, String>,

        // Vector of child nodes
        children: Vec<NodePtr>,
    ) -> NodePtr {
        // Wrap ElementNode in Rc<RefCell<>> for shared mutable access
        Rc::new(RefCell::new(Self::Element(ElementNode {
            // Convert tag name to String
            // RUST FUNDAMENTAL: Calling `.into()` asks Rust to convert the input into the target type expected here.
            // Because the field is a `String`, the compiler resolves an `Into<String>` implementation at compile time.
            tag_name: tag_name.into(),

            // Store attributes map
            // RUST FUNDAMENTAL: This is a move, not a copy.
            // Ownership of the `BTreeMap` is transferred into the new `ElementNode`, so the constructor avoids duplicating the map contents.
            attributes,

            // Store child nodes
            children,
        })))
    }

    // Create an element node with tag name and children (no attributes)
    // RUST FUNDAMENTAL: This is a small convenience constructor.
    // Reusing `element_with_attributes` avoids duplicating creation logic and keeps one canonical implementation.
    pub fn element(tag_name: impl Into<String>, children: Vec<NodePtr>) -> NodePtr {
        // Delegate to element_with_attributes using empty attribute map
        // RUST FUNDAMENTAL: `BTreeMap::new()` creates an empty map value.
        // That value is then moved into `element_with_attributes`, which becomes its owner.
        Self::element_with_attributes(tag_name, BTreeMap::new(), children)
    }

    // Create a text node containing a string
    pub fn text(value: impl Into<String>) -> NodePtr {
        // Wrap Text variant in Rc<RefCell<>> for shared mutable access
        // RUST FUNDAMENTAL: Text nodes use the same `Rc<RefCell<_>>` wrapper as element nodes so the entire DOM
        // has one consistent ownership model. That matters because other subsystems, like JavaScript integration,
        // may need to hold shared references and still mutate nodes later.
        Rc::new(RefCell::new(Self::Text(value.into())))
    }

    // Find a node by its ID attribute and return the path to it
    pub fn find_node_by_id(&self, id: &str) -> Option<Vec<usize>> {
        // Create empty path vector to track indices
        // RUST FUNDAMENTAL: `Vec<usize>` is a natural representation for a path of child indexes through a tree.
        let mut path = Vec::new();
        // Recursively search for node with matching ID
        // RUST FUNDAMENTAL: Passing `&mut path` means the recursive helper updates one shared buffer
        // instead of allocating a fresh path vector at every recursive step.
        if self.find_node_by_id_recursive(id, &mut path) {
            // Return path if found
            // RUST FUNDAMENTAL: Moving `path` into `Some(path)` transfers ownership of the vector to the caller.
            Some(path)
        } else {
            // Return None if not found
            None
        }
    }

    // Recursively search DOM tree for element with matching ID attribute
    fn find_node_by_id_recursive(&self, id: &str, path: &mut Vec<usize>) -> bool {
        // Match on current node type
        // RUST FUNDAMENTAL: Matching on `&self` borrows the enum instead of consuming it,
        // which is why this method can traverse the tree without taking ownership of any node.
        match self {
            // For document nodes, search all children
            Node::Document { children } => {
                // Iterate through children with their indices
                // RUST FUNDAMENTAL: `.enumerate()` decorates each iterator item with its zero-based index.
                for (i, child) in children.iter().enumerate() {
                    // Record current index in path
                    // RUST FUNDAMENTAL: `push` mutates the shared path buffer by appending one more step.
                    path.push(i);
                    // Recursively check if child contains target ID
                    // RUST FUNDAMENTAL: `child.borrow()` creates a temporary shared borrow of the inner `Node`
                    // so we can call another `&self` method on it.
                    if child.borrow().find_node_by_id_recursive(id, path) {
                        // Return true if found
                        return true;
                    }
                    // Remove index from path (backtrack)
                    // RUST FUNDAMENTAL: This is classic backtracking: undo the most recent choice before exploring the next branch.
                    path.pop();
                }
            }
            // For element nodes, check ID then search children
            Node::Element(element) => {
                // Check if this element has matching ID attribute
                // RUST FUNDAMENTAL: Chaining `get(...).map(...).unwrap_or(false)` is a compact way to say
                // "compare the value if it exists, otherwise treat it as false".
                if element
                    .attributes
                    .get("id")
                    .map(|v| v == id)
                    .unwrap_or(false)
                {
                    // Return true if ID matches
                    return true;
                }
                // Iterate through children with their indices
                for (i, child) in element.children.iter().enumerate() {
                    // Record current index in path
                    path.push(i);
                    // Recursively check if child contains target ID
                    if child.borrow().find_node_by_id_recursive(id, path) {
                        // Return true if found
                        return true;
                    }
                    // Remove index from path (backtrack)
                    path.pop();
                }
            }
            // Text nodes have no attributes, skip them
            Node::Text(_) => {}
        }
        // Return false if target not found in this subtree
        false
    }

    // Get a child node by index (from Document or Element)
    pub fn get_child_mut(&mut self, index: usize) -> Option<NodePtr> {
        // Match on node type
        // RUST FUNDAMENTAL: Even though this method takes `&mut self`, it still returns cloned `Rc` pointers
        // instead of direct `&mut Node` references, because this DOM representation is built around shared ownership.
        match self {
            // For document, return child at index if exists
            // RUST FUNDAMENTAL: `.get(index)` returns `Option<&T>` to handle out-of-bounds indexes safely.
            Node::Document { children } => children.get(index).cloned(),
            // For element, return child at index if exists
            Node::Element(element) => element.children.get(index).cloned(),
            // Text nodes have no children
            Node::Text(_) => None,
        }
    }

    /*
    pub fn get_node_at_path_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let mut current = self;
        for &index in path {
            match current {
                Node::Document { children } => {
                    current = &mut *children.get_mut(index)?.borrow_mut();
                }
                Node::Element(element) => {
                    current = &mut *element.children.get_mut(index)?.borrow_mut();
                }
                Node::Text(_) => return None,
            }
        }
        Some(current)
    }
    */

    // Helper function to format node with indentation for pretty-printing
    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        // Create indentation string (2 spaces per depth level)
        // RUST FUNDAMENTAL: `"  ".repeat(depth)` allocates a new `String` containing repeated copies of the input slice.
        let indent = "  ".repeat(depth);
        // Match on node type
        match self {
            // Document node: print label and all children indented
            Node::Document { children } => {
                // Write document label at current indentation
                // RUST FUNDAMENTAL: `writeln!` writes formatted text into any type implementing `fmt::Write`-style formatting APIs.
                // The trailing `?` propagates formatting errors upward.
                writeln!(f, "{indent}#document")?;
                // Recursively format each child with increased indentation
                for child in children {
                    child.borrow().fmt_with_indent(f, depth + 1)?;
                }
                // Return ok result
                Ok(())
            }
            // Element node: print tag with attributes and children
            Node::Element(element) => {
                // Write opening tag name
                // RUST FUNDAMENTAL: `write!` is like `writeln!` but does not append a newline automatically.
                write!(f, "{indent}<{}", element.tag_name)?;
                // Write all attributes as name="value" pairs
                for (name, value) in &element.attributes {
                    write!(f, " {name}=\"{value}\"")?;
                }
                // Close opening tag
                writeln!(f, ">")?;
                // Recursively format each child with increased indentation
                for child in &element.children {
                    child.borrow().fmt_with_indent(f, depth + 1)?;
                }
                // Return ok result
                Ok(())
            }
            // Text node: print quoted text content
            Node::Text(text) => writeln!(f, "{indent}\"{text}\""),
        }
    }
}

// Trait implementation to convert Node to Display string
// RUST FUNDAMENTAL: Implementing `Display` lets a type participate in user-facing `{}` formatting.
impl Display for Node {
    // Format node for display using indentation formatting
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Delegate to fmt_with_indent starting at depth 0
        // RUST FUNDAMENTAL: Small helper methods like this keep trait implementations thin and make recursion easier to reuse.
        self.fmt_with_indent(f, 0)
    }
}
