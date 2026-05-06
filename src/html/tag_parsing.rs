use std::collections::BTreeMap;

use super::tokens::TagToken;

pub(super) fn parse_open_tag(source: &str) -> TagToken {
    let mut chars = source.trim_end_matches('/').trim_end().chars().peekable();
    let mut tag_name = String::new();

    while let Some(ch) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        tag_name.push(*ch);
        chars.next();
    }

    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }

    let rest = chars.collect::<String>();
    TagToken {
        tag_name,
        attributes: parse_attributes(&rest),
    }
}

fn parse_attributes(source: &str) -> BTreeMap<String, String> {
    let mut attributes = BTreeMap::new();
    let chars = source.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '=' {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let name = chars[start..index].iter().collect::<String>();

        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        let value = if index < chars.len() && chars[index] == '=' {
            index += 1;
            parse_attribute_value(&chars, &mut index)
        } else {
            String::new()
        };

        attributes.insert(name, value);
    }

    attributes
}

fn parse_attribute_value(chars: &[char], index: &mut usize) -> String {
    while *index < chars.len() && chars[*index].is_whitespace() {
        *index += 1;
    }

    if *index >= chars.len() {
        String::new()
    } else if chars[*index] == '"' || chars[*index] == '\'' {
        parse_quoted_value(chars, index)
    } else {
        parse_unquoted_value(chars, index)
    }
}

fn parse_quoted_value(chars: &[char], index: &mut usize) -> String {
    let quote = chars[*index];
    *index += 1;
    let value_start = *index;

    while *index < chars.len() && chars[*index] != quote {
        *index += 1;
    }

    let value = chars[value_start..*index].iter().collect::<String>();
    if *index < chars.len() {
        *index += 1;
    }
    value
}

fn parse_unquoted_value(chars: &[char], index: &mut usize) -> String {
    let value_start = *index;
    while *index < chars.len() && !chars[*index].is_whitespace() {
        *index += 1;
    }
    chars[value_start..*index].iter().collect::<String>()
}
