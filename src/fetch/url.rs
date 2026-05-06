use super::FetchError;

/// HTTP or HTTPS scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scheme {
    Http,
    Https,
}

/// Parsed URL components for network requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub(super) scheme: Scheme,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) path_and_query: String,
}

impl ParsedUrl {
    pub(super) fn parse(url: &str) -> Result<Self, FetchError> {
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

        let parsed = Self {
            scheme,
            host,
            port,
            path_and_query,
        };

        parsed.validate()?;
        Ok(parsed)
    }

    pub(super) fn authority(&self) -> String {
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

    pub(super) fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub(super) fn scheme_prefix(&self) -> &'static str {
        match self.scheme {
            Scheme::Http => "http://",
            Scheme::Https => "https://",
        }
    }

    fn validate(&self) -> Result<(), FetchError> {
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
