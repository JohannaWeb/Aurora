// Import CSS styling types
// RUST FUNDAMENTAL: Pulling related types in at the top of the module makes the rest of the file read more like the domain
// and less like a pile of long fully-qualified paths.
use crate::css::{StyleMap, Stylesheet};
// Import DOM node types
use crate::dom::{ElementNode, Node};
// Import BTreeMap (though unused, kept for potential future use)
use std::collections::BTreeMap;
// Import Display formatting traits
use std::fmt::{self, Display, Formatter};

// Struct tracking CSS properties that inherit to child elements
// RUST FUNDAMENTAL: `#[derive(Default, Clone)]` asks the compiler to generate common boilerplate for us.
// `Default` builds an "empty" value for the struct, which here means every `Option` field starts as `None`.
// `Clone` generates field-by-field cloning so inherited style state can be copied when recursing down the tree.
#[derive(Default, Clone)]
struct InheritedStyles {
    // Text color inherited to children
    // RUST FUNDAMENTAL: `Option<String>` distinguishes three useful ideas:
    // no inherited value (`None`), some inherited text (`Some(...)`), and the actual string contents when present.
    color: Option<String>,

    // Font size inherited to children
    font_size: Option<String>,

    // Font weight (bold/normal) inherited
    font_weight: Option<String>,

    // Line height for text inherited
    line_height: Option<String>,

    // Visibility (visible/hidden) inherited
    // RUST FUNDAMENTAL: This struct is a nice example of modeling "inherited state" explicitly.
    // Parent values are copied forward, and each child can either keep inheriting them or replace them with its own concrete value.
    visibility: Option<String>,

    // Text decoration (underline, etc.) inherited
    text_decoration: Option<String>,

    // Font style (italic, etc.) inherited
    font_style: Option<String>,
}

// Tree structure containing DOM nodes with applied CSS styles
// RUST FUNDAMENTAL: Wrapping the root node in its own `StyleTree` type gives the styled representation a clear entry point
// and leaves room for tree-level helper methods later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleTree {
    // Root node (usually the document)
    root: StyledNode,
}

// DOM node with associated CSS styles and child nodes
// RUST FUNDAMENTAL: This is a "parallel tree" structure.
// It points back to the original DOM node while also storing the computed style data and styled children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledNode {
    // Reference to original DOM node
    pub node: crate::dom::NodePtr,
    // CSS styles applied to this node
    pub styles: StyleMap,
    // Child nodes with their styles
    pub children: Vec<StyledNode>,
}

// StyleTree implementation
impl StyleTree {
    // Create style tree by applying stylesheet rules to DOM tree
    pub fn from_dom(document: &crate::dom::NodePtr, stylesheet: &Stylesheet) -> Self {
        Self {
            // Build styled node tree starting from document root
            // RUST FUNDAMENTAL: `Rc::clone(document)` clones the shared pointer, not the whole DOM tree.
            // The style tree and the caller both keep access to the same underlying DOM nodes.
            root: StyledNode::from_dom_node(
                std::rc::Rc::clone(document),
                stylesheet,
                &[],
                InheritedStyles::default(),
                &[],
            ),
        }
    }

    // Get root styled node of the tree
    pub fn root(&self) -> &StyledNode {
        &self.root
    }
}

// StyledNode implementation
impl StyledNode {
    // Get CSS styles for this node
    pub fn styles(&self) -> &StyleMap {
        // RUST FUNDAMENTAL: Returning `&StyleMap` borrows the stored styles instead of cloning them.
        // Getter methods like this are cheap because they only return references.
        &self.styles
    }

    // Get child nodes of this styled node
    pub fn children(&self) -> &[StyledNode] {
        // RUST FUNDAMENTAL: `&[T]` is a slice, which is the standard borrowed view of a sequence in Rust.
        &self.children
    }

    // Get HTML tag name if this node is an element
    pub fn tag_name(&self) -> Option<String> {
        // Borrow DOM node to access data
        // RUST FUNDAMENTAL: Because the DOM node lives behind `RefCell`, we must borrow it before reading its contents.
        let node = self.node.borrow();
        // Check if node is element and return tag name
        if let Node::Element(el) = &*node {
            // RUST FUNDAMENTAL: Cloning the `String` gives the caller an owned result that outlives the temporary borrow.
            Some(el.tag_name.clone())
        } else {
            None
        }
    }

