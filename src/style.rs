use crate::css::{StyleMap, Stylesheet};
use crate::dom::{ElementNode, Node};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

#[derive(Default, Clone)]
struct InheritedStyles {
    color: Option<String>,
    font_size: Option<String>,
    font_weight: Option<String>,
    line_height: Option<String>,
    visibility: Option<String>,
    text_decoration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleTree {
    root: StyledNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledNode {
    kind: StyledNodeKind,
    styles: StyleMap,
    children: Vec<StyledNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StyledNodeKind {
    Document,
    Element {
        tag_name: String,
        attributes: BTreeMap<String, String>,
    },
    Text { text: String },
}

impl StyleTree {
    pub fn from_dom(document: &Node, stylesheet: &Stylesheet) -> Self {
        Self {
            root: StyledNode::from_dom_node(document, stylesheet, &[], InheritedStyles::default()),
        }
    }

    pub fn root(&self) -> &StyledNode {
        &self.root
    }
}

impl StyledNode {
    pub fn styles(&self) -> &StyleMap {
        &self.styles
    }

    pub fn children(&self) -> &[StyledNode] {
        &self.children
    }

    pub fn tag_name(&self) -> Option<&str> {
        match &self.kind {
            StyledNodeKind::Element { tag_name, .. } => Some(tag_name),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            StyledNodeKind::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        match &self.kind {
            StyledNodeKind::Element { attributes, .. } => {
                attributes.get(name).map(String::as_str)
            }
            _ => None,
        }
    }

    fn from_dom_node(
        node: &Node,
        stylesheet: &Stylesheet,
        ancestors: &[&ElementNode],
        inherited: InheritedStyles,
    ) -> Self {
        match node {
            Node::Document { children } => Self {
                kind: StyledNodeKind::Document,
                styles: StyleMap::default(),
                children: children
                    .iter()
                    .map(|child| {
                        Self::from_dom_node(
                            child,
                            stylesheet,
                            ancestors,
                            inherited.clone(),
                        )
                    })
                    .collect(),
            },
            Node::Element(element) => {
                let mut styles = stylesheet.styles_for(element, ancestors);

                // Inherit color
                if styles.get("color").is_none() {
                    if let Some(color) = &inherited.color {
                        styles.set("color", color);
                    }
                }

                // Inherit font-size
                if styles.get("font-size").is_none() {
                    if let Some(font_size) = &inherited.font_size {
                        styles.set("font-size", font_size);
                    }
                }

                // Inherit font-weight
                if styles.get("font-weight").is_none() {
                    if let Some(font_weight) = &inherited.font_weight {
                        styles.set("font-weight", font_weight);
                    }
                }

                // Inherit line-height
                if styles.get("line-height").is_none() {
                    if let Some(line_height) = &inherited.line_height {
                        styles.set("line-height", line_height);
                    }
                }

                // Inherit visibility
                if styles.get("visibility").is_none() {
                    if let Some(visibility) = &inherited.visibility {
                        styles.set("visibility", visibility);
                    }
                }

                // Inherit text-decoration
                if styles.get("text-decoration").is_none() {
                    if let Some(text_decoration) = &inherited.text_decoration {
                        styles.set("text-decoration", text_decoration);
                    }
                }

                let mut next_ancestors = ancestors.to_vec();
                next_ancestors.push(element);

                let next_inherited = InheritedStyles {
                    color: styles.get("color").map(ToOwned::to_owned),
                    font_size: styles.get("font-size").map(ToOwned::to_owned),
                    font_weight: styles.get("font-weight").map(ToOwned::to_owned),
                    line_height: styles.get("line-height").map(ToOwned::to_owned),
                    visibility: styles.get("visibility").map(ToOwned::to_owned),
                    text_decoration: styles.get("text-decoration").map(ToOwned::to_owned),
                };

                Self {
                    kind: StyledNodeKind::Element {
                        tag_name: element.tag_name.clone(),
                        attributes: element.attributes.clone(),
                    },
                    styles,
                    children: element
                        .children
                        .iter()
                        .map(|child| {
                            Self::from_dom_node(
                                child,
                                stylesheet,
                                &next_ancestors,
                                next_inherited.clone(),
                            )
                        })
                        .collect(),
                }
            }
            Node::Text(text) => {
                let mut styles = StyleMap::default();

                if let Some(color) = inherited.color {
                    styles.set("color", color);
                }
                if let Some(font_size) = inherited.font_size {
                    styles.set("font-size", font_size);
                }
                if let Some(font_weight) = inherited.font_weight {
                    styles.set("font-weight", font_weight);
                }
                if let Some(line_height) = inherited.line_height {
                    styles.set("line-height", line_height);
                }
                if let Some(visibility) = inherited.visibility {
                    styles.set("visibility", visibility);
                }
                if let Some(text_decoration) = inherited.text_decoration {
                    styles.set("text-decoration", text_decoration);
                }

                Self {
                    kind: StyledNodeKind::Text { text: text.clone() },
                    styles,
                    children: Vec::new(),
                }
            }
        }
    }

    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match &self.kind {
            StyledNodeKind::Document => writeln!(f, "{indent}#styled-document")?,
            StyledNodeKind::Element { tag_name, .. } => {
                writeln!(f, "{indent}<{tag_name}> {}", self.styles)?
            }
            StyledNodeKind::Text { text } => writeln!(f, "{indent}\"{text}\" {}", self.styles)?,
        }

        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }
}

impl Display for StyleTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.root.fmt_with_indent(f, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::StyleTree;
    use crate::css::Stylesheet;
    use crate::dom::Node;
    use std::collections::BTreeMap;

    #[test]
    fn computes_descendant_matched_styles() {
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

        assert!(rendered.contains("<p> {color: gold, display: inline}"));
        assert!(rendered.contains("\"Hello\" {color: gold}"));
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
        assert!(rendered.contains("\"Inherited\" {color: slate}"));
    }

    #[test]
    fn inherits_typography_properties() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Text")])],
        )]);

        let stylesheet = Stylesheet::parse("body { font-size: 16px; font-weight: bold; line-height: 20px; }");
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
