//! Layout: turn a styled DOM tree into positioned boxes.

mod block;
mod r#box;
mod constants;
mod constraints;
mod construct;
mod control;
mod display;
mod flex;
mod image;
mod inline;
mod inline_sequence;
mod inline_text;
mod rect;
mod text_metrics;
mod tree;

#[cfg(test)]
mod tests;

pub use r#box::LayoutBox;
pub use rect::Rect;
pub use tree::{LayoutTree, ViewportSize};

pub(in crate::layout) use r#box::LayoutKind;
