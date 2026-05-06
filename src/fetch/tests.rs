use super::chunked::decode_chunked_body;
use super::http::HttpResponse;
use super::resolve_relative_url;
use super::url::{ParsedUrl, Scheme};
use super::FetchError;

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
