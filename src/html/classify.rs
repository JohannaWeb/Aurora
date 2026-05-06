pub(super) fn is_raw_text_tag(tag_name: &str) -> bool {
    matches!(tag_name, "script" | "style")
}

pub(super) fn is_void_tag(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

pub(super) fn find_tag_end(source: &str) -> Option<usize> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut quote_char: Option<char> = None;

    while i < chars.len() {
        match (chars[i], quote_char) {
            ('"', None) => quote_char = Some('"'),
            ('"', Some('"')) => quote_char = None,
            ('\'', None) => quote_char = Some('\''),
            ('\'', Some('\'')) => quote_char = None,
            ('>', None) => return Some(i),
            _ => {}
        }
        i += 1;
    }

    None
}
