//! HTTP(S) transport via reqwest::blocking.
//! Replaces the hand-rolled TLS + chunked + redirect stack.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::FetchError;
use reqwest::Method;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use url::Url;

/// An HTTP response before browser-facing status handling is applied.
///
/// `fetch_bytes_with_method` keeps the historical Aurora behavior of turning
/// non-success statuses into `FetchError::HttpStatus`. JavaScript `fetch` and
/// XHR need the status and response body even for 4xx/5xx responses, so they
/// use this lower-level representation instead.
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Spoofed Chrome UA: sites gate their modern bundles on UA sniffing —
/// YouTube serves an ES5 + custom-elements-adapter build to unknown browsers.
/// Also exposed as `navigator.userAgent` by the JS runtimes.
pub(crate) const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36";

// Wikimedia API etiquette requires a descriptive UA with contact info.
// Using a spoofed Chrome string causes aggressive rate-limiting on upload.wikimedia.org.
const AURORA_UA: &str =
    "Aurora/0.1 (https://github.com/JohannaWeb/Aurora; experimental browser engine)";

fn ua_for(url: &str) -> &'static str {
    let host = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default();
    if host.ends_with("wikimedia.org") || host.ends_with("wikipedia.org") {
        AURORA_UA
    } else {
        CHROME_UA
    }
}

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .use_rustls_tls()
        .user_agent(AURORA_UA)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build HTTP client")
});

// Global token bucket: at most one outgoing request every 150 ms.
// Blitz spawns one thread per image, so without this every image fires simultaneously
// and upload.wikimedia.org immediately 429s the whole batch.
#[allow(dead_code)]
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
    fetch_bytes_with_method(url, "GET", None)
}

pub fn fetch_bytes_with_method(
    url: &str,
    method: &str,
    body: Option<&str>,
) -> Result<Vec<u8>, FetchError> {
    let response = fetch_response_with_method(url, method, body, &[])?;
    if !(200..300).contains(&response.status) {
        return Err(FetchError::HttpStatus(
            response.status,
            response.status_text,
        ));
    }
    Ok(response.body)
}

/// Fetch a URL without converting HTTP error statuses into transport errors.
/// Request headers are restricted to names JavaScript is allowed to control;
/// transport-owned headers such as Host, Content-Length, Cookie, and User-Agent
/// remain under Aurora/reqwest control.
pub fn fetch_response_with_method(
    url: &str,
    method: &str,
    body: Option<&str>,
    headers: &[(String, String)],
) -> Result<HttpResponse, FetchError> {
    pace(url);
    let method = Method::from_bytes(method.as_bytes()).unwrap_or(Method::GET);
    let mut request = client()
        .request(method, url)
        .header(ACCEPT, "text/html, text/css, */*")
        .header(USER_AGENT, ua_for(url));

    let mut has_content_type = false;
    for (name, value) in headers {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        if is_forbidden_request_header(&name) {
            continue;
        }
        let Ok(value) = HeaderValue::from_str(value) else {
            continue;
        };
        if name == CONTENT_TYPE {
            has_content_type = true;
        }
        request = request.header(name, value);
    }
    if let Some(body) = body {
        if !has_content_type {
            request = request.header(CONTENT_TYPE, "application/json");
        }
        request = request.body(body.to_string());
    }
    let response = request
        .send()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect();
    let body = response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| FetchError::Network(e.to_string()))?;

    Ok(HttpResponse {
        status,
        status_text,
        headers,
        body,
    })
}

fn is_forbidden_request_header(name: &HeaderName) -> bool {
    let name = name.as_str();
    name == "host"
        || name == CONTENT_LENGTH.as_str()
        || name == USER_AGENT.as_str()
        || name == "accept-encoding"
        || name == "connection"
        || name == "cookie"
        || name == "origin"
        || name == "referer"
        || name == "set-cookie"
        || name == "te"
        || name == "trailer"
        || name == "transfer-encoding"
        || name == "upgrade"
        || name.starts_with("proxy-")
        || name.starts_with("sec-")
}

/// Fetch a URL and return the body as a UTF-8 string.
pub fn fetch_string(url: &str) -> Result<String, FetchError> {
    let bytes = fetch_bytes(url)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[allow(dead_code)]
pub fn fetch_string_with_method(
    url: &str,
    method: &str,
    body: Option<&str>,
) -> Result<String, FetchError> {
    let bytes = fetch_bytes_with_method(url, method, body)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderName;

    use super::is_forbidden_request_header;

    #[test]
    fn script_request_headers_respect_the_browser_security_boundary() {
        for allowed in [
            "content-type",
            "x-youtube-client-name",
            "x-youtube-client-version",
            "x-goog-visitor-id",
            "authorization",
        ] {
            let name = HeaderName::from_bytes(allowed.as_bytes()).unwrap();
            assert!(!is_forbidden_request_header(&name), "{allowed}");
        }
        for forbidden in [
            "host",
            "content-length",
            "cookie",
            "origin",
            "referer",
            "sec-fetch-site",
            "user-agent",
        ] {
            let name = HeaderName::from_bytes(forbidden.as_bytes()).unwrap();
            assert!(is_forbidden_request_header(&name), "{forbidden}");
        }
    }
}
