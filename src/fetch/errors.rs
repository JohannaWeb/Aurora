use std::fmt::{self, Display, Formatter};

/// Error types that can occur during fetching.
#[derive(Debug)]
pub enum FetchError {
    /// Unknown URL scheme.
    UnsupportedScheme(String),
    /// Malformed URL string.
    InvalidUrl(String),
    /// I/O error.
    Io(std::io::Error),
    /// TLS/HTTPS error.
    Tls(rustls::Error),
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
        Self::Io(value)
    }
}

impl From<rustls::Error> for FetchError {
    fn from(value: rustls::Error) -> Self {
        Self::Tls(value)
    }
}
