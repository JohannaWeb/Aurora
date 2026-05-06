pub(super) fn find_var_content(result: &str, start: usize) -> Option<(usize, &str)> {
    let mut paren_depth = 1;
    for (i, ch) in result[start + 4..].chars().enumerate() {
        if ch == '(' {
            paren_depth += 1;
        } else if ch == ')' {
            paren_depth -= 1;
            if paren_depth == 0 {
                let end_pos = start + 4 + i;
                return Some((end_pos, &result[start + 4..end_pos]));
            }
        }
    }
    None
}

pub(super) fn parse_var_content(var_content: &str) -> (String, Option<String>) {
    if let Some(comma_idx) = var_content.find(',') {
        (
            var_content[..comma_idx].trim().to_string(),
            Some(var_content[comma_idx + 1..].trim().to_string()),
        )
    } else {
        (var_content.trim().to_string(), None)
    }
}