    // Get text content if this node is a text node
    pub fn text(&self) -> Option<String> {
        // Borrow DOM node to access data
        let node = self.node.borrow();
        // Check if node is text and return content
        if let Node::Text(text) = &*node {
            // RUST FUNDAMENTAL: Returning owned data here avoids exposing borrow-guard lifetimes to callers of this helper.
            Some(text.clone())
        } else {
            None
        }
    }

    // Get attribute value by name if this is an element node
    pub fn attribute(&self, name: &str) -> Option<String> {
        // Match on node type to get attributes
        // RUST FUNDAMENTAL: Matching directly on the borrowed DOM node is a common pattern when the enum variant determines behavior.
        match &*self.node.borrow() {
            // For elements, look up attribute
            Node::Element(el) => el.attributes.get(name).cloned(),
            // Non-element nodes have no attributes
            _ => None,
        }
    }

    // Recursively build styled node tree from DOM, applying stylesheets
    fn from_dom_node(
        // DOM node to style
        node: crate::dom::NodePtr,
        // CSS stylesheet with rules to apply
        stylesheet: &Stylesheet,
        // Ancestor elements for CSS selector matching
        element_ancestors: &[crate::css::ElementData],
        // Inherited styles from parent element
        inherited: InheritedStyles,
        // Parent style maps for variable resolution
        style_ancestors: &[&StyleMap],
    ) -> Self {
        // Borrow DOM node to access its data
        // RUST FUNDAMENTAL: The recursive builder reads the DOM through a temporary borrow, then constructs a separate owned style tree.
        let node_borrow = node.borrow();
        // Pattern match on node type
        match &*node_borrow {
            // For document nodes, process all children
            Node::Document { children } => {
                // Clone children vector to avoid double borrow
                // RUST FUNDAMENTAL: Cloning a `Vec<NodePtr>` clones the `Rc` pointers inside it, not the full child subtrees.
                let children_vec = children.clone();
                // Drop borrow before recursing
                // RUST FUNDAMENTAL: Explicit `drop(node_borrow)` ends the active `RefCell` borrow early.
                // That matters because recursive calls may try to borrow related nodes again.
                drop(node_borrow);
                // Return styled document with styled children
                Self {
                    node,
                    // Documents have no styles
                    // RUST FUNDAMENTAL: `Default::default()` is a common way to create an empty or zero-value configuration object.
                    styles: StyleMap::default(),
                    // Recursively style all children
                    children: children_vec
                        .into_iter()
                        .map(|child| {
                            // Recursively build styled node for each child
                            // RUST FUNDAMENTAL: Iterator adapters like `.map(...)` plus `.collect()` are the standard Rust way
                            // to transform one collection into another.
                            Self::from_dom_node(
                                child,
                                stylesheet,
                                element_ancestors,
                                inherited.clone(),
                                style_ancestors,
                            )
                        })
                        .collect(),
                }
            }
            // For element nodes, apply CSS and build styled children
            Node::Element(element) => {
                // Build ElementData for CSS selector matching
                // RUST FUNDAMENTAL: Building an intermediate value like `ElementData` is often cleaner than repeatedly pulling fields
                // out of a larger structure in many separate calls.
                let current_data = crate::css::ElementData {
                    // Tag name from element
                    tag_name: element.tag_name.clone(),
                    // Attributes from element
                    attributes: element.attributes.clone(),
                };
                // Get styles matching this element from stylesheet
                // RUST FUNDAMENTAL: `let mut` is needed because later steps mutate the computed `styles` map in place.
                let mut styles = stylesheet.styles_for(&current_data, element_ancestors);

                // Resolve CSS variable references using parent styles
                // RUST FUNDAMENTAL: Mutating helper methods often take borrowed context plus `&mut self`,
                // letting one value be incrementally refined through several passes.
                styles.resolve_vars(style_ancestors);

                // Apply inherited styles if not explicitly set on element
                // These properties cascade from parent to child automatically
                if styles.get("color").is_none() {
                    if let Some(color) = &inherited.color {
                        // RUST FUNDAMENTAL: Borrowing `&inherited.color` avoids moving the option's contents out of `inherited`,
                        // so the remaining inherited fields can still be used afterward.
                        styles.set("color", color);
                    }
                }
                if styles.get("font-size").is_none() {
                    if let Some(font_size) = &inherited.font_size {
                        styles.set("font-size", font_size);
                    }
                }
                if styles.get("font-weight").is_none() {
                    if let Some(font_weight) = &inherited.font_weight {
                        styles.set("font-weight", font_weight);
                    }
                }
                if styles.get("line-height").is_none() {
                    if let Some(line_height) = &inherited.line_height {
                        styles.set("line-height", line_height);
                    }
                }
                if styles.get("visibility").is_none() {
                    if let Some(visibility) = &inherited.visibility {
                        styles.set("visibility", visibility);
                    }
                }
                if styles.get("text-decoration").is_none() {
                    if let Some(text_decoration) = &inherited.text_decoration {
                        styles.set("text-decoration", text_decoration);
                    }
                }

                // Build list of ancestors including current element
                // RUST FUNDAMENTAL: `.to_vec()` clones a slice into an owned vector so this function can extend it for child recursion.
                let mut next_element_ancestors = element_ancestors.to_vec();
                // Add current element to ancestor chain
                next_element_ancestors.push(current_data);

                // Build inherited styles to pass to children
                // RUST FUNDAMENTAL: `.map(ToOwned::to_owned)` converts `Option<&str>`-style borrowed values
                // into `Option<String>` owned values suitable for storing beyond the current borrow.
                let next_inherited = InheritedStyles {
                    // Extract color for next level
                    color: styles.get("color").map(ToOwned::to_owned),
                    // Extract font-size for next level
                    font_size: styles.get("font-size").map(ToOwned::to_owned),
                    // Extract font-weight for next level
                    font_weight: styles.get("font-weight").map(ToOwned::to_owned),
                    // Extract line-height for next level
                    line_height: styles.get("line-height").map(ToOwned::to_owned),
                    // Extract visibility for next level
                    visibility: styles.get("visibility").map(ToOwned::to_owned),
                    // Extract text-decoration for next level
                    text_decoration: styles.get("text-decoration").map(ToOwned::to_owned),
                    // Extract font-style for next level
                    font_style: styles.get("font-style").map(ToOwned::to_owned),
                };

                // Clone children vector to avoid double borrow of element
                let element_children = element.children.clone();
                // Drop borrow before building returned node
                drop(node_borrow);

                // Create styled node with styles but empty children
                // RUST FUNDAMENTAL: It is common to construct a partially complete struct first,
                // then fill in a field like `children` after more work is done.
                let mut node_to_return = Self {
                    node,
                    styles,
                    children: Vec::new(),
                };

                // Build ancestor chain of parent styles for CSS variable resolution
                let mut next_style_ancestors = style_ancestors.to_vec();
                // Add this element's styles to ancestor chain
                // RUST FUNDAMENTAL: Storing `&node_to_return.styles` here means child recursion sees the fully computed parent style map
                // without taking ownership of it.
                next_style_ancestors.push(&node_to_return.styles);

                // Recursively style all child elements
                node_to_return.children = element_children
                    .into_iter()
                    .map(|child| {
                        // Recursively build styled node for each child
                        // RUST FUNDAMENTAL: `next_inherited.clone()` duplicates the inherited-style bundle for each child branch,
                        // which keeps the recursion simple and avoids aliasing mutable state across siblings.
                        Self::from_dom_node(
                            child,
                            stylesheet,
                            &next_element_ancestors,
                            next_inherited.clone(),
                            &next_style_ancestors,
                        )
                    })
                    .collect();

                // Return the completely styled subtree
                node_to_return
            }
            // For text nodes, apply inherited text styles
            Node::Text(_text) => {
                // Create default styles for text node
                let mut styles = StyleMap::default();
                // Text is displayed inline by default
                // RUST FUNDAMENTAL: Text nodes do not get selector-matched element rules here,
                // so the builder seeds the style map with the behavior text should have by default.
                styles.set("display", "inline");

                // Apply inherited text color
                if let Some(color) = &inherited.color {
                    styles.set("color", color);
                }
                // Apply inherited font size
                if let Some(font_size) = &inherited.font_size {
                    styles.set("font-size", font_size);
                }
                // Apply inherited font weight
                if let Some(font_weight) = &inherited.font_weight {
                    styles.set("font-weight", font_weight);
                }
                // Apply inherited line height
                if let Some(line_height) = &inherited.line_height {
                    styles.set("line-height", line_height);
                }
                // Apply inherited visibility
                if let Some(visibility) = &inherited.visibility {
                    styles.set("visibility", visibility);
                }
                // Apply inherited text decoration
                if let Some(text_decoration) = &inherited.text_decoration {
                    styles.set("text-decoration", text_decoration);
                }
                // Apply inherited font style
                if let Some(font_style) = &inherited.font_style {
                    styles.set("font-style", font_style);
                }

                // Return styled text node with no children
                Self {
                    // RUST FUNDAMENTAL: `node.clone()` clones the shared node pointer so the styled tree keeps its own handle to the DOM text node.
                    node: node.clone(),
                    styles,
                    children: Vec::new(),
                }
            }
        }
    }

