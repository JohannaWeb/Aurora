use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Document { children: Vec<Node> },
    Element(ElementNode),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementNode {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn document(children: Vec<Node>) -> Self {
        Self::Document { children }
    }

    #[cfg(test)]
    pub fn element(tag_name: impl Into<String>, children: Vec<Node>) -> Self {
        Self::element_with_attributes(tag_name, BTreeMap::new(), children)
    }

    pub fn element_with_attributes(
        tag_name: impl Into<String>,
        attributes: BTreeMap<String, String>,
        children: Vec<Node>,
    ) -> Self {
        Self::Element(ElementNode {
            tag_name: tag_name.into(),
            attributes,
            children,
        })
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match self {
            Node::Document { children } => {
                writeln!(f, "{indent}#document")?;
                for child in children {
                    child.fmt_with_indent(f, depth + 1)?;
                }
                Ok(())
            }
            Node::Element(element) => {
                write!(f, "{indent}<{}", element.tag_name)?;
                for (name, value) in &element.attributes {
                    write!(f, " {name}=\"{value}\"")?;
                }
                writeln!(f, ">")?;
                for child in &element.children {
                    child.fmt_with_indent(f, depth + 1)?;
                }
                Ok(())
            }
            Node::Text(text) => writeln!(f, "{indent}\"{text}\""),
        }
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}
