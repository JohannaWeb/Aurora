//! Fetch local files and HTTP(S) resources.
//!
//! Public API: string/byte fetching and relative URL resolution.

mod api;
mod capability;
mod data_url;
mod errors;
pub mod http;
mod resolve;

#[cfg(test)]
mod tests;

pub use api::{fetch_bytes, fetch_html, fetch_string};
pub use errors::FetchError;
pub use resolve::resolve_relative_url;
