use rustls::pki_types::ServerName;
use rustls::{ClientConnection, StreamOwned};
use std::io::{Read, Write};
use std::net::TcpStream;

use super::chunked::decode_chunked_body;
use super::headers::{find_header_end, header_value, strip_header_separator};
use super::tls::tls_config;
use super::url::{ParsedUrl, Scheme};
use super::FetchError;

#[derive(Debug)]
pub(super) struct HttpResponse {
    pub(super) status_code: u16,
    pub(super) reason_phrase: String,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
}

pub(super) fn send_request(url: &ParsedUrl) -> Result<HttpResponse, FetchError> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Aurora/0.1\r\nAccept: text/html, text/css, */*\r\nAccept-Encoding: gzip, identity\r\nConnection: close\r\n\r\n",
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
    if let Err(e) = reader.read_to_end(&mut response) {
        if e.kind() != std::io::ErrorKind::UnexpectedEof {
            return Err(FetchError::Io(e));
        }
    }

    if response.is_empty() {
        return Err(FetchError::InvalidResponse("empty response".to_string()));
    }

    HttpResponse::parse(&response)
}

impl HttpResponse {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, FetchError> {
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
        } else if let Some(length) =
            header_value(&headers, "content-length").and_then(|value| value.parse::<usize>().ok())
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

    pub(super) fn header(&self, name: &str) -> Option<&str> {
        header_value(&self.headers, name)
    }
}
