pub(super) fn collapse_whitespace(input: &str) -> String {
    let mut result = String::new();
    let mut last_was_whitespace = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(ch);
            last_was_whitespace = false;
        }
    }

    result
}

pub(super) fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&copy;", "\u{00A9}")
        .replace("&reg;", "\u{00AE}")
        .replace("&trade;", "\u{2122}")
        .replace("&bull;", "\u{2022}")
        .replace("&middot;", "\u{00B7}")
        .replace("&ndash;", "\u{2013}")
        .replace("&mdash;", "\u{2014}")
}
