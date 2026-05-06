//! Fetch local files and HTTP(S) resources.
//!
//! Public API: string/byte fetching and relative URL resolution.

mod api;
mod capability;
mod chunked;
mod errors;
mod headers;
mod http;
mod redirects;
mod resolve;
mod tls;
mod url;

#[cfg(test)]
mod tests;

pub use api::{fetch_bytes, fetch_html, fetch_string};
pub use errors::FetchError;
pub use resolve::resolve_relative_url;
