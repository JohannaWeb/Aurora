//! DOM node model.
//!
//! Public API: node types plus constructors and tree operations.

mod display;
mod node;

pub use node::{DocumentMode, ElementNode, Node, NodePtr};
