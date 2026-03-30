use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::fmt::{self, Display, Formatter};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    Http,
    Https,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    scheme: Scheme,
    host: String,
    port: u16,
    path_and_query: String,
}

#[derive(Debug)]
pub enum FetchError {
    UnsupportedScheme(String),
    InvalidUrl(String),
    Io(std::io::Error),
    Tls(rustls::Error),
    InvalidResponse(String),
    HttpStatus(u16, String),
    TooManyRedirects,
}

impl ParsedUrl {
    fn parse(url: &str) -> Result<Self, FetchError> {
        let (scheme, without_scheme) = if let Some(rest) = url.strip_prefix("http://") {
            (Scheme::Http, rest)
        } else if let Some(rest) = url.strip_prefix("https://") {
            (Scheme::Https, rest)
        } else {
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
                .map_err(|_| FetchError::InvalidUrl(url.to_string()))?;
            (host.to_string(), port)
        } else {
            (authority.to_string(), default_port)
        };

        Ok(Self {
            scheme,
            host,
            port,
            path_and_query,
        })
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
}

impl Display for FetchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::UnsupportedScheme(scheme) => {
                write!(f, "unsupported URL scheme: {scheme} (only http:// and https:// are supported)")
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
        Self::Io(value)
    }
}

impl From<rustls::Error> for FetchError {
    fn from(value: rustls::Error) -> Self {
        Self::Tls(value)
    }
}

pub fn fetch_html(url: &str) -> Result<String, FetchError> {
    fetch_html_with_redirects(url, MAX_REDIRECTS)
}

fn fetch_html_with_redirects(url: &str, remaining_redirects: usize) -> Result<String, FetchError> {
    let parsed = ParsedUrl::parse(url)?;
    let response = send_request(&parsed)?;

    if is_redirect(response.status_code) {
        if remaining_redirects == 0 {
            return Err(FetchError::TooManyRedirects);
        }
        let Some(location) = response.header("location") else {
            return Err(FetchError::InvalidResponse(
                "redirect response missing Location header".to_string(),
            ));
        };
        let next_url = resolve_redirect_url(&parsed, location);
        return fetch_html_with_redirects(&next_url, remaining_redirects - 1);
    }

    if !(200..300).contains(&response.status_code) {
        return Err(FetchError::HttpStatus(
            response.status_code,
            response.reason_phrase,
        ));
    }

    Ok(String::from_utf8_lossy(&response.body).into_owned())
}

fn send_request(url: &ParsedUrl) -> Result<HttpResponse, FetchError> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Aurora/0.1\r\nAccept: text/html, */*\r\nConnection: close\r\n\r\n",
        url.path_and_query,
        url.authority(),
    );

    match url.scheme {
        Scheme::Http => {
            let mut stream = TcpStream::connect(url.socket_addr())?;
            stream.write_all(request.as_bytes())?;
            read_response_bytes(&mut stream)
        }
        Scheme::Https => {
            let stream = TcpStream::connect(url.socket_addr())?;
            let config = tls_config();
            let server_name = ServerName::try_from(url.host.clone())
                .map_err(|_| FetchError::InvalidUrl(url.host.clone()))?;
            let connection = ClientConnection::new(config, server_name)?;
            let mut tls_stream = StreamOwned::new(connection, stream);
            tls_stream.write_all(request.as_bytes())?;
            read_response_bytes(&mut tls_stream)
        }
    }
}

fn read_response_bytes<R: Read>(reader: &mut R) -> Result<HttpResponse, FetchError> {
    let mut response = Vec::new();
    reader.read_to_end(&mut response)?;
    HttpResponse::parse(&response)
}

fn tls_config() -> Arc<ClientConfig> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };

    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
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

        let body = if header_value(&headers, "transfer-encoding")
            .map(|value| value.eq_ignore_ascii_case("chunked"))
            .unwrap_or(false)
        {
            decode_chunked_body(body_bytes)?
        } else if let Some(length) = header_value(&headers, "content-length")
            .and_then(|value| value.parse::<usize>().ok())
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
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .or_else(|| bytes.windows(2).position(|window| window == b"\n\n"))
}

fn strip_header_separator(bytes: &[u8]) -> &[u8] {
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
        let size_end = find_crlf(body, cursor)
            .ok_or_else(|| FetchError::InvalidResponse("unterminated chunk size".to_string()))?;
        let size_line = std::str::from_utf8(&body[cursor..size_end])
            .map_err(|_| FetchError::InvalidResponse("non-utf8 chunk size".to_string()))?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| FetchError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = size_end + 2;

        if size == 0 {
            break;
        }

        let chunk_end = cursor + size;
        if chunk_end > body.len() {
            return Err(FetchError::InvalidResponse("truncated chunk body".to_string()));
        }
        decoded.extend_from_slice(&body[cursor..chunk_end]);
        cursor = chunk_end;

        if body.get(cursor..cursor + 2) != Some(b"\r\n".as_slice()) {
            return Err(FetchError::InvalidResponse("missing chunk terminator".to_string()));
        }
        cursor += 2;
    }

    Ok(decoded)
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes[start..]
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

fn is_redirect(status_code: u16) -> bool {
    matches!(status_code, 301 | 302 | 303 | 307 | 308)
}

fn resolve_redirect_url(base: &ParsedUrl, location: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        return location.to_string();
    }

    if location.starts_with("//") {
        return format!("{}{}", base.scheme_prefix(), location.trim_start_matches("//"));
    }

    if location.starts_with('/') {
        return format!("{}{}{}", base.scheme_prefix(), base.authority(), location);
    }

    let base_dir = match base.path_and_query.rsplit_once('/') {
        Some((prefix, _)) if !prefix.is_empty() => prefix,
        _ => "",
    };

    format!(
        "{}{}{}/{}",
        base.scheme_prefix(),
        base.authority(),
        base_dir,
        location
    )
}

#[cfg(test)]
mod tests {
    use super::{
        decode_chunked_body, resolve_redirect_url, FetchError, HttpResponse, ParsedUrl, Scheme,
    };

    #[test]
    fn parses_http_urls() {
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
        assert_eq!(String::from_utf8_lossy(&response.body), "<html><body>cats</body></html>");
    }

    #[test]
    fn resolves_redirect_targets() {
        let base = ParsedUrl::parse("https://example.com/cats/start").unwrap();

        assert_eq!(
            resolve_redirect_url(&base, "/photos"),
            "https://example.com/photos"
        );
        assert_eq!(
            resolve_redirect_url(&base, "loaf"),
            "https://example.com/cats/loaf"
        );
        assert_eq!(
            resolve_redirect_url(&base, "//cdn.example.com/cat.jpg"),
            "https://cdn.example.com/cat.jpg"
        );
        assert_eq!(
            resolve_redirect_url(&base, "http://other.test/zoom"),
            "http://other.test/zoom"
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
