//! Layout: turn a styled DOM tree into positioned boxes.

mod block;
mod r#box;
pub mod document;
mod constants;
mod constraints;
mod construct;
mod control;
mod display;
mod engine;
mod flex;
mod image;
mod inline;
mod inline_sequence;
mod inline_text;
mod parley_text;
mod rect;
mod taffy_adapter;
mod taffy_layout;
mod text_metrics;
mod tree;

#[cfg(test)]
mod tests;

pub use r#box::LayoutBox;
pub use rect::Rect;
#[allow(unused_imports)]
pub use taffy_adapter::style_to_taffy;
pub use tree::{LayoutTree, ViewportSize};

pub(in crate::layout) use r#box::LayoutKind;
