// Import TLS server name parsing
// RUST FUNDAMENTAL: External crate imports look just like standard-library imports in code;
// Cargo is what decides where the crate actually comes from.
use rustls::pki_types::ServerName;
// Import TLS client configuration and connections
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
// Import trusted CA certificate roots for HTTPS validation
use webpki_roots::TLS_SERVER_ROOTS;
// Import path handling types
use std::path::{Path, PathBuf};
// Import Display/Formatter for error messages
use std::fmt::{self, Display, Formatter};
// Import Read and Write traits for socket I/O
// RUST FUNDAMENTAL: Traits like `Read` and `Write` define shared behavior that many concrete stream types can implement.
use std::io::{Read, Write};
// Import TCP stream for network connections
use std::net::TcpStream;
// Import Arc for shared pointer to TLS config
// RUST FUNDAMENTAL: `Arc<T>` is the thread-safe reference-counted pointer type.
use std::sync::Arc;

// Maximum number of HTTP redirects to follow before giving up
const MAX_REDIRECTS: usize = 5;

// Enum representing HTTP or HTTPS scheme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    // HTTP (unencrypted)
    Http,
    // HTTPS (TLS encrypted)
    Https,
}

// Parsed URL components for network requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    // HTTP or HTTPS scheme
    scheme: Scheme,
    // Hostname (domain)
    host: String,
    // Port number (80 for HTTP, 443 for HTTPS)
    port: u16,
    // Path and query string components
    path_and_query: String,
}

// Error types that can occur during fetching
// RUST FUNDAMENTAL: Rust usually models recoverable errors as data with `Result<T, E>`.
// An enum like `FetchError` gives the program a closed set of specific failure cases that callers can inspect explicitly.
#[derive(Debug)]
pub enum FetchError {
    // Unknown URL scheme (not http or https)
    // RUST FUNDAMENTAL: Variants can carry payloads, so this one stores the actual unsupported scheme string.
    // That gives error handlers more context than a bare yes/no failure.
    UnsupportedScheme(String),

    // Malformed URL string
    InvalidUrl(String),

    // I/O error (network socket)
    // RUST FUNDAMENTAL: It is common to wrap lower-level library errors inside a higher-level application error enum.
    // That keeps the public API focused while still preserving the original error information.
    Io(std::io::Error),

    // TLS/HTTPS error
    // RUST FUNDAMENTAL: A dedicated variant for TLS failures lets calling code distinguish transport/security problems
    // from parsing or HTTP protocol problems.
    Tls(rustls::Error),

    // Invalid HTTP response format
    InvalidResponse(String),

    // HTTP error status code with reason
    // RUST FUNDAMENTAL: Tuple variants can carry multiple values without naming individual fields.
    // They are useful when the meaning is obvious from position and the variant name.
    HttpStatus(u16, String),

    // Too many redirects encountered
    // RUST FUNDAMENTAL: A unit variant carries no extra payload.
    // It is the enum equivalent of saying "this exact condition happened, and no further data is needed".
    TooManyRedirects,
}

impl ParsedUrl {
    fn parse(url: &str) -> Result<Self, FetchError> {
        // RUST FUNDAMENTAL: Parsing code often works by progressively splitting an input string into smaller validated pieces.
        let (scheme, without_scheme) = if let Some(rest) = url.strip_prefix("http://") {
            (Scheme::Http, rest)
        } else if let Some(rest) = url.strip_prefix("https://") {
            (Scheme::Https, rest)
        } else {
            // RUST FUNDAMENTAL: Returning `Err(...)` exits the function early with an explicit failure value.
            let scheme = url.split("://").next().unwrap_or(url).to_string();
            return Err(FetchError::UnsupportedScheme(scheme));
        };

        if without_scheme.is_empty() {
            return Err(FetchError::InvalidUrl(url.to_string()));
        }

        let (authority, path_and_query) =
            if let Some((authority, rest)) = without_scheme.split_once('/') {
                (authority, format!("/{}", rest))
            } else {
                (without_scheme, "/".to_string())
            };

        if authority.is_empty() {
            return Err(FetchError::InvalidUrl(url.to_string()));
        }

        let default_port = match scheme {
            Scheme::Http => 80,
            Scheme::Https => 443,
        };

        let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
            if host.is_empty() {
                return Err(FetchError::InvalidUrl(url.to_string()));
            }
            let port = port
                .parse::<u16>()
                // RUST FUNDAMENTAL: `.map_err(...)` transforms one error type into another while keeping the success type unchanged.
                .map_err(|_| FetchError::InvalidUrl(url.to_string()))?;
            (host.to_string(), port)
        } else {
            (authority.to_string(), default_port)
        };

