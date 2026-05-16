//! Layout: turn a styled DOM tree into positioned boxes.

mod r#box;
pub mod document;
mod constants;
mod constraints;
mod control;
mod display;
mod image;
mod inline;
mod inline_sequence;
mod inline_text;
pub mod parley_text;
pub mod taffy_adapter;
pub mod taffy_layout;

pub use r#box::{LayoutBox, LayoutKind, Rect};
pub use document::LayoutDocument;
