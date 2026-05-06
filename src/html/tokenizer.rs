use super::classify::{find_tag_end, is_raw_text_tag};
use super::tag_parsing::parse_open_tag;
use super::text::{collapse_whitespace, decode_entities};
use super::tokens::Token;

pub(super) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text_buffer = String::new();
    let mut index = 0;

    while index < source.len() {
        let rest = &source[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch == '<' {
            flush_text(&mut tokens, &mut text_buffer);

            let Some(tag_end_offset) = find_tag_end(rest) else {
                text_buffer.push(ch);
                index += ch.len_utf8();
                continue;
            };

            if rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
                index += tag_end_offset + 1;
                continue;
            }

            if rest.starts_with("<!--") {
                index = skip_comment(rest, index);
                continue;
            }

            let tag = rest[1..tag_end_offset].trim();
            index += tag_end_offset + 1;

            if let Some(stripped) = tag.strip_prefix('/') {
                tokens.push(Token::CloseTag(stripped.trim().to_string()));
            } else if !tag.is_empty() {
                let open_tag = parse_open_tag(tag);
                let raw_text_tag = is_raw_text_tag(&open_tag.tag_name);
                let tag_name = open_tag.tag_name.clone();
                tokens.push(Token::OpenTag(open_tag));

                if raw_text_tag {
                    index = consume_raw_text(source, index, &tag_name, &mut tokens);
                    if index >= source.len() {
                        break;
                    }
                }
            }
        } else {
            text_buffer.push(ch);
            index += ch.len_utf8();
        }
    }

    flush_text(&mut tokens, &mut text_buffer);
    tokens
}

fn flush_text(tokens: &mut Vec<Token>, text_buffer: &mut String) {
    if text_buffer.is_empty() {
        return;
    }

    let collapsed = collapse_whitespace(text_buffer);
    if !collapsed.trim().is_empty() {
        tokens.push(Token::Text(decode_entities(&collapsed)));
    }
    text_buffer.clear();
}

fn skip_comment(rest: &str, index: usize) -> usize {
    if let Some(comment_end) = rest.find("-->") {
        index + comment_end + 3
    } else {
        index + 4
    }
}

fn consume_raw_text(source: &str, index: usize, tag_name: &str, tokens: &mut Vec<Token>) -> usize {
    let close_tag = format!("</{tag_name}>");

    if let Some(close_offset) = source[index..].find(&close_tag) {
        let raw_text = decode_entities(&source[index..index + close_offset]);
        if !raw_text.trim().is_empty() {
            tokens.push(Token::Text(raw_text.trim().to_string()));
        }
        tokens.push(Token::CloseTag(tag_name.to_string()));
        index + close_offset + close_tag.len()
    } else {
        let raw_text = decode_entities(&source[index..]);
        if !raw_text.trim().is_empty() {
            tokens.push(Token::Text(raw_text.trim().to_string()));
        }
        source.len()
    }
}