        let parsed = Self {
            scheme,
            host,
            port,
            path_and_query,
        };

        parsed.validate()?;
        // RUST FUNDAMENTAL: `?` on a `Result` means "if this is `Err`, return that error now; otherwise continue with the success value".
        Ok(parsed)
    }

    fn authority(&self) -> String {
        let default_port = match self.scheme {
            Scheme::Http => 80,
            Scheme::Https => 443,
        };

        if self.port == default_port {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn scheme_prefix(&self) -> &'static str {
        match self.scheme {
            Scheme::Http => "http://",
            Scheme::Https => "https://",
        }
    }

    fn validate(&self) -> Result<(), FetchError> {
        // RUST FUNDAMENTAL: `()` is the unit type, so `Result<(), E>` means "success carries no extra data, only the fact of success".
        if self.host.is_empty()
            || self
                .host
                .chars()
                .any(|ch| ch.is_ascii_control() || ch.is_ascii_whitespace())
            || self.path_and_query.chars().any(|ch| ch.is_ascii_control())
        {
            return Err(FetchError::InvalidUrl(format!(
                "{}{}",
                self.authority(),
                self.path_and_query
            )));
        }

        Ok(())
    }
}

impl Display for FetchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // RUST FUNDAMENTAL: Implementing `Display` gives a type human-readable `{}` formatting.
        match self {
            FetchError::UnsupportedScheme(scheme) => {
                write!(
                    f,
                    "unsupported URL scheme: {scheme} (only http:// and https:// are supported)"
                )
            }
            FetchError::InvalidUrl(url) => write!(f, "invalid URL: {url}"),
            FetchError::Io(error) => write!(f, "network error: {error}"),
            FetchError::Tls(error) => write!(f, "TLS error: {error}"),
            FetchError::InvalidResponse(message) => write!(f, "invalid HTTP response: {message}"),
            FetchError::HttpStatus(code, reason) => write!(f, "HTTP {code} {reason}"),
            FetchError::TooManyRedirects => write!(f, "too many redirects"),
        }
    }
}

impl From<std::io::Error> for FetchError {
    fn from(value: std::io::Error) -> Self {
        // RUST FUNDAMENTAL: `From` implementations enable ergonomic conversion with `.into()` and are also used by `?`
        // when a lower-level error needs to become a higher-level one.
        Self::Io(value)
    }
}

impl From<rustls::Error> for FetchError {
    fn from(value: rustls::Error) -> Self {
        Self::Tls(value)
    }
}

use flate2::read::GzDecoder;
use opus::domain::{Capability, Identity};

pub fn fetch_html(url: &str, identity: &Identity) -> Result<String, FetchError> {
    // RUST FUNDAMENTAL: Small wrapper functions are useful when two public APIs share the same implementation today
    // but may diverge semantically later.
    fetch_string(url, identity)
}

pub fn fetch_string(url: &str, identity: &Identity) -> Result<String, FetchError> {
    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        // RUST FUNDAMENTAL: `map_err(FetchError::Io)` converts a standard I/O result into this module's error type in one expression.
        return std::fs::read_to_string(path).map_err(FetchError::Io);
    }

    if !identity
        .default_capabilities
        .contains(&Capability::NetworkAccess)
    {
        return Err(FetchError::InvalidUrl(format!(
            "Identity {} lacks network.access capability",
            identity.did
        )));
    }
    fetch_with_redirects(url, MAX_REDIRECTS)
}

