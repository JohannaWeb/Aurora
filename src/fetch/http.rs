//! HTTP(S) transport via reqwest::blocking.
//! Replaces the hand-rolled TLS + chunked + redirect stack.

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::ACCEPT;

use super::FetchError;

// Wikimedia API etiquette requires a descriptive UA with contact info.
// Using a spoofed Chrome string causes aggressive rate-limiting on upload.wikimedia.org.
const USER_AGENT_STR: &str =
    "Aurora/0.1 (https://github.com/JohannaWeb/Aurora; experimental browser engine)";

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .use_rustls_tls()
        .user_agent(USER_AGENT_STR)
        .build()
        .expect("failed to build HTTP client")
});

// Global token bucket: at most one outgoing request every 150 ms.
// Blitz spawns one thread per image, so without this every image fires simultaneously
// and upload.wikimedia.org immediately 429s the whole batch.
static LAST_REQUEST: LazyLock<Mutex<Instant>> = LazyLock::new(|| {
    Mutex::new(Instant::now().checked_sub(Duration::from_millis(150)).unwrap_or(Instant::now()))
});
const REQUEST_INTERVAL: Duration = Duration::from_millis(150);

fn pace() {
    // Hold the lock while sleeping so queued threads each wait their turn
    // rather than all waking simultaneously after one interval.
    let mut last = LAST_REQUEST.lock().unwrap();
    let elapsed = last.elapsed();
    if elapsed < REQUEST_INTERVAL {
        std::thread::sleep(REQUEST_INTERVAL - elapsed);
    }
    *last = Instant::now();
}

fn client() -> &'static Client {
    &CLIENT
}

/// Fetch a URL and return the body as bytes.
/// Follows redirects automatically (reqwest handles this natively).
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    pace();
    let response = client()
        .get(url)
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
