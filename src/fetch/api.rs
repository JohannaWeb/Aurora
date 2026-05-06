use opus::domain::Identity;

use super::capability::{require_file_access, require_network_access};
use super::redirects::{fetch_bytes_with_redirects, fetch_with_redirects};
use super::FetchError;

const MAX_REDIRECTS: usize = 5;

pub fn fetch_html(url: &str, identity: &Identity) -> Result<String, FetchError> {
    fetch_string(url, identity)
}

pub fn fetch_string(url: &str, identity: &Identity) -> Result<String, FetchError> {
    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        return std::fs::read_to_string(path).map_err(FetchError::Io);
    }

    require_network_access(identity)?;
    fetch_with_redirects(url, MAX_REDIRECTS)
}

pub fn fetch_bytes(url: &str, identity: &Identity) -> Result<Vec<u8>, FetchError> {
    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        return std::fs::read(path).map_err(FetchError::Io);
    }

    require_network_access(identity)?;
    fetch_bytes_with_redirects(url, MAX_REDIRECTS)
}