pub fn fetch_bytes(url: &str, identity: &Identity) -> Result<Vec<u8>, FetchError> {
    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        return std::fs::read(path).map_err(FetchError::Io);
    }

    if !identity
        .default_capabilities
        .contains(&Capability::NetworkAccess)
    {
        return Err(FetchError::InvalidUrl(format!(
            "Identity {} lacks network.access capability",
            identity.did
        )));
    }
    fetch_bytes_with_redirects(url, MAX_REDIRECTS)
}

fn fetch_with_redirects(url: &str, remaining_redirects: usize) -> Result<String, FetchError> {
    let parsed = ParsedUrl::parse(url)?;
    let response = send_request(&parsed)?;

    if is_redirect(response.status_code) {
        // RUST FUNDAMENTAL: Recursive retry helpers like this are a clean way to model bounded redirect chains.
        if remaining_redirects == 0 {
            return Err(FetchError::InvalidResponse(
                "too many redirects".to_string(),
            ));
        }
        let location = header_value(&response.headers, "location")
            .ok_or_else(|| FetchError::InvalidResponse("missing location header".to_string()))?;
        let next_url = resolve_relative_url(url, location)?;
        return fetch_with_redirects(&next_url, remaining_redirects - 1);
    }

    if response.status_code != 200 {
        return Err(FetchError::InvalidResponse(format!(
            "HTTP {}",
            response.status_code
        )));
    }

    // Handle compression
    let mut body = response.body;
    if let Some(encoding) = header_value(&response.headers, "content-encoding") {
        if encoding.eq_ignore_ascii_case("gzip") {
            // RUST FUNDAMENTAL: `&body[..]` borrows the whole vector as a byte slice, which is what stream-style decoders usually consume.
            let mut decoder = GzDecoder::new(&body[..]);
            let mut decoded = Vec::new();
            if decoder.read_to_end(&mut decoded).is_ok() {
                body = decoded;
            }
        }
    }

    Ok(String::from_utf8_lossy(&body).to_string())
}

fn fetch_bytes_with_redirects(
    url: &str,
    remaining_redirects: usize,
) -> Result<Vec<u8>, FetchError> {
    let parsed = ParsedUrl::parse(url)?;
    let response = send_request(&parsed)?;

    if is_redirect(response.status_code) {
        if remaining_redirects == 0 {
            return Err(FetchError::InvalidResponse(
                "too many redirects".to_string(),
            ));
        }
        let location = header_value(&response.headers, "location")
            .ok_or_else(|| FetchError::InvalidResponse("missing location header".to_string()))?;
        let next_url = resolve_relative_url(url, location)?;
        return fetch_bytes_with_redirects(&next_url, remaining_redirects - 1);
    }

    if response.status_code != 200 {
        return Err(FetchError::InvalidResponse(format!(
            "HTTP {}",
            response.status_code
        )));
    }

    // Handle compression
    let mut body = response.body;
    if let Some(encoding) = header_value(&response.headers, "content-encoding") {
        if encoding.eq_ignore_ascii_case("gzip") {
            let mut decoder = GzDecoder::new(&body[..]);
            let mut decoded = Vec::new();
            if decoder.read_to_end(&mut decoded).is_ok() {
                body = decoded;
            }
        }
    }

    Ok(body)
}

pub fn resolve_relative_url(base: &str, relative: &str) -> Result<String, FetchError> {
    // RUST FUNDAMENTAL: Early-return branches keep the special cases close to the top and simplify the remaining "normal path".
    if let Some(base_path) = base.strip_prefix("file://") {
        return resolve_relative_file_url(base_path, relative);
    }

    if relative.starts_with("http://") || relative.starts_with("https://") {
        return Ok(relative.to_string());
    }

    let base_parsed = ParsedUrl::parse(base)?;

    if relative.starts_with("//") {
        return Ok(format!(
            "{}{}",
            base_parsed.scheme_prefix(),
            relative.trim_start_matches("//")
        ));
    }

    if relative.starts_with('/') {
        return Ok(format!(
            "{}{}{}",
            base_parsed.scheme_prefix(),
            base_parsed.authority(),
            relative
        ));
    }

    let base_dir = match base_parsed.path_and_query.rsplit_once('/') {
        Some((prefix, _)) if !prefix.is_empty() => prefix,
        _ => "",
    };

    Ok(format!(
        "{}{}{}/{}",
        base_parsed.scheme_prefix(),
        base_parsed.authority(),
        base_dir,
        relative
    ))
}

