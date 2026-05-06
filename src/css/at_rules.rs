pub(super) fn strip_at_rules(
    source: &str,
    fetch_ctx: Option<(&str, &opus::domain::Identity)>,
    depth: u32,
) -> String {
    let mut result = String::with_capacity(source.len());
    let mut imports = String::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '@' {
            consume_at_rule(&mut chars, fetch_ctx, depth, &mut imports);
        } else {
            result.push(ch);
        }
    }

    if imports.is_empty() {
        result
    } else {
        imports + &result
    }
}

fn consume_at_rule<I>(
    chars: &mut std::iter::Peekable<I>,
    fetch_ctx: Option<(&str, &opus::domain::Identity)>,
    depth: u32,
    imports: &mut String,
) where
    I: Iterator<Item = char>,
{
    let mut keyword_buf = String::new();
    let mut found_brace = false;
    for c in chars.by_ref() {
        if c == '{' {
            found_brace = true;
            break;
        } else if c == ';' {
            break;
        } else {
            keyword_buf.push(c);
        }
    }

    if found_brace {
        skip_at_rule_block(chars);
    } else {
        fetch_import_if_needed(keyword_buf.trim_start(), fetch_ctx, depth, imports);
    }
}

fn skip_at_rule_block<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let mut depth_count = 1usize;
    for c in chars.by_ref() {
        match c {
            '{' => depth_count += 1,
            '}' => {
                depth_count -= 1;
                if depth_count == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

fn fetch_import_if_needed(
    keyword: &str,
    fetch_ctx: Option<(&str, &opus::domain::Identity)>,
    depth: u32,
    imports: &mut String,
) {
    if !keyword.to_ascii_lowercase().starts_with("import") || depth >= 3 {
        return;
    }

    let after_import = keyword["import".len()..].trim();
    let (Some(url), Some((base, identity))) = (extract_import_url(after_import), fetch_ctx) else {
        return;
    };

    if let Ok(resolved) = crate::fetch::resolve_relative_url(base, &url) {
        if let Ok(fetched) = crate::fetch::fetch_string(&resolved, identity) {
            let inner = strip_at_rules(&fetched, Some((&resolved, identity)), depth + 1);
            imports.push_str(&inner);
            imports.push('\n');
        }
    }
}

fn extract_import_url(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("url(") {
        let inner = rest.trim_end_matches(')').trim();
        let inner = inner.trim_matches('"').trim_matches('\'');
        return (!inner.is_empty()).then(|| inner.to_string());
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return Some(inner.to_string());
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Some(inner.to_string());
    }
    None
}
