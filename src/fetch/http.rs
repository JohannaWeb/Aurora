//! HTTP(S) transport via reqwest::blocking.
//! Replaces the hand-rolled TLS + chunked + redirect stack.

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};

use super::FetchError;

const USER_AGENT_STR: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Safari/537.36 Aurora/0.1";

fn client() -> Result<Client, FetchError> {
    Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(|e| FetchError::InvalidResponse(e.to_string()))
}

/// Fetch a URL and return the body as bytes.
/// Follows redirects automatically (reqwest handles this natively).
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    let response = client()?
        .get(url)
        .header(USER_AGENT, USER_AGENT_STR)
        .header(ACCEPT, "text/html, text/css, */*")
        .send()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(FetchError::HttpStatus(
            status,
            response.status().canonical_reason().unwrap_or("").to_string(),
        ));
    }

    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| FetchError::Network(e.to_string()))
}

/// Fetch a URL and return the body as a UTF-8 string.
pub fn fetch_string(url: &str) -> Result<String, FetchError> {
    let bytes = fetch_bytes(url)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
