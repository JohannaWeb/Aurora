pub(super) fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub(super) fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .or_else(|| bytes.windows(2).position(|window| window == b"\n\n"))
}

pub(super) fn strip_header_separator(bytes: &[u8]) -> &[u8] {
    if let Some(stripped) = bytes.strip_prefix(b"\r\n\r\n") {
        stripped
    } else if let Some(stripped) = bytes.strip_prefix(b"\n\n") {
        stripped
    } else {
        bytes
    }
}