    // Format styled node tree with indentation for debug output
    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        // Create indentation string for pretty-printing
        let indent = "  ".repeat(depth);
        // Borrow DOM node to access its data
        let node_borrow = self.node.borrow();
        // Match on node type for formatting
        match &*node_borrow {
            // Document nodes print as label
            Node::Document { .. } => writeln!(f, "{indent}#styled-document")?,
            // Element nodes print tag and styles
            Node::Element(el) => {
                // RUST FUNDAMENTAL: Because `StyleMap` implements formatting, it can be embedded directly inside another format string.
                writeln!(f, "{indent}<{}> {}", el.tag_name, self.styles)?
            }
            // Text nodes print quoted text and styles
            Node::Text(text) => writeln!(f, "{indent}\"{text}\" {}", self.styles)?,
        }
        // Drop borrow before recursing
        drop(node_borrow);

        // Recursively format all children with increased indentation
        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        // Return ok result
        Ok(())
    }
}

// Trait implementation to print StyleTree
// RUST FUNDAMENTAL: Implementing `Display` on the wrapper type lets callers print the whole tree with one `{}` formatter.
impl Display for StyleTree {
    // Format entire style tree
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Delegate to fmt_with_indent starting at depth 0
        self.root.fmt_with_indent(f, 0)
    }
}

