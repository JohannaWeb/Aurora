use std::fmt::{self, Display, Formatter};

use super::{StyleMap, Stylesheet};

impl Display for Stylesheet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.rules.is_empty() {
            return writeln!(f, "(empty)");
        }
        for rule in &self.rules {
            let sel_str = {
                use cssparser::ToCss;
                rule.selector.to_css_string()
            };
            write!(f, "{sel_str} ")?;
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
