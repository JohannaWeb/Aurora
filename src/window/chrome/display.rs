pub(in crate::window) fn truncate_chrome_text(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

pub(in crate::window) fn chrome_display_url(url: &str) -> String {
    if url.contains("/fixtures/aurora-search/") {
        "https://aurora.sovereign/search".to_string()
    } else if url.contains("/fixtures/google-homepage/") {
        "https://google.com/search".to_string()
    } else if url.contains("/fixtures/demo/") {
        "aurora://fixture/demo".to_string()
    } else {
        url.to_string()
    }
}
