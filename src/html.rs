use crate::dom::Node;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    OpenTag(TagToken),
    CloseTag(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TagToken {
    tag_name: String,
    attributes: BTreeMap<String, String>,
}

pub struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    #[allow(dead_code)]
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            tokens: tokenize(source),
            position: 0,
            source,
        }
    }

    pub fn parse_document(&mut self) -> Node {
        let mut children = Vec::new();

        while !self.is_eof() {
            if let Some(node) = self.parse_node() {
                children.push(node);
            } else {
                self.position += 1;
            }
        }

        Node::document(children)
    }

    fn parse_node(&mut self) -> Option<Node> {
        match self.peek()? {
            Token::Text(text) => {
                let text = text.clone();
                self.position += 1;
                Some(Node::text(text))
            }
            Token::OpenTag(tag) => {
                let tag = tag.clone();
                self.position += 1;
                if is_void_tag(&tag.tag_name) {
                    Some(Node::element_with_attributes(
                        tag.tag_name,
                        tag.attributes,
                        Vec::new(),
                    ))
                } else {
                    Some(self.parse_element(tag))
                }
            }
            Token::CloseTag(_) => None,
        }
    }

    fn parse_element(&mut self, tag: TagToken) -> Node {
        let mut children = Vec::new();

        while let Some(token) = self.peek() {
            match token {
                Token::CloseTag(close_tag) if close_tag == &tag.tag_name => {
                    self.position += 1;
                    break;
                }
                Token::CloseTag(_) => break,
                _ => {
                    if let Some(node) = self.parse_node() {
                        children.push(node);
                    }
                }
            }
        }

        Node::element_with_attributes(tag.tag_name, tag.attributes, children)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text_buffer = String::new();
    let mut index = 0;

    while index < source.len() {
        let rest = &source[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch == '<' {
            if !text_buffer.trim().is_empty() {
                tokens.push(Token::Text(text_buffer.trim().to_string()));
            }
            text_buffer.clear();

            let Some(tag_end_offset) = rest.find('>') else {
                text_buffer.push(ch);
                index += ch.len_utf8();
                continue;
            };

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
                    let close_tag = format!("</{tag_name}>");
                    if let Some(close_offset) = source[index..].find(&close_tag) {
                        let raw_text = &source[index..index + close_offset];
                        if !raw_text.trim().is_empty() {
                            tokens.push(Token::Text(raw_text.trim().to_string()));
                        }
                        tokens.push(Token::CloseTag(tag_name));
                        index += close_offset + close_tag.len();
                    } else {
                        let raw_text = &source[index..];
                        if !raw_text.trim().is_empty() {
                            tokens.push(Token::Text(raw_text.trim().to_string()));
                        }
                        break;
                    }
                }
            }
        } else {
            text_buffer.push(ch);
            index += ch.len_utf8();
        }
    }

    if !text_buffer.trim().is_empty() {
        tokens.push(Token::Text(text_buffer.trim().to_string()));
    }

    tokens
}

fn parse_open_tag(source: &str) -> TagToken {
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

fn is_raw_text_tag(tag_name: &str) -> bool {
    matches!(tag_name, "script" | "style")
}

fn is_void_tag(tag_name: &str) -> bool {
    matches!(tag_name, "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr")
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
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }

            if index >= chars.len() {
                String::new()
            } else if chars[index] == '"' || chars[index] == '\'' {
                let quote = chars[index];
                index += 1;
                let value_start = index;
                while index < chars.len() && chars[index] != quote {
                    index += 1;
                }
                let value = chars[value_start..index].iter().collect::<String>();
                if index < chars.len() {
                    index += 1;
                }
                value
            } else {
                let value_start = index;
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                chars[value_start..index].iter().collect::<String>()
            }
        } else {
            String::new()
        };

        attributes.insert(name, value);
    }

    attributes
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::dom::Node;
    use std::collections::BTreeMap;

    #[test]
    fn parses_nested_html_into_dom_tree() {
        let mut parser = Parser::new("<html><body><p>Hello</p><p>World</p></body></html>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "html",
                vec![Node::element(
                    "body",
                    vec![
                        Node::element("p", vec![Node::text("Hello")]),
                        Node::element("p", vec![Node::text("World")]),
                    ],
                )],
            )])
        );
    }

    #[test]
    fn ignores_whitespace_only_text_nodes() {
        let mut parser = Parser::new("<div>\n  <p>Text</p>\n</div>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "div",
                vec![Node::element("p", vec![Node::text("Text")])],
            )])
        );
    }

    #[test]
    fn keeps_script_contents_as_raw_text() {
        let mut parser = Parser::new("<script>if (a < b) { run(); }</script>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "script",
                vec![Node::text("if (a < b) { run(); }")],
            )])
        );
    }

    #[test]
    fn keeps_mismatched_closing_tag_for_parent_recovery() {
        let mut parser = Parser::new("<div><span>t</div><p>x</p>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![
                Node::element(
                    "div",
                    vec![Node::element("span", vec![Node::text("t")])],
                ),
                Node::element("p", vec![Node::text("x")]),
            ])
        );
    }

    #[test]
    fn treats_img_as_void_element() {
        let mut parser = Parser::new("<div><img src=\"cat.txt\" alt=\"sleepy cat\"><p>caption</p></div>");
        let document = parser.parse_document();

        let mut img_attributes = BTreeMap::new();
        img_attributes.insert("alt".to_string(), "sleepy cat".to_string());
        img_attributes.insert("src".to_string(), "cat.txt".to_string());

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "div",
                vec![
                    Node::element_with_attributes("img", img_attributes, Vec::new()),
                    Node::element("p", vec![Node::text("caption")]),
                ],
            )])
        );
    }

    #[test]
    fn preserves_tag_attributes() {
        let mut parser = Parser::new(
            r#"<div id="app" class="shell main"><p data-role=hero hidden>Hello</p></div>"#,
        );
        let document = parser.parse_document();

        let mut div_attributes = BTreeMap::new();
        div_attributes.insert("class".to_string(), "shell main".to_string());
        div_attributes.insert("id".to_string(), "app".to_string());

        let mut p_attributes = BTreeMap::new();
        p_attributes.insert("data-role".to_string(), "hero".to_string());
        p_attributes.insert("hidden".to_string(), String::new());

        assert_eq!(
            document,
            Node::document(vec![Node::element_with_attributes(
                "div",
                div_attributes,
                vec![Node::element_with_attributes(
                    "p",
                    p_attributes,
                    vec![Node::text("Hello")],
                )],
            )])
        );
    }
}
