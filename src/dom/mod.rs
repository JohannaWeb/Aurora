//! DOM node model.
//!
//! Public API: node types plus constructors and tree operations.

mod display;
mod node;
mod ops;

pub use node::{ElementNode, Node, NodePtr};
