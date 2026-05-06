use flate2::read::GzDecoder;
use std::io::Read;

use super::headers::header_value;
use super::http::send_request;
use super::resolve_relative_url;
use super::url::ParsedUrl;
use super::FetchError;

pub(super) fn fetch_with_redirects(
    url: &str,
    remaining_redirects: usize,
) -> Result<String, FetchError> {
    let body = fetch_body_with_redirects(url, remaining_redirects)?;
    Ok(String::from_utf8_lossy(&body).to_string())
}

pub(super) fn fetch_bytes_with_redirects(
    url: &str,
    remaining_redirects: usize,
) -> Result<Vec<u8>, FetchError> {
    fetch_body_with_redirects(url, remaining_redirects)
}

fn fetch_body_with_redirects(url: &str, remaining_redirects: usize) -> Result<Vec<u8>, FetchError> {
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
        return fetch_body_with_redirects(&next_url, remaining_redirects - 1);
    }

    if response.status_code != 200 {
        return Err(FetchError::InvalidResponse(format!(
            "HTTP {}",
            response.status_code
        )));
    }

    decompress_body_if_needed(response.body, &response.headers)
}

fn decompress_body_if_needed(
    mut body: Vec<u8>,
    headers: &[(String, String)],
) -> Result<Vec<u8>, FetchError> {
    if let Some(encoding) = header_value(headers, "content-encoding") {
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

fn is_redirect(status_code: u16) -> bool {
    matches!(status_code, 301 | 302 | 303 | 307 | 308)
}
