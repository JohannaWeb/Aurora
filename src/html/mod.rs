//! HTML parsing into the Aurora DOM.
//!
//! Public API: `Parser`.

mod classify;
mod parser;
mod tag_parsing;
mod text;
mod tokenizer;
mod tokens;

#[cfg(test)]
mod tests;

pub use parser::Parser;
