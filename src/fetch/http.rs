//! HTTP(S) transport via reqwest::blocking.
//! Replaces the hand-rolled TLS + chunked + redirect stack.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::FetchError;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use url::Url;

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
    Mutex::new(
        Instant::now()
            .checked_sub(Duration::from_millis(150))
            .unwrap_or(Instant::now()),
    )
});
const REQUEST_INTERVAL: Duration = Duration::from_millis(150);
static RATE_LIMITERS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn pace(url: &str) {
    let host = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default();

    let throttled = matches!(host.as_str(), "upload.wikimedia.org" | "en.wikipedia.org");
    if !throttled {
        return;
    }

    let sleep_for = {
        let mut map = RATE_LIMITERS.lock().unwrap();
        let last = map
            .entry(host)
            .or_insert_with(|| Instant::now() - REQUEST_INTERVAL);
        let elapsed = last.elapsed();
        let sleep_for = if elapsed < REQUEST_INTERVAL {
            REQUEST_INTERVAL - elapsed
        } else {
            Duration::ZERO
        };
        *last = Instant::now() + sleep_for;
        sleep_for
    }; // lock dropped here

    if sleep_for > Duration::ZERO {
        std::thread::sleep(sleep_for);
    }
}

fn client() -> &'static Client {
    &CLIENT
}

/// Fetch a URL and return the body as bytes.
/// Follows redirects automatically (reqwest handles this natively).
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    pace(url);
    let response = client()
        .get(url)
        .header(ACCEPT, "text/html, text/css, */*")
        .send()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(FetchError::HttpStatus(
            status,
            response
                .status()
                .canonical_reason()
                .unwrap_or("")
                .to_string(),
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
