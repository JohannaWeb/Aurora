use super::fetch_string;
use super::resolve_relative_url;
use super::url::{ParsedUrl, Scheme};
use super::FetchError;
use crate::identity::{Capability, Identity, IdentityKind};

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
    assert_eq!(
        resolve_relative_url(base, "data:text/plain;base64,Y2F0cw==").unwrap(),
        "data:text/plain;base64,Y2F0cw=="
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
fn fetches_data_url_strings_without_network() {
    let identity = Identity::new(
        "did:test:fetch",
        "Fetch Test",
        IdentityKind::Human,
        [Capability::ReadWorkspace],
    );

    assert_eq!(
        fetch_string("data:text/plain;base64,Y2F0cw==", &identity).unwrap(),
        "cats"
    );
    assert_eq!(
        fetch_string("data:text/plain,hello%20aurora", &identity).unwrap(),
        "hello aurora"
    );
}

