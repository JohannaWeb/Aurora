use std::fmt::{self, Display, Formatter};

use super::Node;

impl Node {
    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);

        match self {
            Node::Document { children } => {
                writeln!(f, "{indent}#document")?;
                for child in children {
                    child.borrow().fmt_with_indent(f, depth + 1)?;
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
                    child.borrow().fmt_with_indent(f, depth + 1)?;
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