fn resolve_relative_file_url(base_path: &str, relative: &str) -> Result<String, FetchError> {
    if relative.starts_with("file://") {
        return Ok(relative.to_string());
    }

    let base = Path::new(base_path);
    let resolved = if relative.starts_with('/') {
        PathBuf::from(relative)
    } else {
        let parent = if base.is_dir() {
            base
        } else {
            base.parent().unwrap_or_else(|| Path::new("/"))
        };
        parent.join(relative)
    };

    let normalized = normalize_path(resolved);
    let absolute = if normalized.is_absolute() {
        normalized
    } else {
        // RUST FUNDAMENTAL: Standard-library functions that touch the environment or filesystem often return `Result`
        // because many OS-level operations can fail.
        std::env::current_dir()
            .map_err(FetchError::Io)?
            .join(normalized)
    };

    Ok(format!("file://{}", absolute.display()))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    // RUST FUNDAMENTAL: Iterating over `path.components()` yields semantic path pieces like `CurDir`, `ParentDir`, and normal segments.
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn send_request(url: &ParsedUrl) -> Result<HttpResponse, FetchError> {
    // RUST FUNDAMENTAL: `format!` is useful for request construction because it can interpolate numbers, strings,
    // and helper-method results into one owned request buffer.
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Aurora/0.1\r\nAccept: text/html, text/css, */*\r\nAccept-Encoding: gzip, identity\r\nConnection: close\r\n\r\n",
        url.path_and_query,
        url.authority(),
    );

    match url.scheme {
        Scheme::Http => {
            let mut stream = TcpStream::connect(url.socket_addr())?;
            // RUST FUNDAMENTAL: Methods like `write_all(...)` come from the `Write` trait, not from `TcpStream` specifically.
            stream.write_all(request.as_bytes())?;
            read_response_bytes(&mut stream)
        }
        Scheme::Https => {
            let stream = TcpStream::connect(url.socket_addr())?;
            let config = tls_config();
            let server_name = ServerName::try_from(url.host.clone())
                .map_err(|_| FetchError::InvalidUrl(url.host.clone()))?;
            let connection = ClientConnection::new(config, server_name)?;
            // RUST FUNDAMENTAL: `StreamOwned` is an adapter that combines a TLS state machine and an underlying transport stream
            // into one object that implements read/write operations.
            let mut tls_stream = StreamOwned::new(connection, stream);
            tls_stream.write_all(request.as_bytes())?;
            read_response_bytes(&mut tls_stream)
        }
    }
}

fn read_response_bytes<R: Read>(reader: &mut R) -> Result<HttpResponse, FetchError> {
    // RUST FUNDAMENTAL: `R: Read` is a generic trait bound, meaning this function works with any reader type
    // that implements the `Read` trait, not just one concrete stream type.
    let mut response = Vec::new();
    if let Err(e) = reader.read_to_end(&mut response) {
        if e.kind() != std::io::ErrorKind::UnexpectedEof {
            return Err(FetchError::Io(e));
        }
    }

    if response.is_empty() {
        return Err(FetchError::InvalidResponse("empty response".to_string()));
    }

    HttpResponse::parse(&response)
}

fn tls_config() -> Arc<ClientConfig> {
    // RUST FUNDAMENTAL: Returning `Arc<ClientConfig>` lets multiple TLS connections share one immutable config value cheaply.
    let root_store = RootCertStore::from_iter(TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    )
}

fn require_file_access(identity: &Identity) -> Result<(), FetchError> {
    // RUST FUNDAMENTAL: Capability checks like this are ordinary boolean conditions in Rust;
    // the type system does not enforce them automatically, so explicit guard code matters.
    if identity
        .default_capabilities
        .contains(&Capability::ReadWorkspace)
    {
        Ok(())
    } else {
        Err(FetchError::InvalidUrl(format!(
            "Identity {} lacks workspace.read capability",
            identity.did
        )))
    }
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn parse(bytes: &[u8]) -> Result<Self, FetchError> {
        // RUST FUNDAMENTAL: Parsing a byte slice instead of a string first lets the code handle raw protocol framing
        // before deciding how to decode text portions.
        let header_end = find_header_end(bytes)
            .ok_or_else(|| FetchError::InvalidResponse("missing header terminator".to_string()))?;
        let (head, body_bytes) = bytes.split_at(header_end);
        let body_bytes = strip_header_separator(body_bytes);
        let head_text = String::from_utf8_lossy(head);
        let mut lines = head_text.lines();
        let status_line = lines
            .next()
            .ok_or_else(|| FetchError::InvalidResponse("missing status line".to_string()))?;
        let mut status_parts = status_line.splitn(3, ' ');
        let _http_version = status_parts
            .next()
            .ok_or_else(|| FetchError::InvalidResponse("missing HTTP version".to_string()))?;
        let status_code = status_parts
            .next()
            .ok_or_else(|| FetchError::InvalidResponse("missing status code".to_string()))?
            .parse::<u16>()
            .map_err(|_| FetchError::InvalidResponse("invalid status code".to_string()))?;
        let reason_phrase = status_parts.next().unwrap_or("").trim().to_string();

        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
            })
            .collect::<Vec<_>>();
        // RUST FUNDAMENTAL: Lowercasing header names once here makes later header lookup simpler and case-insensitive.

        let body = if header_value(&headers, "transfer-encoding")
            .map(|value| value.eq_ignore_ascii_case("chunked"))
            .unwrap_or(false)
        {
            decode_chunked_body(body_bytes)?
        } else if let Some(length) =
            header_value(&headers, "content-length").and_then(|value| value.parse::<usize>().ok())
        {
            body_bytes[..body_bytes.len().min(length)].to_vec()
        } else {
            body_bytes.to_vec()
        };

        Ok(Self {
            status_code,
            reason_phrase,
            headers,
            body,
        })
    }

    fn header(&self, name: &str) -> Option<&str> {
        header_value(&self.headers, name)
    }
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    // RUST FUNDAMENTAL: The explicit lifetime `'a` ties the returned `&str` to the lifetime of the input headers slice.
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    // RUST FUNDAMENTAL: `.windows(n)` walks overlapping fixed-size slices across a larger slice,
    // which is handy for protocol delimiter searches.
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .or_else(|| bytes.windows(2).position(|window| window == b"\n\n"))
}

fn strip_header_separator(bytes: &[u8]) -> &[u8] {
    // RUST FUNDAMENTAL: Returning a subslice here borrows from the original response buffer with zero copying.
    if let Some(stripped) = bytes.strip_prefix(b"\r\n\r\n") {
        stripped
    } else if let Some(stripped) = bytes.strip_prefix(b"\n\n") {
        stripped
    } else {
        bytes
    }
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, FetchError> {
    let mut cursor = 0;
    let mut decoded = Vec::new();

    loop {
        // RUST FUNDAMENTAL: Cursor-based parsing is a common low-level technique for byte protocols.
        let size_end = find_crlf(body, cursor)
            .ok_or_else(|| FetchError::InvalidResponse("unterminated chunk size".to_string()))?;
        let size_line = std::str::from_utf8(&body[cursor..size_end])
            .map_err(|_| FetchError::InvalidResponse("non-utf8 chunk size".to_string()))?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16)
            // RUST FUNDAMENTAL: `from_str_radix` parses numeric text in a caller-specified base, here hexadecimal.
            .map_err(|_| FetchError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = size_end + 2;

        if size == 0 {
            // RUST FUNDAMENTAL: A zero-sized chunk is the protocol marker for the end of a chunked HTTP body.
            break;
        }

        let chunk_end = cursor + size;
        if chunk_end > body.len() {
            return Err(FetchError::InvalidResponse(
                "truncated chunk body".to_string(),
            ));
        }
        decoded.extend_from_slice(&body[cursor..chunk_end]);
        // RUST FUNDAMENTAL: `extend_from_slice` appends bytes efficiently from one slice into a `Vec<u8>`.
        cursor = chunk_end;

        if body.get(cursor..cursor + 2) != Some(b"\r\n".as_slice()) {
            return Err(FetchError::InvalidResponse(
                "missing chunk terminator".to_string(),
            ));
        }
        cursor += 2;
    }

    Ok(decoded)
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    // RUST FUNDAMENTAL: Returning `start + offset` converts a relative position in the subslice back into an absolute cursor position.
    bytes[start..]
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

fn is_redirect(status_code: u16) -> bool {
    matches!(status_code, 301 | 302 | 303 | 307 | 308)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_chunked_body, resolve_relative_url, FetchError, HttpResponse, ParsedUrl, Scheme,
    };

    #[test]
    fn parses_http_urls() {
        // RUST FUNDAMENTAL: Tests often use `unwrap()` because a failure should crash the test immediately and loudly.
        let parsed = ParsedUrl::parse("http://example.com:8080/cats?name=loaf").unwrap();
        assert_eq!(parsed.scheme, Scheme::Http);
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 8080);
        assert_eq!(parsed.path_and_query, "/cats?name=loaf");
    }

    #[test]
    fn parses_https_urls() {
        let parsed = ParsedUrl::parse("https://example.com/cats").unwrap();
        assert_eq!(parsed.scheme, Scheme::Https);
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.path_and_query, "/cats");
    }

    #[test]
    fn rejects_non_http_urls() {
        // RUST FUNDAMENTAL: Matching on the error enum in tests verifies not just that something failed,
        // but that it failed for the expected reason.
        match ParsedUrl::parse("ftp://example.com") {
            Err(FetchError::UnsupportedScheme(scheme)) => assert_eq!(scheme, "ftp"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn decodes_chunked_responses() {
        let body = b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        let decoded = decode_chunked_body(body).unwrap();
        assert_eq!(decoded, b"Wikipedia");
    }

    #[test]
    fn parses_http_response_body() {
        let response = HttpResponse::parse(
            b"HTTP/1.1 200 OK\r\nContent-Length: 31\r\nContent-Type: text/html\r\n\r\n<html><body>cats</body></html>",
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(
            String::from_utf8_lossy(&response.body),
            "<html><body>cats</body></html>"
        );
    }

    #[test]
    fn resolves_redirect_targets() {
        let base = "https://example.com/cats/start";

        assert_eq!(
            resolve_relative_url(base, "/photos").unwrap(),
            "https://example.com/photos"
        );
        assert_eq!(
            resolve_relative_url(base, "loaf").unwrap(),
            "https://example.com/cats/loaf"
        );
        assert_eq!(
            resolve_relative_url(base, "//cdn.example.com/cat.jpg").unwrap(),
            "https://cdn.example.com/cat.jpg"
        );
        assert_eq!(
            resolve_relative_url(base, "http://other.test/zoom").unwrap(),
            "http://other.test/zoom"
        );
    }

    #[test]
    fn resolves_relative_file_paths() {
        let base = "file:///tmp/aurora/fixtures/google-homepage/index.html";

        assert_eq!(
            resolve_relative_url(base, "styles.css").unwrap(),
            "file:///tmp/aurora/fixtures/google-homepage/styles.css"
        );
    }

    #[test]
    fn parses_chunked_http_response_body() {
        let response = HttpResponse::parse(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n11\r\n<html>cats</html>\r\n0\r\n\r\n",
        )
        .unwrap();

        assert_eq!(String::from_utf8_lossy(&response.body), "<html>cats</html>");
    }
}
