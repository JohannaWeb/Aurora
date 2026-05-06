use super::FetchError;

pub(super) fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, FetchError> {
    let mut cursor = 0;
    let mut decoded = Vec::new();

    loop {
        let size_end = find_crlf(body, cursor)
            .ok_or_else(|| FetchError::InvalidResponse("unterminated chunk size".to_string()))?;
        let size_line = std::str::from_utf8(&body[cursor..size_end])
            .map_err(|_| FetchError::InvalidResponse("non-utf8 chunk size".to_string()))?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| FetchError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = size_end + 2;

        if size == 0 {
            break;
        }

        let chunk_end = cursor + size;
        if chunk_end > body.len() {
            return Err(FetchError::InvalidResponse(
                "truncated chunk body".to_string(),
            ));
        }

        decoded.extend_from_slice(&body[cursor..chunk_end]);
        cursor = chunk_end;

        if body.get(cursor..cursor + 2) != Some(b"\r\n".as_slice()) {
            return Err(FetchError::InvalidResponse(
                "missing chunk terminator".to_string(),
            ));
        }
        cursor += 2;
    }

    Ok(decoded)
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes[start..]
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}
