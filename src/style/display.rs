use crate::dom::Node;
use std::fmt::{self, Display, Formatter};

use super::{StyleTree, StyledNode};

impl StyledNode {
    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let node_borrow = self.node.borrow();

        match &*node_borrow {
            Node::Document { .. } => writeln!(f, "{indent}#styled-document")?,
            Node::Element(el) => writeln!(f, "{indent}<{}> {}", el.tag_name, self.styles)?,
            Node::Text(text) => writeln!(f, "{indent}\"{text}\" {}", self.styles)?,
        }

        drop(node_borrow);
        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }
}

impl Display for StyleTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.root().fmt_with_indent(f, 0)
    }
}
