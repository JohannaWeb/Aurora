use super::FetchError;

pub(super) fn decode(url: &str) -> Result<Vec<u8>, FetchError> {
    let Some(payload) = url.strip_prefix("data:") else {
        return Err(FetchError::InvalidUrl(url.to_string()));
    };
    let Some((metadata, data)) = payload.split_once(',') else {
        return Err(FetchError::InvalidUrl(url.to_string()));
    };

    if metadata
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        decode_base64(data)
    } else {
        percent_decode(data)
    }
}

fn percent_decode(value: &str) -> Result<Vec<u8>, FetchError> {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(FetchError::InvalidUrl(value.to_string()));
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Ok(out)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, FetchError> {
    let mut out = Vec::with_capacity(value.len() * 3 / 4);
    let mut chunk = [Some(0u8); 4];
    let mut len = 0;

    for byte in value.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        chunk[len] = base64_value(byte)?;
        len += 1;
        if len == 4 {
            push_base64_chunk(&chunk, &mut out)?;
            len = 0;
        }
    }
    if len != 0 {
        return Err(FetchError::InvalidUrl(value.to_string()));
    }
    Ok(out)
}

fn base64_value(byte: u8) -> Result<Option<u8>, FetchError> {
    match byte {
        b'A'..=b'Z' => Ok(Some(byte - b'A')),
        b'a'..=b'z' => Ok(Some(byte - b'a' + 26)),
        b'0'..=b'9' => Ok(Some(byte - b'0' + 52)),
        b'+' => Ok(Some(62)),
        b'/' => Ok(Some(63)),
        b'=' => Ok(None),
        _ => Err(FetchError::InvalidUrl((byte as char).to_string())),
    }
}

fn push_base64_chunk(chunk: &[Option<u8>; 4], out: &mut Vec<u8>) -> Result<(), FetchError> {
    let Some(first) = chunk[0] else {
        return Err(FetchError::InvalidUrl("data:".to_string()));
    };
    let Some(second) = chunk[1] else {
        return Err(FetchError::InvalidUrl("data:".to_string()));
    };
    let third = chunk[2].unwrap_or(0);
    let fourth = chunk[3].unwrap_or(0);
    let combined =
        ((first as u32) << 18) | ((second as u32) << 12) | ((third as u32) << 6) | fourth as u32;
    out.push((combined >> 16) as u8);
    if chunk[2].is_some() {
        out.push((combined >> 8) as u8);
    }
    if chunk[3].is_some() {
        out.push(combined as u8);
    }
    Ok(())
}

fn hex_value(byte: u8) -> Result<u8, FetchError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(FetchError::InvalidUrl((byte as char).to_string())),
    }
}
