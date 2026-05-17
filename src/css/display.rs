use std::fmt::{self, Display, Formatter};

use super::{Combinator, Selector, SelectorPart, SimpleSelector, StyleMap, Stylesheet};

impl Display for Stylesheet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.rules.is_empty() {
            return writeln!(f, "(empty)");
        }

        for rule in &self.rules {
            write!(f, "{} ", rule.selector)?;
            write!(f, "{{")?;
            for (index, declaration) in rule.declarations.iter().enumerate() {
                if index > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}: {}", declaration.name, declaration.value)?;
                if declaration.important {
                    write!(f, " !important")?;
                }
                write!(f, ";")?;
            }
            writeln!(f, " }}")?;
        }

        Ok(())
    }
}

impl Display for Selector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                match part.combinator {
                    Combinator::Descendant => write!(f, " ")?,
                    Combinator::Child => write!(f, " > ")?,
                    Combinator::Adjacent => write!(f, " + ")?,
                    Combinator::Sibling => write!(f, " ~ ")?,
                }
            }
            write!(f, "{}", part.simple)?;
        }
        Ok(())
    }
}

impl Display for SelectorPart {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.simple)
    }
}

impl Display for SimpleSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(tag) = &self.tag_name {
            write!(f, "{tag}")?;
        }
        if let Some(id) = &self.id {
            write!(f, "#{id}")?;
        }
        for cn in &self.class_names {
            write!(f, ".{cn}")?;
        }
        for attr in &self.attributes {
            write!(f, "[{}]", attr.name)?;
        }
        Ok(())
    }
}

impl Display for StyleMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (index, (name, value)) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}: {value}")?;
        }
        write!(f, "}}")
    }
}
