use crate::css::{DisplayMode, StyleMap};
use std::fmt::{self, Display, Formatter};

use super::{LayoutBox, LayoutKind, LayoutTree};

impl LayoutBox {
    pub(in crate::layout) fn fmt_with_indent(
        &self,
        f: &mut Formatter<'_>,
        depth: usize,
    ) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match &self.kind {
            LayoutKind::Viewport => {
                writeln!(f, "{indent}viewport {}", self.rect)?;
            }
            LayoutKind::Block { tag_name } => {
                writeln!(
                    f,
                    "{indent}block<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::InlineBlock { tag_name } => {
                writeln!(
                    f,
                    "{indent}inline-block<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Inline { tag_name } => {
                writeln!(
                    f,
                    "{indent}inline<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Control { tag_name } => {
                writeln!(
                    f,
                    "{indent}control<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Image {
                alt,
                src,
                display_mode,
            } => {
                let kind = if *display_mode == DisplayMode::Inline {
                    "inline"
                } else {
                    "block"
                };
                writeln!(
                    f,
                    "{indent}{kind}<img alt={:?} src={:?}> {} {}",
                    alt,
                    src,
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Text { text } => {
                writeln!(f, "{indent}text(\"{text}\") {}", self.rect)?;
            }
        }

        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }
}

impl Display for LayoutTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.root().fmt_with_indent(f, 0)
    }
}

fn format_styles(styles: &StyleMap) -> String {
    if styles.is_empty() {
        "{}".to_string()
    } else {
        format!("{styles}")
    }
}
