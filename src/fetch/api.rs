use crate::identity::Identity;

use super::capability::{require_file_access, require_network_access};
use super::FetchError;

pub fn fetch_html(url: &str, identity: &Identity) -> Result<String, FetchError> {
    fetch_string(url, identity)
}

pub fn fetch_string(url: &str, identity: &Identity) -> Result<String, FetchError> {
    if url.starts_with("data:") {
        let bytes = super::data_url::decode(url)?;
        return String::from_utf8(bytes)
            .map_err(|_| FetchError::InvalidResponse("invalid UTF-8 data URL".to_string()));
    }

    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        return std::fs::read_to_string(path).map_err(FetchError::Io);
    }

    require_network_access(identity)?;
    super::http::fetch_string(url)
}

pub fn fetch_bytes(url: &str, identity: &Identity) -> Result<Vec<u8>, FetchError> {
    if url.starts_with("data:") {
        return super::data_url::decode(url);
    }

    if let Some(path) = url.strip_prefix("file://") {
        require_file_access(identity)?;
        return std::fs::read(path).map_err(FetchError::Io);
    }

    require_network_access(identity)?;
    super::http::fetch_bytes(url)
}