#[cfg(test)]
mod tests {
    // RUST FUNDAMENTAL: Test modules usually import only the small surface area they need,
    // which keeps the assertions readable and local.
    use super::StyleTree;
    use crate::css::Stylesheet;
    use crate::dom::{Node, NodePtr};
    use std::collections::BTreeMap;

    fn element(tag: &str, children: Vec<NodePtr>) -> NodePtr {
        // RUST FUNDAMENTAL: Tiny test helpers reduce duplication and make expected tree shapes easier to scan.
        Node::element(tag, children)
    }

    #[test]
    fn computes_descendant_matched_styles() {
        // RUST FUNDAMENTAL: Tests often build concrete expected input structures directly rather than going through parsers,
        // so failures are easier to localize.
        let mut section_attributes = BTreeMap::new();
        section_attributes.insert("class".to_string(), "hero".to_string());
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element_with_attributes(
                "section",
                section_attributes,
                vec![Node::element("p", vec![Node::text("Hello")])],
            )],
        )]);

        let stylesheet = Stylesheet::parse("section.hero p { color: gold; display: inline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let rendered = style_tree.to_string();

        // RUST FUNDAMENTAL: String-based assertions are a pragmatic way to validate pretty-printed debug output.
        assert!(rendered.contains("<p> {color: gold, display: inline}"));
        assert!(rendered.contains("\"Hello\" {color: gold, display: inline}"));
    }

    #[test]
    fn inherits_color_to_descendants() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Inherited")])],
        )]);

        let stylesheet = Stylesheet::parse("body { color: slate; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let rendered = style_tree.to_string();

        assert!(rendered.contains("<p> {color: slate}"));
        assert!(rendered.contains("\"Inherited\" {color: slate, display: inline}"));
    }

    #[test]
    fn inherits_typography_properties() {
        let dom = Node::document(vec![element(
            "body",
            vec![element("p", vec![Node::text("Text")])],
        )]);

        let stylesheet =
            Stylesheet::parse("body { font-size: 16px; font-weight: bold; line-height: 20px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let rendered = style_tree.to_string();

        assert!(rendered.contains("font-size: 16px"));
        assert!(rendered.contains("font-weight: bold"));
        assert!(rendered.contains("line-height: 20px"));
    }

    #[test]
    fn inherits_visibility() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Text")])],
        )]);

        let stylesheet = Stylesheet::parse("body { visibility: hidden; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let rendered = style_tree.to_string();

        assert!(rendered.contains("visibility: hidden"));
    }
}
