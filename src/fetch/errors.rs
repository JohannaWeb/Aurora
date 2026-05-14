use std::fmt::{self, Display, Formatter};

/// Error types that can occur during fetching.
#[derive(Debug)]
pub enum FetchError {
    /// Unknown URL scheme.
    UnsupportedScheme(String),
    /// Malformed URL string.
    InvalidUrl(String),
    /// I/O error (file:// access).
    Io(std::io::Error),
    /// Network error from reqwest.
    Network(String),
    /// Invalid HTTP response format.
    InvalidResponse(String),
    /// HTTP error status code with reason.
    HttpStatus(u16, String),
    /// Too many redirects encountered.
    TooManyRedirects,
}

impl Display for FetchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::UnsupportedScheme(scheme) => {
                write!(f, "unsupported URL scheme: {scheme}")
            }
            FetchError::InvalidUrl(url) => write!(f, "invalid URL: {url}"),
            FetchError::Io(error) => write!(f, "I/O error: {error}"),
            FetchError::Network(msg) => write!(f, "network error: {msg}"),
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
