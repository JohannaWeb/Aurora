//! HTML parsing into the Aurora DOM.
//!
//! Public API: `Parser`.

mod parser;

#[cfg(test)]
mod tests;

pub use parser::Parser;
