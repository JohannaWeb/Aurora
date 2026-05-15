use super::FetchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

#[derive(Debug, Clone)]
pub struct ParsedUrl {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

impl ParsedUrl {
    pub fn parse(url: &str) -> Result<Self, FetchError> {
        let (scheme_str, rest) = url
            .split_once("://")
            .ok_or_else(|| FetchError::InvalidUrl(format!("no scheme in URL: {url}")))?;

        let scheme = match scheme_str {
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            other => return Err(FetchError::UnsupportedScheme(other.to_string())),
        };

        let default_port = match scheme {
            Scheme::Http => 80,
            Scheme::Https => 443,
        };

        let (authority_str, path_and_query) = match rest.split_once('/') {
            Some((auth, path)) => (auth, format!("/{path}")),
            None => (rest, String::from("/")),
        };

        let (host, port) = if let Some((h, p)) = authority_str.split_once(':') {
            let port = p
                .parse::<u16>()
                .map_err(|_| FetchError::InvalidUrl(format!("invalid port in URL: {url}")))?;
            (h.to_string(), port)
        } else {
            (authority_str.to_string(), default_port)
        };

        Ok(ParsedUrl {
            scheme,
            host,
            port,
            path_and_query,
        })
    }

    pub fn scheme_prefix(&self) -> &'static str {
        match self.scheme {
            Scheme::Http => "http://",
            Scheme::Https => "https://",
        }
    }

    pub fn authority(&self) -> String {
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
}
